//! Bounded, cancellable IO for file transfers relayed through this client.
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

pub struct RelayRequest {
    pub path: String,
    pub offset: u64,
    pub length: u64,
    pub upload: bool,
}

pub async fn copy_exact<R, W>(
    reader: &mut R,
    writer: &mut W,
    length: u64,
    cancel: &CancellationToken,
    io_timeout: Duration,
) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut remaining = length;
    let mut buffer = vec![0; super::TRANSFER_IO_BUFFER_BYTES];
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("transfer canceled".into()),
            result = tokio::time::timeout(io_timeout, reader.read(&mut buffer[..limit])) => result.map_err(|_| "读取中转数据超时")?.map_err(|e| e.to_string())?,
        };
        if count == 0 {
            return Err("中转源提前结束，断点已保留".into());
        }
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("transfer canceled".into()),
            result = tokio::time::timeout(io_timeout, writer.write_all(&buffer[..count])) => result.map_err(|_| "写入中转数据超时")?.map_err(|e| e.to_string())?,
        }
        remaining -= count as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
