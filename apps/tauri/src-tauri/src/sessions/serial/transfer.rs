//! Serial file-transfer protocols.
//!
//! The worker owns the port while a transfer is active. Keeping the protocol
//! state machine here prevents XMODEM/YMODEM retries from blocking ordinary
//! serial-control code and makes the checksum/frame rules testable without a
//! physical adapter.

use std::path::Path;
use std::time::Duration;

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::{
    SerialTransferDirection, SerialTransferMode, SerialTransferRequest, SerialTransferResult,
};

const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const EOT: u8 = 0x04;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
const CRC_REQUEST: u8 = b'C';
const PAD: u8 = 0x1a;
const BLOCK_TIMEOUT: Duration = Duration::from_secs(10);
const START_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 10;
const RAW_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) async fn execute<S>(
    stream: &mut S,
    request: SerialTransferRequest,
    cancellation: CancellationToken,
) -> Result<SerialTransferResult, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let path = request.local_path.clone();
    let bytes_transferred = match (request.direction, request.mode) {
        (SerialTransferDirection::Send, SerialTransferMode::Raw) => {
            send_raw(stream, Path::new(&path), &cancellation).await?
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Raw) => {
            receive_raw(stream, Path::new(&path), &cancellation).await?
        }
        (SerialTransferDirection::Send, SerialTransferMode::Xmodem) => {
            send_xmodem(stream, Path::new(&path), false, &cancellation).await?
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Xmodem) => {
            receive_xmodem(stream, Path::new(&path), &cancellation).await?
        }
        (SerialTransferDirection::Send, SerialTransferMode::Ymodem) => {
            send_ymodem(stream, Path::new(&path), &cancellation).await?
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Ymodem) => {
            receive_ymodem(stream, Path::new(&path), &cancellation).await?
        }
    };
    Ok(SerialTransferResult {
        bytes_transferred,
        local_path: path,
    })
}

async fn send_raw<S>(
    stream: &mut S,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncWrite + Unpin,
{
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut buffer, cancellation).await?;
        if count == 0 {
            break;
        }
        write_all(stream, &buffer[..count], cancellation).await?;
        total += count as u64;
    }
    flush(stream, cancellation).await?;
    Ok(total)
}

async fn receive_raw<S>(
    stream: &mut S,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + Unpin,
{
    let mut file = create_target(path).await?;
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let read = tokio::select! {
            _ = cancellation.cancelled() => return Err("串口接收已取消".to_string()),
            result = timeout(RAW_IDLE_TIMEOUT, stream.read(&mut buffer)) => result
                .map_err(|_| String::new())
                .and_then(|result| result.map_err(|error| error.to_string())),
        };
        let count = match read {
            Ok(count) => count,
            Err(error) if error.is_empty() => break,
            Err(error) => return Err(format!("串口接收失败：{error}")),
        };
        if count == 0 {
            break;
        }
        write_file(&mut file, &buffer[..count], cancellation).await?;
        total += count as u64;
    }
    file.flush()
        .await
        .map_err(|error| format!("无法保存串口接收文件：{error}"))?;
    Ok(total)
}

async fn send_xmodem<S>(
    stream: &mut S,
    path: &Path,
    use_crc: bool,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let negotiated_crc = wait_for_sender_start(stream, cancellation).await?;
    let use_crc = use_crc || negotiated_crc;
    send_blocks(stream, path, 128, use_crc, cancellation).await
}

async fn send_ymodem<S>(
    stream: &mut S,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let use_crc = wait_for_sender_start(stream, cancellation).await?;
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "串口 YMODEM 文件名无效".to_string())?;
    let mut header = vec![0_u8; 128];
    let header_text = format!("{}\0{}\0", file_name, metadata.len());
    let header_bytes = header_text.as_bytes();
    let header_len = header_bytes.len().min(header.len());
    header[..header_len].copy_from_slice(&header_bytes[..header_len]);
    send_packet(stream, 0, &header, use_crc, cancellation).await?;
    // The receiver sends another C after ACKing the block-0 metadata.
    let _ = wait_for_sender_start(stream, cancellation).await?;
    send_blocks(stream, path, 1024, use_crc, cancellation).await
}

async fn send_blocks<S>(
    stream: &mut S,
    path: &Path,
    block_size: usize,
    use_crc: bool,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    let mut block = vec![PAD; block_size];
    let mut sequence = 1_u8;
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut block, cancellation).await?;
        if count == 0 {
            break;
        }
        block[count..].fill(PAD);
        send_packet(stream, sequence, &block, use_crc, cancellation).await?;
        total += count as u64;
        sequence = sequence.wrapping_add(1);
        block.fill(PAD);
    }
    send_eot(stream, cancellation).await?;
    Ok(total)
}

async fn receive_xmodem<S>(
    stream: &mut S,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = create_target(path).await?;
    write_all(stream, &[CRC_REQUEST], cancellation).await?;
    let mut expected = 1_u8;
    let mut pending_last: Option<Vec<u8>> = None;
    let mut total = 0_u64;
    loop {
        let Some(control) = read_byte(stream, BLOCK_TIMEOUT, cancellation).await? else {
            return Err("等待 XMODEM 数据超时".to_string());
        };
        match control {
            EOT => {
                write_all(stream, &[ACK], cancellation).await?;
                if let Some(mut last) = pending_last.take() {
                    while last.last() == Some(&PAD) {
                        last.pop();
                    }
                    write_file(&mut file, &last, cancellation).await?;
                    total += last.len() as u64;
                }
                break;
            }
            SOH | STX => {
                let block_size = if control == SOH { 128 } else { 1024 };
                let Some((sequence, payload)) =
                    read_packet_tail(stream, block_size, true, cancellation).await?
                else {
                    write_all(stream, &[NAK], cancellation).await?;
                    continue;
                };
                if sequence == expected {
                    if let Some(previous) = pending_last.replace(payload) {
                        write_file(&mut file, &previous, cancellation).await?;
                        total += previous.len() as u64;
                    }
                    expected = expected.wrapping_add(1);
                    write_all(stream, &[ACK], cancellation).await?;
                } else if sequence == expected.wrapping_sub(1) {
                    // The ACK may have been lost; acknowledge a duplicate
                    // without writing it twice.
                    write_all(stream, &[ACK], cancellation).await?;
                } else {
                    write_all(stream, &[CAN, CAN], cancellation).await?;
                    return Err("XMODEM 数据块序号不连续".to_string());
                }
            }
            CAN => return Err("对端取消了 XMODEM 传输".to_string()),
            _ => {}
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("无法保存 XMODEM 文件：{error}"))?;
    Ok(total)
}

async fn receive_ymodem<S>(
    stream: &mut S,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = create_target(path).await?;
    write_all(stream, &[CRC_REQUEST], cancellation).await?;
    let Some(control) = read_byte(stream, START_TIMEOUT, cancellation).await? else {
        return Err("等待 YMODEM 文件头超时".to_string());
    };
    if control != SOH {
        return Err("YMODEM 文件头格式无效".to_string());
    }
    let Some((_sequence, header)) = read_packet_tail(stream, 128, true, cancellation).await? else {
        return Err("YMODEM 文件头校验失败".to_string());
    };
    let size = parse_ymodem_size(&header)?;
    write_all(stream, &[ACK, CRC_REQUEST], cancellation).await?;
    let mut expected = 1_u8;
    let mut remaining = size;
    let mut total = 0_u64;
    loop {
        let Some(control) = read_byte(stream, BLOCK_TIMEOUT, cancellation).await? else {
            return Err("等待 YMODEM 数据超时".to_string());
        };
        match control {
            EOT => {
                write_all(stream, &[ACK], cancellation).await?;
                break;
            }
            SOH | STX => {
                let block_size = if control == SOH { 128 } else { 1024 };
                let Some((sequence, payload)) =
                    read_packet_tail(stream, block_size, true, cancellation).await?
                else {
                    write_all(stream, &[NAK], cancellation).await?;
                    continue;
                };
                if sequence != expected {
                    if sequence == expected.wrapping_sub(1) {
                        write_all(stream, &[ACK], cancellation).await?;
                        continue;
                    }
                    write_all(stream, &[CAN, CAN], cancellation).await?;
                    return Err("YMODEM 数据块序号不连续".to_string());
                }
                let count = (remaining as usize).min(payload.len());
                write_file(&mut file, &payload[..count], cancellation).await?;
                remaining -= count as u64;
                total += count as u64;
                expected = expected.wrapping_add(1);
                write_all(stream, &[ACK], cancellation).await?;
            }
            CAN => return Err("对端取消了 YMODEM 传输".to_string()),
            _ => {}
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("无法保存 YMODEM 文件：{error}"))?;
    Ok(total)
}

async fn wait_for_sender_start<S>(
    stream: &mut S,
    cancellation: &CancellationToken,
) -> Result<bool, String>
where
    S: AsyncRead + Unpin,
{
    for _ in 0..3 {
        if let Some(value) = read_byte(stream, START_TIMEOUT, cancellation).await? {
            match value {
                CRC_REQUEST => return Ok(true),
                NAK => return Ok(false),
                CAN => return Err("对端取消了串口文件传输".to_string()),
                _ => {}
            }
        }
    }
    Err("等待串口文件接收端启动超时".to_string())
}

async fn send_packet<S>(
    stream: &mut S,
    sequence: u8,
    payload: &[u8],
    use_crc: bool,
    cancellation: &CancellationToken,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let marker = if payload.len() == 1024 { STX } else { SOH };
    let mut packet = Vec::with_capacity(payload.len() + 5);
    packet.extend_from_slice(&[marker, sequence, 255_u8.wrapping_sub(sequence)]);
    packet.extend_from_slice(payload);
    if use_crc {
        packet.extend_from_slice(&crc16(payload).to_be_bytes());
    } else {
        packet.push(checksum(payload));
    }

    for _ in 0..MAX_RETRIES {
        write_all(stream, &packet, cancellation).await?;
        flush(stream, cancellation).await?;
        match read_byte(stream, BLOCK_TIMEOUT, cancellation).await? {
            Some(ACK) => return Ok(()),
            Some(CAN) => return Err("对端取消了串口文件传输".to_string()),
            Some(NAK) | Some(_) | None => {}
        }
    }
    Err(format!("串口数据块 {sequence} 重试次数过多"))
}

async fn send_eot<S>(stream: &mut S, cancellation: &CancellationToken) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for _ in 0..MAX_RETRIES {
        write_all(stream, &[EOT], cancellation).await?;
        flush(stream, cancellation).await?;
        match read_byte(stream, BLOCK_TIMEOUT, cancellation).await? {
            Some(ACK) => return Ok(()),
            Some(CAN) => return Err("对端取消了串口文件传输".to_string()),
            Some(NAK) | Some(_) | None => {}
        }
    }
    Err("串口文件传输结束确认超时".to_string())
}

async fn read_packet_tail<S>(
    stream: &mut S,
    block_size: usize,
    use_crc: bool,
    cancellation: &CancellationToken,
) -> Result<Option<(u8, Vec<u8>)>, String>
where
    S: AsyncRead + Unpin,
{
    let Some(sequence) = read_byte(stream, BLOCK_TIMEOUT, cancellation).await? else {
        return Ok(None);
    };
    let Some(inverse) = read_byte(stream, BLOCK_TIMEOUT, cancellation).await? else {
        return Ok(None);
    };
    if sequence ^ inverse != 0xff {
        return Ok(None);
    }
    let mut payload = vec![0_u8; block_size];
    if !read_exact_with_timeout(stream, &mut payload, BLOCK_TIMEOUT, cancellation).await? {
        return Ok(None);
    }
    if use_crc {
        let mut trailer = [0_u8; 2];
        if !read_exact_with_timeout(stream, &mut trailer, BLOCK_TIMEOUT, cancellation).await? {
            return Ok(None);
        }
        if crc16(&payload) != u16::from_be_bytes(trailer) {
            return Ok(None);
        }
    } else {
        let Some(value) = read_byte(stream, BLOCK_TIMEOUT, cancellation).await? else {
            return Ok(None);
        };
        if checksum(&payload) != value {
            return Ok(None);
        }
    }
    Ok(Some((sequence, payload)))
}

fn parse_ymodem_size(header: &[u8]) -> Result<u64, String> {
    let end = header
        .iter()
        .position(|value| *value == 0)
        .ok_or_else(|| "YMODEM 文件头缺少文件名".to_string())?;
    let size_start = end + 1;
    let size_end = header[size_start..]
        .iter()
        .position(|value| *value == 0)
        .map(|offset| size_start + offset)
        .unwrap_or(header.len());
    if size_start == size_end {
        return Err("YMODEM 文件头缺少文件大小".to_string());
    }
    std::str::from_utf8(&header[size_start..size_end])
        .map_err(|_| "YMODEM 文件大小不是有效文本".to_string())?
        .parse::<u64>()
        .map_err(|_| "YMODEM 文件大小无效".to_string())
}

async fn read_byte<S>(
    stream: &mut S,
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<Option<u8>, String>
where
    S: AsyncRead + Unpin,
{
    let mut byte = [0_u8; 1];
    if !read_exact_with_timeout(stream, &mut byte, wait, cancellation).await? {
        return Ok(None);
    }
    Ok(Some(byte[0]))
}

async fn read_exact_with_timeout<S>(
    stream: &mut S,
    buffer: &mut [u8],
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<bool, String>
where
    S: AsyncRead + Unpin,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = timeout(wait, stream.read_exact(buffer)) => match result {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(error)) => Err(format!("读取串口文件传输数据失败：{error}")),
            Err(_) => Ok(false),
        }
    }
}

async fn read_file(
    file: &mut File,
    buffer: &mut [u8],
    cancellation: &CancellationToken,
) -> Result<usize, String> {
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = file.read(buffer) => result.map_err(|error| format!("读取串口发送文件失败：{error}")),
    }
}

async fn write_all<S>(
    stream: &mut S,
    buffer: &[u8],
    cancellation: &CancellationToken,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = stream.write_all(buffer) => result.map_err(|error| format!("写入串口文件传输数据失败：{error}")),
    }
}

async fn flush<S>(stream: &mut S, cancellation: &CancellationToken) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = stream.flush() => result.map_err(|error| format!("刷新串口文件传输数据失败：{error}")),
    }
}

async fn create_target(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .await
        .map_err(|error| format!("无法创建串口接收文件：{error}"))
}

async fn write_file(
    file: &mut File,
    buffer: &[u8],
    cancellation: &CancellationToken,
) -> Result<(), String> {
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = file.write_all(buffer) => result.map_err(|error| format!("保存串口接收文件失败：{error}")),
    }
}

fn checksum(payload: &[u8]) -> u8 {
    payload
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
}

fn crc16(payload: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in payload {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::io::duplex;
    use tokio_util::sync::CancellationToken;

    use super::{checksum, crc16, parse_ymodem_size, receive_xmodem, send_xmodem};

    #[test]
    fn calculates_xmodem_checksums() {
        assert_eq!(checksum(b"123456789"), 0xdd);
        assert_eq!(crc16(b"123456789"), 0x31c3);
    }

    #[test]
    fn parses_ymodem_header_size() {
        let mut header = [0_u8; 128];
        let bytes = [
            b'f', b'i', b'l', b'e', b'.', b'b', b'i', b'n', 0, b'1', b'2', b'3', 0,
        ];
        header[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_ymodem_size(&header).unwrap(), 123);
        assert!(parse_ymodem_size(b"file.bin").is_err());
    }

    #[tokio::test]
    async fn xmodem_round_trip_works_without_a_physical_device() {
        let source = temporary_path("source");
        let target = temporary_path("target");
        let bytes = (0..251_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let receiver_target = target.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            send_xmodem(&mut sender_stream, &sender_source, false, &cancellation).await
        });
        let receiver = tokio::spawn(async move {
            receive_xmodem(
                &mut receiver_stream,
                &receiver_target,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(sender.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(receiver.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(tokio::fs::read(&target).await.unwrap(), bytes);
        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_file(target).await;
    }

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fileterm-serial-{label}-{}", uuid::Uuid::new_v4()))
    }
}
