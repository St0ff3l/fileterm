use std::{future::Future, io};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio_util::sync::CancellationToken;

use super::pacing::{write_serial_bytes, SerialPacing};

pub(super) async fn write_while_receiving<R, W, F, O>(
    reader: &mut R,
    writer: &mut W,
    bytes: &[u8],
    cancellation: &CancellationToken,
    pacing: SerialPacing,
    mut receive: F,
) -> io::Result<bool>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
    F: FnMut(Vec<u8>) -> O,
    O: Future<Output = io::Result<()>>,
{
    let write = write_serial_bytes(writer, bytes, cancellation, pacing);
    tokio::pin!(write);
    let mut buffer = [0_u8; 4096];
    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(false),
            result = &mut write => return result,
            read = reader.read(&mut buffer) => {
                let count = read?;
                if count == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "串口设备已断开"));
                }
                receive(buffer[..count].to_vec()).await?;
            }
        }
    }
}

#[cfg(test)]
mod tests;
