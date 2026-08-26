//! Shared X/YMODEM frame I/O and protocol control helpers.
//!
//! Keeping framing separate from file orchestration makes timeout recovery,
//! checksum validation, and cancellation behavior reusable by each transfer
//! mode without growing one protocol switchboard file.

use std::path::Path;
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::file_safety::{is_safe_transfer_file_name, StagedReceiveFile};
use super::super::timing::SerialTransferTiming;

pub(super) const SOH: u8 = 0x01;
pub(super) const STX: u8 = 0x02;
pub(super) const EOT: u8 = 0x04;
pub(super) const ACK: u8 = 0x06;
pub(super) const NAK: u8 = 0x15;
pub(super) const CAN: u8 = 0x18;
pub(super) const CRC_REQUEST: u8 = b'C';
pub(super) const PAD: u8 = 0x1a;
pub(super) const MAX_RETRIES: usize = 10;

pub(super) async fn receive_protocol_start<S>(
    stream: &mut S,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<(u8, bool), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // X/YMODEM receivers conventionally try CRC first and fall back to the
    // original checksum handshake for bootloaders that do not understand C.
    for attempt in 0..MAX_RETRIES {
        let use_crc = attempt < 3;
        write_all(
            stream,
            &[if use_crc { CRC_REQUEST } else { NAK }],
            timing.write_timeout,
            cancellation,
        )
        .await?;
        flush(stream, timing.write_timeout, cancellation).await?;
        if let Some(value) = read_byte(stream, timing.control_timeout(), cancellation).await? {
            match value {
                SOH | STX => return Ok((value, use_crc)),
                CAN => return Err("对端取消了串口文件传输".to_string()),
                _ => {}
            }
        }
    }
    Err("等待串口文件传输启动超时".to_string())
}

pub(super) async fn read_next_protocol_byte<S>(
    stream: &mut S,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<u8, String>
where
    S: AsyncRead + Unpin,
{
    read_byte(stream, timing.control_timeout(), cancellation)
        .await?
        .ok_or_else(|| "等待串口文件传输数据超时".to_string())
}

pub(super) async fn wait_for_sender_start<S>(
    stream: &mut S,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<bool, String>
where
    S: AsyncRead + Unpin,
{
    for _ in 0..3 {
        if let Some(value) = read_byte(stream, timing.control_timeout(), cancellation).await? {
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

pub(super) async fn send_packet<S>(
    stream: &mut S,
    sequence: u8,
    payload: &[u8],
    use_crc: bool,
    timing: SerialTransferTiming,
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
        write_all(stream, &packet, timing.write_timeout, cancellation).await?;
        flush(stream, timing.write_timeout, cancellation).await?;
        match read_byte(stream, timing.control_timeout(), cancellation).await? {
            Some(ACK) => return Ok(()),
            Some(CAN) => return Err("对端取消了串口文件传输".to_string()),
            Some(NAK) | Some(_) | None => {}
        }
    }
    Err(format!("串口数据块 {sequence} 重试次数过多"))
}

pub(super) async fn send_eot<S>(
    stream: &mut S,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for _ in 0..MAX_RETRIES {
        write_all(stream, &[EOT], timing.write_timeout, cancellation).await?;
        flush(stream, timing.write_timeout, cancellation).await?;
        match read_byte(stream, timing.control_timeout(), cancellation).await? {
            Some(ACK) => return Ok(()),
            Some(CAN) => return Err("对端取消了串口文件传输".to_string()),
            Some(NAK) | Some(_) | None => {}
        }
    }
    Err("串口文件传输结束确认超时".to_string())
}

pub(super) async fn cancel_protocol<S>(stream: &mut S)
where
    S: AsyncWrite + Unpin,
{
    let _ = timeout(Duration::from_millis(100), async {
        let _ = stream.write_all(&[CAN, CAN]).await;
        let _ = stream.flush().await;
    })
    .await;
}

pub(super) async fn read_packet_tail<S>(
    stream: &mut S,
    block_size: usize,
    use_crc: bool,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<Option<(u8, Vec<u8>)>, String>
where
    S: AsyncRead + Unpin,
{
    let packet_timeout = timing.packet_timeout(block_size, if use_crc { 2 } else { 1 });
    read_packet_tail_with_timeout(stream, block_size, use_crc, packet_timeout, cancellation).await
}

/// Read everything after SOH/STX as one frame. If a frame times out after
/// consuming only part of it, make one second bounded drain attempt before
/// asking the sender to retry. This is important on slow UARTs: cancelling a
/// `read_exact` future can leave the rest of the old frame in the driver
/// buffer, and the next retry would otherwise interpret those bytes as a new
/// sequence number.
pub(crate) async fn read_packet_tail_with_timeout<S>(
    stream: &mut S,
    block_size: usize,
    use_crc: bool,
    packet_timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<Option<(u8, Vec<u8>)>, String>
where
    S: AsyncRead + Unpin,
{
    let trailer_len = if use_crc { 2 } else { 1 };
    let mut frame = vec![0_u8; 2 + block_size + trailer_len];
    let filled =
        match read_exact_with_timeout(stream, &mut frame, packet_timeout, cancellation).await? {
            ReadExactResult::Complete => frame.len(),
            ReadExactResult::Partial(filled) => {
                // The first timeout may have consumed a prefix. Continue
                // draining the same expected frame, but never let a broken
                // sender block a retry indefinitely.
                match read_exact_with_timeout(
                    stream,
                    &mut frame[filled..],
                    packet_timeout,
                    cancellation,
                )
                .await?
                {
                    ReadExactResult::Complete => frame.len(),
                    ReadExactResult::Partial(_) => return Ok(None),
                }
            }
        };

    debug_assert_eq!(filled, frame.len());
    let sequence = frame[0];
    let inverse = frame[1];
    if sequence ^ inverse != 0xff {
        // The complete malformed frame has already been consumed, so the
        // caller can safely NAK and wait for a fresh SOH/STX marker.
        return Ok(None);
    }
    let payload_end = 2 + block_size;
    let payload = frame[2..payload_end].to_vec();
    if use_crc {
        if crc16(&payload) != u16::from_be_bytes([frame[payload_end], frame[payload_end + 1]]) {
            return Ok(None);
        }
    } else {
        let value = frame[payload_end];
        if checksum(&payload) != value {
            return Ok(None);
        }
    }
    Ok(Some((sequence, payload)))
}

pub(super) fn parse_ymodem_header(header: &[u8]) -> Result<Option<(String, u64)>, String> {
    let end = header
        .iter()
        .position(|value| *value == 0)
        .ok_or_else(|| "YMODEM 文件头缺少文件名".to_string())?;
    if end == 0 {
        return Ok(None);
    }
    let file_name =
        std::str::from_utf8(&header[..end]).map_err(|_| "YMODEM 文件名不是有效文本".to_string())?;
    if !is_safe_transfer_file_name(file_name) {
        return Err("YMODEM 文件名无效，不允许包含路径或控制字符".to_string());
    }
    let size = parse_ymodem_size(header)?;
    Ok(Some((file_name.to_string(), size)))
}

pub(crate) fn parse_ymodem_size(header: &[u8]) -> Result<u64, String> {
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

pub(super) fn ymodem_target(
    directory: &Path,
    file_name: &str,
) -> Result<std::path::PathBuf, String> {
    if !is_safe_transfer_file_name(file_name) {
        return Err("YMODEM 文件名无效，不允许写出接收目录".to_string());
    }
    Ok(directory.join(file_name))
}

pub(super) async fn read_byte<S>(
    stream: &mut S,
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<Option<u8>, String>
where
    S: AsyncRead + Unpin,
{
    let mut byte = [0_u8; 1];
    match read_exact_with_timeout(stream, &mut byte, wait, cancellation).await? {
        ReadExactResult::Complete => Ok(Some(byte[0])),
        ReadExactResult::Partial(_) => Ok(None),
    }
}

enum ReadExactResult {
    Complete,
    Partial(usize),
}

async fn read_exact_with_timeout<S>(
    stream: &mut S,
    buffer: &mut [u8],
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<ReadExactResult, String>
where
    S: AsyncRead + Unpin,
{
    let deadline = tokio::time::Instant::now() + wait;
    let mut filled = 0;
    while filled < buffer.len() {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Ok(ReadExactResult::Partial(filled));
        }
        let result = tokio::select! {
            _ = cancellation.cancelled() => return Err("串口文件传输已取消".to_string()),
            result = timeout(remaining, stream.read(&mut buffer[filled..])) => result,
        };
        match result {
            Ok(Ok(0)) => return Err("读取串口文件传输数据失败：串口流提前结束".to_string()),
            Ok(Ok(count)) => filled += count,
            Ok(Err(error)) => return Err(format!("读取串口文件传输数据失败：{error}")),
            Err(_) => return Ok(ReadExactResult::Partial(filled)),
        }
    }
    Ok(ReadExactResult::Complete)
}

pub(super) async fn read_file(
    file: &mut File,
    buffer: &mut [u8],
    cancellation: &CancellationToken,
) -> Result<usize, String> {
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = file.read(buffer) => result.map_err(|error| format!("读取串口发送文件失败：{error}")),
    }
}

pub(crate) async fn write_all<S>(
    stream: &mut S,
    buffer: &[u8],
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = timeout(wait, stream.write_all(buffer)) => match result {
            Ok(result) => result.map_err(|error| format!("写入串口文件传输数据失败：{error}")),
            Err(_) => Err("串口文件传输写入等待硬件流控超时".to_string()),
        },
    }
}

pub(crate) async fn flush<S>(
    stream: &mut S,
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<(), String>
where
    S: AsyncWrite + Unpin,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("串口文件传输已取消".to_string()),
        result = timeout(wait, stream.flush()) => match result {
            Ok(result) => result.map_err(|error| format!("刷新串口文件传输数据失败：{error}")),
            Err(_) => Err("串口文件传输刷新等待硬件流控超时".to_string()),
        },
    }
}

pub(crate) async fn create_target(
    path: &Path,
    max_bytes: u64,
) -> Result<StagedReceiveFile, String> {
    StagedReceiveFile::create(path, max_bytes).await
}

pub(crate) fn checksum(payload: &[u8]) -> u8 {
    payload
        .iter()
        .fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
}

pub(crate) fn crc16(payload: &[u8]) -> u16 {
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
