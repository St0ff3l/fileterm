//! Classic, short-packet Kermit adapter.
//!
//! This intentionally implements the required stop-and-wait core (S/F/D/Z/B,
//! Y/N) with control and 8-bit quoting. Long packets, sliding windows,
//! repeat-count compression, server commands, and attribute negotiation are
//! left disabled so a peer can fall back to the lowest common denominator.
//! The packet layout and checksum follow the Kermit Project's basic packet
//! format rather than copying an implementation from a terminal emulator.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::SerialTransferRequest;
use super::progress::SerialTransferReporter;
use super::timing::SerialTransferTiming;
use super::transfer::{create_target, flush, is_safe_transfer_file_name, write_all};

const MARK: u8 = 0x01;
const EOL: u8 = b'\r';
const CONTROL_QUOTE: u8 = b'#';
const EIGHT_BIT_QUOTE: u8 = b'&';
const MAX_PACKET_VALUE: usize = 94;
const MAX_DATA_BYTES: usize = 32;
const MAX_RETRIES: usize = 10;

#[derive(Debug)]
struct Packet {
    sequence: u8,
    kind: u8,
    data: Vec<u8>,
}

struct ReceiveFile {
    file: File,
    path: PathBuf,
}

pub(super) async fn send<S>(
    stream: &mut S,
    request: &SerialTransferRequest,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let paths = if request.local_paths.is_empty() {
        vec![request.local_path.clone()]
    } else {
        request.local_paths.clone()
    };
    let mut files = Vec::with_capacity(paths.len());
    let mut total_size = 0_u64;
    for path in paths {
        let path = PathBuf::from(path);
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|error| format!("无法读取 Kermit 发送文件：{error}"))?;
        if !metadata.is_file() {
            return Err("Kermit 发送路径不是文件".to_string());
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| is_safe_transfer_file_name(value))
            .ok_or_else(|| "Kermit 发送文件名无效".to_string())?
            .to_string();
        total_size = total_size
            .checked_add(metadata.len())
            .ok_or_else(|| "Kermit 文件总大小超出支持范围".to_string())?;
        files.push((path, name, metadata.len()));
    }
    if files.is_empty() {
        return Err("Kermit 至少需要一个发送文件".to_string());
    }
    reporter.set_total(Some(total_size));

    let local_init = send_init_packet(0)?;
    let peer_init = send_and_wait_ack(stream, &local_init, 0, timing, cancellation).await?;
    let use_eight_bit_quote = negotiated_eight_bit_quote(&peer_init);
    let mut sequence = 1_u8;
    let mut bytes_transferred = 0_u64;

    for (path, name, _) in &files {
        let file_header = make_packet(sequence, b'F', name.as_bytes(), true, use_eight_bit_quote)?;
        send_and_wait_ack(stream, &file_header, sequence, timing, cancellation).await?;
        sequence = next_sequence(sequence);

        let mut file = File::open(path)
            .await
            .map_err(|error| format!("无法打开 Kermit 发送文件：{error}"))?;
        loop {
            let mut source = vec![0_u8; MAX_DATA_BYTES];
            let count = file
                .read(&mut source)
                .await
                .map_err(|error| format!("读取 Kermit 发送文件失败：{error}"))?;
            if count == 0 {
                break;
            }
            let packet = make_packet(sequence, b'D', &source[..count], true, use_eight_bit_quote)?;
            send_and_wait_ack(stream, &packet, sequence, timing, cancellation).await?;
            sequence = next_sequence(sequence);
            bytes_transferred = bytes_transferred.saturating_add(count as u64);
            reporter.report(bytes_transferred, Some(u64::from(sequence.wrapping_sub(1))));
        }

        let eof = make_packet(sequence, b'Z', &[], false, use_eight_bit_quote)?;
        send_and_wait_ack(stream, &eof, sequence, timing, cancellation).await?;
        sequence = next_sequence(sequence);
    }

    let break_packet = make_packet(sequence, b'B', &[], false, use_eight_bit_quote)?;
    send_and_wait_ack(stream, &break_packet, sequence, timing, cancellation).await?;
    Ok(bytes_transferred)
}

pub(super) async fn receive<S>(
    stream: &mut S,
    directory: &Path,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !directory.is_dir() {
        return Err("Kermit 接收目录不存在".to_string());
    }

    // In basic Kermit the receiver waits for S; the sender initiates the
    // transaction. This avoids both sides emitting S at once when two
    // FileTerm tabs are connected through a null-modem pair.
    let mut packet = wait_for_packet(stream, timing, cancellation).await?;
    let use_eight_bit_quote;
    loop {
        match packet.kind {
            b'S' => {
                use_eight_bit_quote = negotiated_eight_bit_quote(&packet.data);
                let ack = ack_init_packet(packet.sequence)?;
                write_all(stream, &ack, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                break;
            }
            _ => {
                let nak = make_packet(packet.sequence, b'N', &[], false, false)?;
                write_all(stream, &nak, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                packet = wait_for_packet(stream, timing, cancellation).await?;
            }
        }
    }

    let mut expected = next_sequence(packet.sequence);
    let mut file: Option<ReceiveFile> = None;
    let mut receive_retries = 0_usize;
    let mut bytes_transferred = 0_u64;
    loop {
        let packet = match read_packet(stream, timing.control_timeout(), cancellation).await {
            Ok(Some(packet)) => {
                receive_retries = 0;
                packet
            }
            Ok(None) => {
                receive_retries = receive_retries.saturating_add(1);
                if receive_retries >= MAX_RETRIES {
                    cleanup_received_file(&mut file).await;
                    return Err("Kermit 等待确认超时，重试次数过多".to_string());
                }
                let nak = make_packet(expected, b'N', &[], false, false)?;
                write_all(stream, &nak, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                continue;
            }
            Err(error) if is_retryable_receive_error(&error) => {
                receive_retries = receive_retries.saturating_add(1);
                if receive_retries >= MAX_RETRIES {
                    cleanup_received_file(&mut file).await;
                    return Err("Kermit 等待确认超时，重试次数过多".to_string());
                }
                let nak = make_packet(expected, b'N', &[], false, false)?;
                write_all(stream, &nak, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                continue;
            }
            Err(error) => {
                cleanup_received_file(&mut file).await;
                return Err(error);
            }
        };

        if packet.sequence == previous_sequence(expected) {
            let ack = make_packet(packet.sequence, b'Y', &[], false, false)?;
            write_all(stream, &ack, timing.write_timeout, cancellation).await?;
            flush(stream, timing.write_timeout, cancellation).await?;
            continue;
        }
        if packet.sequence != expected {
            let nak = make_packet(expected, b'N', &[], false, false)?;
            write_all(stream, &nak, timing.write_timeout, cancellation).await?;
            flush(stream, timing.write_timeout, cancellation).await?;
            continue;
        }

        match packet.kind {
            b'F' => {
                if file.is_some() {
                    cleanup_received_file(&mut file).await;
                    return Err("Kermit 收到新文件头前上一个文件尚未结束".to_string());
                }
                let name = decode_data(&packet.data, use_eight_bit_quote)?;
                let name = std::str::from_utf8(&name)
                    .map_err(|_| "Kermit 文件名不是有效文本".to_string())?;
                if !is_safe_transfer_file_name(name) {
                    return Err("Kermit 文件名无效，不允许写出接收目录".to_string());
                }
                let path = directory.join(name);
                file = Some(ReceiveFile {
                    file: create_target(&path).await?,
                    path,
                });
                let ack = make_packet(packet.sequence, b'Y', &[], false, false)?;
                if let Err(error) =
                    write_all(stream, &ack, timing.write_timeout, cancellation).await
                {
                    cleanup_received_file(&mut file).await;
                    return Err(error);
                }
                if let Err(error) = flush(stream, timing.write_timeout, cancellation).await {
                    cleanup_received_file(&mut file).await;
                    return Err(error);
                }
            }
            b'D' => {
                let target = file
                    .as_mut()
                    .ok_or_else(|| "Kermit 收到数据前没有文件头".to_string())?;
                let bytes = match decode_data(&packet.data, use_eight_bit_quote) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        cleanup_received_file(&mut file).await;
                        return Err(error);
                    }
                };
                let write_result = target.file.write_all(&bytes).await;
                if let Err(error) = write_result {
                    cleanup_received_file(&mut file).await;
                    return Err(format!("保存 Kermit 接收文件失败：{error}"));
                }
                bytes_transferred = bytes_transferred.saturating_add(bytes.len() as u64);
                reporter.report(bytes_transferred, Some(u64::from(packet.sequence)));
                let ack = make_packet(packet.sequence, b'Y', &[], false, false)?;
                if let Err(error) =
                    write_all(stream, &ack, timing.write_timeout, cancellation).await
                {
                    cleanup_received_file(&mut file).await;
                    return Err(error);
                }
                if let Err(error) = flush(stream, timing.write_timeout, cancellation).await {
                    cleanup_received_file(&mut file).await;
                    return Err(error);
                }
            }
            b'Z' => {
                let Some(mut target) = file.take() else {
                    return Err("Kermit 收到文件结束包前没有文件头".to_string());
                };
                if let Err(error) = target.file.flush().await {
                    let path = target.path.clone();
                    drop(target.file);
                    let _ = tokio::fs::remove_file(path).await;
                    return Err(format!("刷新 Kermit 接收文件失败：{error}"));
                }
                let ack = make_packet(packet.sequence, b'Y', &[], false, false)?;
                write_all(stream, &ack, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
            }
            b'B' => {
                if file.is_some() {
                    cleanup_received_file(&mut file).await;
                    return Err("Kermit 会话结束时接收文件尚未结束".to_string());
                }
                let ack = make_packet(packet.sequence, b'Y', &[], false, false)?;
                write_all(stream, &ack, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                return Ok(bytes_transferred);
            }
            b'S' => {
                // A lost ACK can make the sender repeat S. Answering it again
                // is safe and keeps the receiver from treating it as data.
                let ack = ack_init_packet(packet.sequence)?;
                write_all(stream, &ack, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                expected = next_sequence(packet.sequence);
                continue;
            }
            _ => {
                let nak = make_packet(packet.sequence, b'N', &[], false, false)?;
                write_all(stream, &nak, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                continue;
            }
        }
        expected = next_sequence(expected);
    }
}

async fn send_and_wait_ack<S>(
    stream: &mut S,
    packet: &[u8],
    sequence: u8,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    for _ in 0..MAX_RETRIES {
        write_all(stream, packet, timing.write_timeout, cancellation).await?;
        flush(stream, timing.write_timeout, cancellation).await?;
        match read_packet(stream, timing.control_timeout(), cancellation).await {
            Ok(Some(response)) if response.sequence == sequence && response.kind == b'Y' => {
                return Ok(response.data)
            }
            Ok(Some(response)) if response.kind == b'E' => {
                return Err("对端返回 Kermit 错误".to_string())
            }
            Ok(_) | Err(_) => {}
        }
    }
    Err("Kermit 等待确认超时，重试次数过多".to_string())
}

fn send_init_packet(sequence: u8) -> Result<Vec<u8>, String> {
    // MAXL=80, TIME=5, no padding, CR EOL, # control quote, & 8-bit quote,
    // single-character checksum, and no repeat-count prefix.
    make_packet(
        sequence,
        b'S',
        &[
            tochar(80)?,
            tochar(5)?,
            tochar(0)?,
            0,
            tochar(13)?,
            CONTROL_QUOTE,
            EIGHT_BIT_QUOTE,
            b'1',
            b' ',
        ],
        false,
        true,
    )
}

fn ack_init_packet(sequence: u8) -> Result<Vec<u8>, String> {
    make_packet(
        sequence,
        b'Y',
        &[
            tochar(80)?,
            tochar(5)?,
            tochar(0)?,
            0,
            tochar(13)?,
            CONTROL_QUOTE,
            EIGHT_BIT_QUOTE,
            b'1',
            b' ',
        ],
        false,
        true,
    )
}

fn negotiated_eight_bit_quote(data: &[u8]) -> bool {
    matches!(data.get(6), Some(b'&' | b'Y'))
}

fn make_packet(
    sequence: u8,
    kind: u8,
    data: &[u8],
    encode: bool,
    use_eight_bit_quote: bool,
) -> Result<Vec<u8>, String> {
    let data = if encode {
        encode_data(data, use_eight_bit_quote)?
    } else {
        data.to_vec()
    };
    let length = 3_usize
        .checked_add(data.len())
        .ok_or_else(|| "Kermit 数据包长度溢出".to_string())?;
    let length_char = tochar(length)?;
    let mut body = Vec::with_capacity(length + 2);
    body.extend_from_slice(&[length_char, tochar(usize::from(sequence))?, kind]);
    body.extend_from_slice(&data);
    body.push(checksum(&body));
    let mut packet = Vec::with_capacity(body.len() + 2);
    packet.push(MARK);
    packet.extend_from_slice(&body);
    packet.push(EOL);
    Ok(packet)
}

fn encode_data(data: &[u8], use_eight_bit_quote: bool) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::with_capacity(data.len());
    for byte in data {
        let high = byte & 0x80 != 0;
        let low = byte & 0x7f;
        if high && !use_eight_bit_quote {
            return Err("Kermit 对端未协商 8 位数据转义，无法安全传输二进制文件".to_string());
        }
        if high {
            encoded.push(EIGHT_BIT_QUOTE);
        }
        if low < 0x20 || low == 0x7f {
            encoded.push(CONTROL_QUOTE);
            encoded.push(low ^ 0x40);
        } else if low == CONTROL_QUOTE || (use_eight_bit_quote && low == EIGHT_BIT_QUOTE) {
            encoded.push(CONTROL_QUOTE);
            encoded.push(low);
        } else {
            encoded.push(low);
        }
    }
    if encoded.len() + 3 > MAX_PACKET_VALUE {
        return Err("Kermit 数据包超过经典短包限制".to_string());
    }
    Ok(encoded)
}

fn decode_data(data: &[u8], use_eight_bit_quote: bool) -> Result<Vec<u8>, String> {
    let mut decoded = Vec::with_capacity(data.len());
    let mut index = 0;
    while index < data.len() {
        let mut high = false;
        if use_eight_bit_quote && data[index] == EIGHT_BIT_QUOTE {
            high = true;
            index += 1;
            if index == data.len() {
                return Err("Kermit 8 位转义序列不完整".to_string());
            }
        }
        let mut byte = data[index];
        index += 1;
        if byte == CONTROL_QUOTE {
            if index == data.len() {
                return Err("Kermit 控制转义序列不完整".to_string());
            }
            let quoted = data[index];
            // ctl() toggles bit 6 for control bytes. A quote prefix before a
            // printable quote character (##, #&, …) means the character
            // itself and must not be transformed.
            byte = if quoted >= 0x40 || quoted == 0x3f {
                quoted ^ 0x40
            } else {
                quoted
            };
            index += 1;
        }
        decoded.push(byte | if high { 0x80 } else { 0 });
    }
    Ok(decoded)
}

fn checksum(bytes: &[u8]) -> u8 {
    let sum = bytes
        .iter()
        .fold(0_u16, |sum, byte| sum.saturating_add(u16::from(*byte)));
    tochar_unchecked(usize::from((sum + ((sum & 0xc0) >> 6)) & 0x3f))
}

fn tochar(value: usize) -> Result<u8, String> {
    if value > MAX_PACKET_VALUE {
        return Err("Kermit 可打印字段超出范围".to_string());
    }
    Ok(tochar_unchecked(value))
}

fn tochar_unchecked(value: usize) -> u8 {
    (value as u8).saturating_add(0x20)
}

fn unchar(value: u8) -> Option<u8> {
    (0x20..=0x7e).contains(&value).then_some(value - 0x20)
}

fn next_sequence(sequence: u8) -> u8 {
    sequence.wrapping_add(1) & 0x3f
}

fn previous_sequence(sequence: u8) -> u8 {
    sequence.wrapping_add(0x3f) & 0x3f
}

async fn wait_for_packet<S>(
    stream: &mut S,
    timing: SerialTransferTiming,
    cancellation: &CancellationToken,
) -> Result<Packet, String>
where
    S: AsyncRead + Unpin,
{
    for _ in 0..MAX_RETRIES {
        match read_packet(stream, timing.control_timeout(), cancellation).await {
            Ok(Some(packet)) => return Ok(packet),
            Ok(None) => {}
            Err(error) if is_retryable_receive_error(&error) => {}
            Err(error) => return Err(error),
        }
    }
    Err("Kermit 等待发送端启动超时".to_string())
}

fn is_retryable_receive_error(error: &str) -> bool {
    error.contains("读取超时")
        || error.contains("结束符无效")
        || error.contains("长度无效")
        || error.contains("长度超出范围")
        || error.contains("校验失败")
        || error.contains("序号无效")
}

async fn cleanup_received_file(file: &mut Option<ReceiveFile>) {
    let Some(target) = file.take() else {
        return;
    };
    let path = target.path;
    drop(target.file);
    let _ = tokio::fs::remove_file(path).await;
}

async fn read_packet<S>(
    stream: &mut S,
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<Option<Packet>, String>
where
    S: AsyncRead + Unpin,
{
    loop {
        let Some(marker) = read_byte(stream, wait, cancellation).await? else {
            return Ok(None);
        };
        if marker == MARK {
            break;
        }
    }
    let Some(length_char) = read_byte(stream, wait, cancellation).await? else {
        return Ok(None);
    };
    let Some(length) = unchar(length_char).map(usize::from) else {
        return Err("Kermit 数据包长度无效".to_string());
    };
    if !(3..=MAX_PACKET_VALUE).contains(&length) {
        return Err("Kermit 数据包长度超出范围".to_string());
    }
    let mut body = vec![0_u8; length];
    for byte in &mut body {
        *byte = read_byte(stream, wait, cancellation)
            .await?
            .ok_or_else(|| "Kermit 数据包读取超时".to_string())?;
    }
    let terminator = read_byte(stream, wait, cancellation)
        .await?
        .ok_or_else(|| "Kermit 数据包结束符读取超时".to_string())?;
    if terminator != EOL && terminator != b'\n' {
        return Err("Kermit 数据包结束符无效".to_string());
    }
    let expected = checksum(&[&[length_char], &body[..length - 1]].concat());
    if expected != body[length - 1] {
        return Err("Kermit 数据包校验失败".to_string());
    }
    Ok(Some(Packet {
        sequence: unchar(body[0]).ok_or_else(|| "Kermit 序号无效".to_string())?,
        kind: body[1],
        data: body[2..length - 1].to_vec(),
    }))
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
    tokio::select! {
        _ = cancellation.cancelled() => Err("Kermit 传输已取消".to_string()),
        result = timeout(wait, stream.read(&mut byte)) => match result {
            Ok(Ok(0)) => Err("Kermit 串口流提前结束".to_string()),
            Ok(Ok(_)) => Ok(Some(byte[0])),
            Ok(Err(error)) => Err(format!("读取 Kermit 数据失败：{error}")),
            Err(_) => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::io::duplex;
    use tokio_util::sync::CancellationToken;

    use super::super::super::{SerialTransferDirection, SerialTransferMode, SerialTransferRequest};
    use super::super::progress::SerialTransferReporter;
    use super::super::timing::SerialTransferTiming;
    use super::{decode_data, encode_data, receive, send};

    #[test]
    fn quotes_control_and_eight_bit_bytes_reversibly() {
        let source = [0_u8, 1, b'#', b'&', b'A', 0x80, 0xff, 0x8d];
        let encoded = encode_data(&source, true).unwrap();
        assert_eq!(decode_data(&encoded, true).unwrap(), source);
    }

    #[tokio::test]
    async fn round_trip_works_without_a_physical_device() {
        let source = temporary_path("kermit-source");
        let target_directory = temporary_path("kermit-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let bytes = (0..513_u32)
            .map(|value| (value.wrapping_mul(29) & 0xff) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            let request = SerialTransferRequest {
                direction: SerialTransferDirection::Send,
                mode: SerialTransferMode::Kermit,
                local_path: sender_source.to_string_lossy().into_owned(),
                local_paths: Vec::new(),
                xmodem_preserve_padding: true,
            };
            let mut reporter = SerialTransferReporter::disabled(
                SerialTransferDirection::Send,
                SerialTransferMode::Kermit,
                &request.local_path,
            );
            send(
                &mut sender_stream,
                &request,
                timing,
                &mut reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            let mut reporter = SerialTransferReporter::disabled(
                SerialTransferDirection::Receive,
                SerialTransferMode::Kermit,
                receiver_directory.to_string_lossy().as_ref(),
            );
            receive(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(sender.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(receiver.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(
            tokio::fs::read(target_directory.join(source.file_name().unwrap()))
                .await
                .unwrap(),
            bytes
        );
        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_dir_all(target_directory).await;
    }

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fileterm-serial-{label}-{}", uuid::Uuid::new_v4()))
    }
}
