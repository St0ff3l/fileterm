//! Serial file-transfer protocols.
//!
//! The worker owns the port while a transfer is active. Keeping the protocol
//! state machine here makes its checksum/frame rules testable without a
//! physical adapter; the renderer serializes ordinary controls and quick sends
//! behind the active transfer so protocol bytes cannot be interleaved.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::{
    SerialTransferDirection, SerialTransferMode, SerialTransferRequest, SerialTransferResult,
};
use super::progress::SerialTransferReporter;
use super::timing::SerialTransferTiming;

const SOH: u8 = 0x01;
const STX: u8 = 0x02;
const EOT: u8 = 0x04;
const ACK: u8 = 0x06;
const NAK: u8 = 0x15;
const CAN: u8 = 0x18;
const CRC_REQUEST: u8 = b'C';
const PAD: u8 = 0x1a;
const MAX_RETRIES: usize = 10;

#[derive(Clone, Copy, Debug)]
struct BlockTransferOptions {
    block_size: usize,
    use_crc: bool,
    offset: u64,
}

#[derive(Clone, Copy, Debug)]
struct YmodemFileOptions {
    size: u64,
    use_crc: bool,
    offset: u64,
}

pub(super) async fn execute<S>(
    stream: &mut S,
    request: SerialTransferRequest,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: CancellationToken,
) -> Result<SerialTransferResult, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let path = request.local_path.clone();
    let paths = if request.local_paths.is_empty() {
        vec![path.clone()]
    } else {
        request.local_paths.clone()
    };
    let mode = request.mode;
    let xmodem_preserve_padding = request.xmodem_preserve_padding;
    let result: Result<u64, String> = match (request.direction, mode) {
        (SerialTransferDirection::Send, SerialTransferMode::Raw) => {
            send_raw(stream, Path::new(&path), timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Raw) => {
            receive_raw(stream, Path::new(&path), timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Xmodem) => {
            send_xmodem(
                stream,
                Path::new(&path),
                false,
                timing,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Xmodem) => {
            receive_xmodem(
                stream,
                Path::new(&path),
                timing,
                xmodem_preserve_padding,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Ymodem) => {
            send_ymodem(stream, &paths, timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Ymodem) => {
            receive_ymodem(stream, Path::new(&path), timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Zmodem) => {
            super::zmodem::send(stream, &request, timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Zmodem) => {
            super::zmodem::receive(stream, Path::new(&path), timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Kermit) => {
            super::kermit::send(stream, &request, timing, reporter, &cancellation).await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Kermit) => {
            super::kermit::receive(stream, Path::new(&path), timing, reporter, &cancellation).await
        }
    };
    match &result {
        Ok(bytes) => reporter.finish("completed", *bytes, None, None),
        Err(error) => reporter.finish(
            if cancellation.is_cancelled() {
                "canceled"
            } else {
                "failed"
            },
            0,
            None,
            Some(error.clone()),
        ),
    }
    if result.is_err() && mode != SerialTransferMode::Raw {
        cancel_protocol(stream).await;
    }
    let bytes_transferred = result?;
    Ok(SerialTransferResult {
        bytes_transferred,
        local_path: path,
    })
}

async fn send_raw<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncWrite + Unpin,
{
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    if let Ok(metadata) = file.metadata().await {
        reporter.set_total(Some(metadata.len()));
    }
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut buffer, cancellation).await?;
        if count == 0 {
            break;
        }
        write_all(stream, &buffer[..count], timing.write_timeout, cancellation).await?;
        total += count as u64;
        reporter.report(total, None);
    }
    flush(stream, timing.write_timeout, cancellation).await?;
    Ok(total)
}

async fn receive_raw<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + Unpin,
{
    let mut file = None;
    let mut created = false;
    let mut buffer = vec![0_u8; 32 * 1024];
    let result: Result<u64, String> = async {
        let mut total = 0_u64;
        loop {
            let count = tokio::select! {
                _ = cancellation.cancelled() => return Err("串口接收已取消".to_string()),
                result = timeout(timing.raw_idle_timeout, stream.read(&mut buffer)) => match result {
                    Ok(Ok(count)) => count,
                    Ok(Err(error)) => return Err(format!("串口接收失败：{error}")),
                    Err(_) => break,
                },
            };
            if count == 0 {
                break;
            }
            if file.is_none() {
                file = Some(create_target(path).await?);
                created = true;
            }
            let target = file.as_mut().expect("file was created above");
            write_file(target, &buffer[..count], cancellation).await?;
            total += count as u64;
            reporter.report(total, None);
        }
        if let Some(mut file) = file.take() {
            file.flush()
                .await
                .map_err(|error| format!("无法保存串口接收文件：{error}"))?;
        }
        Ok(total)
    }
    .await;
    if result.is_err() && created {
        drop(file.take());
        cleanup_failed_receive(path).await;
    }
    result
}

async fn send_xmodem<S>(
    stream: &mut S,
    path: &Path,
    use_crc: bool,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let negotiated_crc = wait_for_sender_start(stream, timing, cancellation).await?;
    let use_crc = use_crc || negotiated_crc;
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
    reporter.set_total(Some(metadata.len()));
    send_blocks(
        stream,
        path,
        BlockTransferOptions {
            block_size: 128,
            use_crc,
            offset: 0,
        },
        timing,
        reporter,
        cancellation,
    )
    .await
}

async fn send_ymodem<S>(
    stream: &mut S,
    paths: &[String],
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if paths.is_empty() {
        return Err("串口 YMODEM 至少需要一个发送文件".to_string());
    }
    let mut total_size = 0_u64;
    let mut seen_names = HashSet::new();
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let path = Path::new(path);
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|error| format!("无法读取串口发送文件信息：{error}"))?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "串口 YMODEM 文件名无效".to_string())?;
        if !is_safe_transfer_file_name(file_name) {
            return Err("串口 YMODEM 文件名无效".to_string());
        }
        let header_text = format!("{}\0{}\0", file_name, metadata.len());
        if header_text.len() > 128 {
            return Err("串口 YMODEM 文件头过长，请缩短文件名".to_string());
        }
        if !seen_names.insert(file_name.to_lowercase()) {
            return Err("串口 YMODEM 文件名重复，无法安全接收".to_string());
        }
        total_size = total_size.saturating_add(metadata.len());
        files.push((path.to_path_buf(), metadata.len(), file_name.to_string()));
    }
    reporter.set_total(Some(total_size));

    let mut total = 0_u64;
    for (path, size, file_name) in files {
        let header_crc = wait_for_sender_start(stream, timing, cancellation).await?;
        let mut header = vec![0_u8; 128];
        let header_text = format!("{}\0{}\0", file_name, size);
        let header_bytes = header_text.as_bytes();
        header[..header_bytes.len()].copy_from_slice(header_bytes);
        send_packet(stream, 0, &header, header_crc, timing, cancellation).await?;
        // The receiver sends another C after ACKing the block-0 metadata.
        let data_crc = wait_for_sender_start(stream, timing, cancellation).await?;
        total = total.saturating_add(
            send_blocks(
                stream,
                &path,
                BlockTransferOptions {
                    block_size: 1024,
                    use_crc: data_crc,
                    offset: total,
                },
                timing,
                reporter,
                cancellation,
            )
            .await?,
        );
    }
    // Standard YMODEM terminates with an empty block-0 after the receiver
    // acknowledges EOT. Older peers may stop after EOT, so the final header
    // is negotiated with the same bounded timeout used at startup.
    let final_crc = wait_for_sender_start(stream, timing, cancellation).await?;
    send_packet(stream, 0, &[0_u8; 128], final_crc, timing, cancellation).await?;
    Ok(total)
}

async fn send_blocks<S>(
    stream: &mut S,
    path: &Path,
    options: BlockTransferOptions,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = File::open(path)
        .await
        .map_err(|error| format!("无法读取串口发送文件：{error}"))?;
    let mut block = vec![PAD; options.block_size];
    let mut sequence = 1_u8;
    let mut total = 0_u64;
    loop {
        let count = read_file(&mut file, &mut block, cancellation).await?;
        if count == 0 {
            break;
        }
        block[count..].fill(PAD);
        send_packet(
            stream,
            sequence,
            &block,
            options.use_crc,
            timing,
            cancellation,
        )
        .await?;
        total += count as u64;
        reporter.report(
            options.offset.saturating_add(total),
            Some(u64::from(sequence)),
        );
        sequence = sequence.wrapping_add(1);
        block.fill(PAD);
    }
    send_eot(stream, timing, cancellation).await?;
    Ok(total)
}

async fn receive_xmodem<S>(
    stream: &mut S,
    path: &Path,
    timing: SerialTransferTiming,
    preserve_padding: bool,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut control, use_crc) = receive_protocol_start(stream, timing, cancellation).await?;
    let mut file = create_target(path).await?;
    let result: Result<u64, String> = async {
        let mut expected = 1_u8;
        let mut pending_last: Option<Vec<u8>> = None;
        let mut total = 0_u64;
        let mut eot_seen = false;
        loop {
            match control {
                EOT => {
                    if !eot_seen {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        eot_seen = true;
                        control = read_next_protocol_byte(stream, timing, cancellation).await?;
                        continue;
                    }
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    if let Some(mut last) = pending_last.take() {
                        last = finalize_xmodem_payload(last, preserve_padding);
                        write_file(&mut file, &last, cancellation).await?;
                        total += last.len() as u64;
                    }
                    break;
                }
                SOH | STX => {
                    let block_size = if control == SOH { 128 } else { 1024 };
                    let Some((sequence, payload)) =
                        read_packet_tail(stream, block_size, use_crc, timing, cancellation).await?
                    else {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        control = read_next_protocol_byte(stream, timing, cancellation).await?;
                        continue;
                    };
                    if sequence == expected {
                        if let Some(previous) = pending_last.replace(payload) {
                            write_file(&mut file, &previous, cancellation).await?;
                            total += previous.len() as u64;
                            reporter.report(total, Some(u64::from(sequence.wrapping_sub(1))));
                        }
                        expected = expected.wrapping_add(1);
                        write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    } else if sequence == expected.wrapping_sub(1) {
                        // The ACK may have been lost; acknowledge a duplicate
                        // without writing it twice.
                        write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    } else {
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("XMODEM 数据块序号不连续".to_string());
                    }
                }
                CAN => return Err("对端取消了 XMODEM 传输".to_string()),
                _ => {}
            }
            control = read_next_protocol_byte(stream, timing, cancellation).await?;
        }
        file.flush()
            .await
            .map_err(|error| format!("无法保存 XMODEM 文件：{error}"))?;
        Ok(total)
    }
    .await;
    if result.is_err() {
        drop(file);
        cleanup_failed_receive(path).await;
    }
    result
}

fn finalize_xmodem_payload(mut payload: Vec<u8>, preserve_padding: bool) -> Vec<u8> {
    if !preserve_padding {
        while payload.last() == Some(&PAD) {
            payload.pop();
        }
    }
    payload
}

async fn receive_ymodem<S>(
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
        return Err("YMODEM 接收目录不存在".to_string());
    }
    let (mut control, use_crc) = receive_protocol_start(stream, timing, cancellation).await?;
    if control != SOH {
        return Err("YMODEM 文件头格式无效".to_string());
    }

    let mut total = 0_u64;
    loop {
        let Some((sequence, header)) =
            read_packet_tail(stream, 128, use_crc, timing, cancellation).await?
        else {
            // The complete frame was consumed (or its partial tail was
            // drained), so NAK is safe and the sender can retransmit a fresh
            // block-0 without leaving bytes from the old frame in front of it.
            write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
            control = read_next_protocol_byte(stream, timing, cancellation).await?;
            if control == CAN {
                return Err("对端取消了 YMODEM 传输".to_string());
            }
            continue;
        };
        if sequence != 0 {
            return Err("YMODEM 文件头序号无效".to_string());
        }
        let Some((file_name, size)) = parse_ymodem_header(&header)? else {
            // An empty block-0 is the standard end-of-batch marker.
            write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
            break;
        };
        let target = ymodem_target(directory, &file_name)?;
        let file_result = receive_ymodem_file(
            stream,
            &target,
            YmodemFileOptions {
                size,
                use_crc,
                offset: total,
            },
            timing,
            reporter,
            cancellation,
        )
        .await;
        let received = match file_result {
            Ok(received) => received,
            Err(error) => return Err(error),
        };
        total = total.saturating_add(received);

        // The receiver announces that it is ready for the next block-0. A
        // legacy single-file sender may stop after the EOT ACK, so a timeout
        // here is treated as a successful end of the batch.
        write_all(
            stream,
            &[ACK, if use_crc { CRC_REQUEST } else { NAK }],
            timing.write_timeout,
            cancellation,
        )
        .await?;
        control = match read_byte(stream, timing.control_timeout(), cancellation).await? {
            Some(next) => next,
            None => break,
        };
        if control != SOH {
            return Err("YMODEM 下一个文件头格式无效".to_string());
        }
    }
    Ok(total)
}

async fn receive_ymodem_file<S>(
    stream: &mut S,
    path: &Path,
    options: YmodemFileOptions,
    timing: SerialTransferTiming,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut file = create_target(path).await?;
    let result: Result<u64, String> = async {
        write_all(
            stream,
            &[ACK, if options.use_crc { CRC_REQUEST } else { NAK }],
            timing.write_timeout,
            cancellation,
        )
        .await?;
        let mut expected = 1_u8;
        let mut remaining = options.size;
        let mut total = 0_u64;
        let mut eot_seen = false;
        loop {
            let Some(control) = read_byte(stream, timing.control_timeout(), cancellation).await?
            else {
                return Err("等待 YMODEM 数据超时".to_string());
            };
            match control {
                EOT => {
                    if !eot_seen {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        eot_seen = true;
                        continue;
                    }
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                    break;
                }
                SOH | STX => {
                    let block_size = if control == SOH { 128 } else { 1024 };
                    let Some((sequence, payload)) =
                        read_packet_tail(stream, block_size, options.use_crc, timing, cancellation)
                            .await?
                    else {
                        write_all(stream, &[NAK], timing.write_timeout, cancellation).await?;
                        continue;
                    };
                    if sequence != expected {
                        if sequence == expected.wrapping_sub(1) {
                            write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                            continue;
                        }
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("YMODEM 数据块序号不连续".to_string());
                    }
                    if remaining == 0 {
                        write_all(stream, &[CAN, CAN], timing.write_timeout, cancellation).await?;
                        return Err("YMODEM 接收数据超过文件头声明的大小".to_string());
                    }
                    let count = remaining.min(payload.len() as u64) as usize;
                    write_file(&mut file, &payload[..count], cancellation).await?;
                    remaining -= count as u64;
                    total += count as u64;
                    reporter.report(
                        options.offset.saturating_add(total),
                        Some(u64::from(sequence)),
                    );
                    expected = expected.wrapping_add(1);
                    write_all(stream, &[ACK], timing.write_timeout, cancellation).await?;
                }
                CAN => return Err("对端取消了 YMODEM 传输".to_string()),
                _ => {}
            }
        }
        if remaining != 0 {
            return Err("YMODEM 文件大小与接收数据不一致".to_string());
        }
        file.flush()
            .await
            .map_err(|error| format!("无法保存 YMODEM 文件：{error}"))?;
        Ok(total)
    }
    .await;
    if result.is_err() {
        drop(file);
        cleanup_failed_receive(path).await;
    }
    result
}

async fn receive_protocol_start<S>(
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

async fn read_next_protocol_byte<S>(
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

async fn wait_for_sender_start<S>(
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

async fn send_packet<S>(
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

async fn send_eot<S>(
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

async fn cancel_protocol<S>(stream: &mut S)
where
    S: AsyncWrite + Unpin,
{
    let _ = timeout(Duration::from_millis(100), async {
        let _ = stream.write_all(&[CAN, CAN]).await;
        let _ = stream.flush().await;
    })
    .await;
}

async fn read_packet_tail<S>(
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
async fn read_packet_tail_with_timeout<S>(
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
                // The first timeout may have consumed a prefix. Continue draining
                // the *same* expected frame, but never let a broken sender block a
                // retry indefinitely. All bytes consumed by this second attempt
                // are discarded if the frame still cannot be completed.
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

fn parse_ymodem_header(header: &[u8]) -> Result<Option<(String, u64)>, String> {
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

fn ymodem_target(directory: &Path, file_name: &str) -> Result<std::path::PathBuf, String> {
    if !is_safe_transfer_file_name(file_name) {
        return Err("YMODEM 文件名无效，不允许写出接收目录".to_string());
    }
    Ok(directory.join(file_name))
}

pub(super) fn is_safe_transfer_file_name(file_name: &str) -> bool {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        || file_name.ends_with('.')
        || file_name.ends_with(' ')
    {
        return false;
    }

    let stem = file_name
        .trim_end_matches(['.', ' '])
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_windows_numbered_device_name(&stem, "COM")
        && !is_windows_numbered_device_name(&stem, "LPT")
}

fn is_windows_numbered_device_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
    })
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

pub(super) async fn write_all<S>(
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

pub(super) async fn flush<S>(
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

pub(super) async fn create_target(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "串口接收目标文件已存在，请更换文件名".to_string()
            } else {
                format!("无法创建串口接收文件：{error}")
            }
        })
}

async fn cleanup_failed_receive(path: &Path) {
    let _ = tokio::fs::remove_file(path).await;
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

    use tokio::io::{duplex, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    use super::super::super::{SerialTransferDirection, SerialTransferMode};
    use super::super::progress::SerialTransferReporter;
    use super::super::timing::SerialTransferTiming;
    use super::{
        checksum, crc16, create_target, finalize_xmodem_payload, is_safe_transfer_file_name,
        parse_ymodem_header, parse_ymodem_size, read_byte, read_packet_tail,
        read_packet_tail_with_timeout, receive_raw, receive_xmodem, receive_ymodem,
        receive_ymodem_file, send_xmodem, send_ymodem, YmodemFileOptions, SOH,
    };

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
        assert_eq!(
            parse_ymodem_header(&header).unwrap(),
            Some(("file.bin".to_string(), 123))
        );
        assert_eq!(parse_ymodem_header(&[0_u8; 128]).unwrap(), None);
        assert!(parse_ymodem_size(b"file.bin").is_err());
    }

    #[test]
    fn rejects_cross_platform_unsafe_transfer_names() {
        for name in [
            "../escape",
            "folder\\escape",
            "CON",
            "COM1",
            "report:",
            "trailing.",
        ] {
            assert!(
                !is_safe_transfer_file_name(name),
                "name should be rejected: {name}"
            );
        }
        assert!(is_safe_transfer_file_name("report.bin"));
    }

    #[test]
    fn xmodem_padding_is_only_trimmed_when_explicitly_requested() {
        let payload = vec![0x41, 0x1a, 0x1a];
        assert_eq!(finalize_xmodem_payload(payload.clone(), true), payload);
        assert_eq!(finalize_xmodem_payload(payload, false), vec![0x41]);
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
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Xmodem,
            source.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Xmodem,
            target.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let receiver_target = target.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            send_xmodem(
                &mut sender_stream,
                &sender_source,
                false,
                timing,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            receive_xmodem(
                &mut receiver_stream,
                &receiver_target,
                timing,
                false,
                &mut receiver_reporter,
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

    #[tokio::test]
    async fn ymodem_round_trip_works_without_a_physical_device() {
        let source = temporary_path("ymodem-source");
        let target_directory = temporary_path("ymodem-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let target = target_directory.join(source.file_name().unwrap());
        let bytes = (0..1537_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let cancellation = CancellationToken::new();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Ymodem,
            source.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target_directory.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let sender_paths = vec![sender_source.to_string_lossy().into_owned()];
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            send_ymodem(
                &mut sender_stream,
                &sender_paths,
                timing,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            receive_ymodem(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(sender.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(receiver.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(tokio::fs::read(&target).await.unwrap(), bytes);
        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_dir_all(target_directory).await;
    }

    #[tokio::test]
    async fn ymodem_round_trip_supports_a_batch_without_a_physical_device() {
        let source_a = temporary_path("ymodem-batch-a");
        let source_b = temporary_path("ymodem-batch-b");
        let target_directory = temporary_path("ymodem-batch-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let bytes_a = (0..257_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let bytes_b = (0..2049_u16)
            .map(|value| (value % 239) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source_a, &bytes_a).await.unwrap();
        tokio::fs::write(&source_b, &bytes_b).await.unwrap();
        let cancellation = CancellationToken::new();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Ymodem,
            source_a.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target_directory.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_paths = vec![
            source_a.to_string_lossy().into_owned(),
            source_b.to_string_lossy().into_owned(),
        ];
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            send_ymodem(
                &mut sender_stream,
                &sender_paths,
                timing,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            receive_ymodem(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(
            sender.await.unwrap().unwrap(),
            (bytes_a.len() + bytes_b.len()) as u64
        );
        assert_eq!(
            receiver.await.unwrap().unwrap(),
            (bytes_a.len() + bytes_b.len()) as u64
        );
        assert_eq!(
            tokio::fs::read(target_directory.join(source_a.file_name().unwrap()))
                .await
                .unwrap(),
            bytes_a
        );
        assert_eq!(
            tokio::fs::read(target_directory.join(source_b.file_name().unwrap()))
                .await
                .unwrap(),
            bytes_b
        );
        let _ = tokio::fs::remove_file(source_a).await;
        let _ = tokio::fs::remove_file(source_b).await;
        let _ = tokio::fs::remove_dir_all(target_directory).await;
    }

    #[tokio::test]
    async fn receive_target_never_overwrites_existing_file() {
        let target = temporary_path("existing-target");
        let original = b"keep this file";
        tokio::fs::write(&target, original).await.unwrap();
        let error = create_target(&target).await.unwrap_err();
        assert!(error.contains("已存在"));
        assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn ymodem_existing_target_is_not_removed_after_create_failure() {
        let target = temporary_path("existing-ymodem-target");
        let original = b"keep this YMODEM file";
        tokio::fs::write(&target, original).await.unwrap();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let mut reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target.to_str().unwrap(),
        );
        let (_writer, mut reader) = duplex(64);
        let error = receive_ymodem_file(
            &mut reader,
            &target,
            YmodemFileOptions {
                size: original.len() as u64,
                use_crc: true,
                offset: 0,
            },
            timing,
            &mut reporter,
            &cancellation,
        )
        .await
        .unwrap_err();
        assert!(error.contains("已存在"));
        assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn canceled_raw_receive_removes_partial_target() {
        let target = temporary_path("canceled-raw-target");
        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let receiver_target = target.clone();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Raw,
            target.to_str().unwrap(),
        );
        let (mut sender_stream, mut receiver_stream) = duplex(64);
        let receiver = tokio::spawn(async move {
            receive_raw(
                &mut receiver_stream,
                &receiver_target,
                timing,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        sender_stream.write_all(b"partial").await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !target.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        cancellation.cancel();
        assert!(receiver.await.unwrap().is_err());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn incomplete_packet_is_not_retried_with_leftover_bytes() {
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let (mut writer, mut reader) = duplex(256);
        writer.write_all(&[1, 254, 0x41]).await.unwrap();
        writer.shutdown().await.unwrap();
        let error = read_packet_tail(&mut reader, 128, true, timing, &cancellation)
            .await
            .unwrap_err();
        assert!(
            error.contains("数据帧不完整") || error.contains("读取串口文件传输数据失败"),
            "unexpected incomplete-frame error: {error}"
        );
    }

    #[tokio::test]
    async fn partial_packet_is_drained_before_a_fresh_retry_marker() {
        let cancellation = CancellationToken::new();
        let (mut writer, mut reader) = duplex(512);
        let payload = vec![0x42_u8; 128];
        let mut packet = vec![1_u8, 254_u8];
        packet.extend_from_slice(&payload);
        packet.extend_from_slice(&crc16(&payload).to_be_bytes());
        let retry_packet = packet.clone();
        let writer_task = tokio::spawn(async move {
            // Only a prefix of the first frame reaches the receiver. The
            // sender later retransmits a complete frame after the receiver's
            // bounded drain window has elapsed.
            writer.write_all(&packet[..9]).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            writer.write_all(&[SOH]).await.unwrap();
            writer.write_all(&retry_packet).await.unwrap();
        });

        let first = read_packet_tail_with_timeout(
            &mut reader,
            128,
            true,
            std::time::Duration::from_millis(5),
            &cancellation,
        )
        .await
        .unwrap();
        assert!(first.is_none());

        let marker = read_byte(
            &mut reader,
            std::time::Duration::from_millis(100),
            &cancellation,
        )
        .await
        .unwrap();
        assert_eq!(marker, Some(SOH));
        let retry = read_packet_tail_with_timeout(
            &mut reader,
            128,
            true,
            std::time::Duration::from_millis(100),
            &cancellation,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(retry, (1, payload));
        writer_task.await.unwrap();
    }

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fileterm-serial-{label}-{}", uuid::Uuid::new_v4()))
    }
}
