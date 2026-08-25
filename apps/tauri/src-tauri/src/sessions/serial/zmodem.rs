//! ZMODEM transport adapter.
//!
//! `zmodem2` owns the wire state machine; this module only adapts its
//! poll/submit API to Tokio serial I/O and FileTerm's progress/cancellation
//! model. Keeping the adapter separate prevents the legacy X/YMODEM framing
//! code from becoming a protocol switchboard.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zmodem2::{Action, Event, FileInfo, Position, Receiver, Sender};

use super::super::SerialTransferRequest;
use super::file_safety::StagedReceiveFile;
use super::limits::TransferBudget;
use super::progress::SerialTransferReporter;
use super::timing::SerialTransferTiming;
use super::transfer::{create_target, flush, is_safe_transfer_file_name, write_all};

const MAX_WIRE_READ: usize = 16 * 1024;

struct SendFile {
    path: PathBuf,
    name: Vec<u8>,
    size: u64,
}

struct ReceiveFile {
    file: StagedReceiveFile,
    declared_size: Option<u64>,
    bytes_written: u64,
}

pub(super) async fn send<S>(
    stream: &mut S,
    request: &SerialTransferRequest,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
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
            .map_err(|error| format!("无法读取 ZMODEM 发送文件：{error}"))?;
        if !metadata.is_file() {
            return Err("ZMODEM 发送路径不是文件".to_string());
        }
        if metadata.len() > u64::from(u32::MAX) {
            return Err("ZMODEM 单个文件不能超过 4 GiB".to_string());
        }
        budget.begin_file(Some(metadata.len()))?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| is_safe_transfer_file_name(value))
            .ok_or_else(|| "ZMODEM 发送文件名无效".to_string())?
            .as_bytes()
            .to_vec();
        total_size = total_size
            .checked_add(metadata.len())
            .ok_or_else(|| "ZMODEM 文件总大小超出支持范围".to_string())?;
        files.push(SendFile {
            path,
            name,
            size: metadata.len(),
        });
    }
    if files.is_empty() {
        return Err("ZMODEM 至少需要一个发送文件".to_string());
    }
    reporter.set_total(Some(total_size));

    let mut protocol = Sender::new().map_err(|error| format!("初始化 ZMODEM 发送失败：{error}"))?;
    let mut file_index = 0_usize;
    protocol
        .start_file(file_info(&files[file_index]))
        .map_err(|error| format!("启动 ZMODEM 文件失败：{error}"))?;
    let mut pending_wire = Vec::new();
    let mut bytes_transferred = 0_u64;
    let mut current_file_offset = 0_u64;

    loop {
        match protocol.poll() {
            Action::WriteWire(wire) => {
                let wire = wire.to_vec();
                write_all(stream, &wire, timing.write_timeout, cancellation).await?;
                flush(stream, timing.write_timeout, cancellation).await?;
                protocol.wire_written(wire.len());
            }
            Action::ReadFile { offset, max_len } => {
                let file = &files[file_index];
                let mut source = File::open(&file.path)
                    .await
                    .map_err(|error| format!("无法读取 ZMODEM 发送文件：{error}"))?;
                source
                    .seek(std::io::SeekFrom::Start(u64::from(offset.get())))
                    .await
                    .map_err(|error| format!("定位 ZMODEM 发送文件失败：{error}"))?;
                let mut buffer = vec![0_u8; max_len.max(1)];
                let count = source
                    .read(&mut buffer)
                    .await
                    .map_err(|error| format!("读取 ZMODEM 发送文件失败：{error}"))?;
                if count == 0 {
                    return Err("ZMODEM 文件在传输过程中提前结束".to_string());
                }
                let count = count.min(max_len);
                let next_offset = u64::from(offset.get()).saturating_add(count as u64);
                if next_offset > current_file_offset {
                    bytes_transferred = bytes_transferred
                        .saturating_add(next_offset.saturating_sub(current_file_offset));
                    current_file_offset = next_offset;
                }
                reporter.report(bytes_transferred, Some(u64::from(offset.get()) / 1024));
                protocol
                    .submit_file(&buffer[..count])
                    .map_err(|error| format!("提交 ZMODEM 文件数据失败：{error}"))?;
            }
            Action::Event(Event::FileCompleted) => {
                file_index += 1;
                current_file_offset = 0;
                if let Some(file) = files.get(file_index) {
                    protocol
                        .start_file(file_info(file))
                        .map_err(|error| format!("启动下一个 ZMODEM 文件失败：{error}"))?;
                } else {
                    protocol
                        .finish()
                        .map_err(|error| format!("结束 ZMODEM 会话失败：{error}"))?;
                }
            }
            Action::Event(Event::SessionCompleted) => return Ok(bytes_transferred),
            Action::Event(Event::Aborted) => return Err("对端取消了 ZMODEM 传输".to_string()),
            Action::Event(Event::FileStarted(_)) | Action::WriteFile(_) => {
                return Err("ZMODEM 发送端收到无效协议状态".to_string())
            }
            Action::Idle if !pending_wire.is_empty() => {
                let consumed = match protocol.submit_wire(&pending_wire) {
                    Ok(consumed) => consumed,
                    Err(error) if is_retryable_wire_error(&error) => {
                        pending_wire.clear();
                        continue;
                    }
                    Err(error) => return Err(format!("ZMODEM 读取对端响应失败：{error}")),
                };
                if consumed == 0 {
                    return Err("ZMODEM 协议没有消费输入数据".to_string());
                }
                pending_wire.drain(..consumed);
            }
            Action::Idle => {
                protocol
                    .timeout()
                    .map_err(|error| format!("ZMODEM 等待对端响应失败：{error}"))?;
                if let Some(bytes) =
                    read_wire(stream, timing.control_timeout(), cancellation).await?
                {
                    pending_wire.extend_from_slice(&bytes);
                }
            }
            _ => return Err("ZMODEM 收到未知协议动作".to_string()),
        }
    }
}

pub(super) async fn receive<S>(
    stream: &mut S,
    directory: &Path,
    timing: SerialTransferTiming,
    budget: &mut TransferBudget,
    reporter: &mut SerialTransferReporter,
    cancellation: &CancellationToken,
) -> Result<u64, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    if !directory.is_dir() {
        return Err("ZMODEM 接收目录不存在".to_string());
    }
    let mut protocol =
        Receiver::new().map_err(|error| format!("初始化 ZMODEM 接收失败：{error}"))?;
    protocol.set_manual_file_accept(true);
    let mut pending_wire = Vec::new();
    let mut current_file: Option<ReceiveFile> = None;
    let mut bytes_transferred = 0_u64;

    loop {
        match protocol.poll() {
            Action::WriteWire(wire) => {
                let wire = wire.to_vec();
                if let Err(error) =
                    write_all(stream, &wire, timing.write_timeout, cancellation).await
                {
                    cleanup_receive_file(&mut current_file).await;
                    return Err(error);
                }
                if let Err(error) = flush(stream, timing.write_timeout, cancellation).await {
                    cleanup_receive_file(&mut current_file).await;
                    return Err(error);
                }
                protocol.wire_written(wire.len());
            }
            Action::WriteFile(bytes) => {
                let bytes = bytes.to_vec();
                let count = bytes.len() as u64;
                let write_result = {
                    let file = current_file
                        .as_mut()
                        .ok_or_else(|| "ZMODEM 收到文件数据前没有文件头".to_string())?;
                    let next_size = file
                        .bytes_written
                        .checked_add(count)
                        .ok_or_else(|| "ZMODEM 接收文件大小超出支持范围".to_string())?;
                    if file
                        .declared_size
                        .is_some_and(|declared_size| next_size > declared_size)
                    {
                        Err("ZMODEM 接收数据超过文件头声明的大小".to_string())
                    } else {
                        let result = file
                            .file
                            .write_all(
                                &bytes,
                                cancellation,
                                file.declared_size.is_none().then_some(budget),
                            )
                            .await;
                        if result.is_ok() {
                            file.bytes_written = next_size;
                        }
                        result
                    }
                };
                if let Err(error) = write_result {
                    cleanup_receive_file(&mut current_file).await;
                    return Err(format!("保存 ZMODEM 接收文件失败：{error}"));
                }
                if let Err(error) = protocol.file_written(bytes.len()) {
                    cleanup_receive_file(&mut current_file).await;
                    return Err(format!("确认 ZMODEM 接收数据失败：{error}"));
                }
                bytes_transferred = bytes_transferred.saturating_add(bytes.len() as u64);
                reporter.report(bytes_transferred, None);
            }
            Action::Event(Event::FileStarted(info)) => {
                if current_file.is_some() {
                    cleanup_receive_file(&mut current_file).await;
                    return Err("ZMODEM 在上一个文件结束前又发送了文件头".to_string());
                }
                let name = std::str::from_utf8(info.name)
                    .map_err(|_| "ZMODEM 文件名不是有效文本".to_string())?;
                if !is_safe_transfer_file_name(name) {
                    return Err("ZMODEM 文件名无效，不允许写出接收目录".to_string());
                }
                let path = directory.join(name);
                let declared_size = info.size.map(|size| u64::from(size.get()));
                budget.begin_file(declared_size)?;
                let file = create_target(&path, budget.max_file_bytes()).await?;
                reporter.set_total(Some(
                    bytes_transferred.saturating_add(declared_size.unwrap_or(0)),
                ));
                if let Err(error) = protocol.accept_file_at(0) {
                    file.cleanup().await;
                    return Err(format!("接受 ZMODEM 文件失败：{error}"));
                }
                current_file = Some(ReceiveFile {
                    file,
                    declared_size,
                    bytes_written: 0,
                });
            }
            Action::Event(Event::FileCompleted) => {
                let Some(receive_file) = current_file.as_ref() else {
                    return Err("ZMODEM 文件结束时没有打开的接收文件".to_string());
                };
                if receive_file
                    .declared_size
                    .is_some_and(|declared_size| receive_file.bytes_written != declared_size)
                {
                    cleanup_receive_file(&mut current_file).await;
                    return Err("ZMODEM 文件大小与接收数据不一致".to_string());
                }
                let file = current_file
                    .take()
                    .expect("ZMODEM receive file exists after completion check");
                if let Err(error) = file.file.commit().await {
                    return Err(format!("刷新 ZMODEM 接收文件失败：{error}"));
                }
            }
            Action::Event(Event::SessionCompleted) => {
                if current_file.is_some() {
                    cleanup_receive_file(&mut current_file).await;
                    return Err("ZMODEM 会话结束时接收文件尚未完成".to_string());
                }
                return Ok(bytes_transferred);
            }
            Action::Event(Event::Aborted) => {
                cleanup_receive_file(&mut current_file).await;
                return Err("对端取消了 ZMODEM 传输".to_string());
            }
            Action::ReadFile { .. } => return Err("ZMODEM 接收端收到无效协议状态".to_string()),
            Action::Idle if !pending_wire.is_empty() => {
                let consumed = match protocol.submit_wire(&pending_wire) {
                    Ok(consumed) => consumed,
                    Err(error) if is_retryable_wire_error(&error) => {
                        pending_wire.clear();
                        continue;
                    }
                    Err(error) => {
                        cleanup_receive_file(&mut current_file).await;
                        return Err(format!("ZMODEM 接收数据失败：{error}"));
                    }
                };
                if consumed == 0 {
                    cleanup_receive_file(&mut current_file).await;
                    return Err("ZMODEM 协议没有消费输入数据".to_string());
                }
                pending_wire.drain(..consumed);
            }
            Action::Idle => {
                if let Err(error) = protocol.timeout() {
                    cleanup_receive_file(&mut current_file).await;
                    return Err(format!("ZMODEM 接收等待超时失败：{error}"));
                }
                let bytes = match read_wire(stream, timing.control_timeout(), cancellation).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        cleanup_receive_file(&mut current_file).await;
                        return Err(error);
                    }
                };
                if let Some(bytes) = bytes {
                    pending_wire.extend_from_slice(&bytes);
                }
            }
            _ => {
                cleanup_receive_file(&mut current_file).await;
                return Err("ZMODEM 收到未知协议动作".to_string());
            }
        }
    }
}

async fn cleanup_receive_file(file: &mut Option<ReceiveFile>) {
    let Some(file) = file.take() else {
        return;
    };
    file.file.cleanup().await;
}

fn file_info(file: &SendFile) -> FileInfo<'_> {
    FileInfo::new(
        &file.name,
        Some(Position::new(file.size.try_into().unwrap_or(u32::MAX))),
    )
}

fn is_retryable_wire_error(error: &zmodem2::Error) -> bool {
    matches!(
        error,
        zmodem2::Error::UnexpectedCrc16 | zmodem2::Error::UnexpectedCrc32
    )
}

async fn read_wire<S>(
    stream: &mut S,
    wait: Duration,
    cancellation: &CancellationToken,
) -> Result<Option<Vec<u8>>, String>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = vec![0_u8; MAX_WIRE_READ];
    tokio::select! {
        _ = cancellation.cancelled() => Err("ZMODEM 传输已取消".to_string()),
        result = timeout(wait, stream.read(&mut buffer)) => match result {
            Ok(Ok(0)) => Err("ZMODEM 串口流提前结束".to_string()),
            Ok(Ok(count)) => {
                buffer.truncate(count);
                Ok(Some(buffer))
            }
            Ok(Err(error)) => Err(format!("读取 ZMODEM 数据失败：{error}")),
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
    use super::super::limits::{SerialTransferLimits, TransferBudget};
    use super::super::progress::SerialTransferReporter;
    use super::super::timing::SerialTransferTiming;
    use super::{receive, send};

    #[tokio::test]
    async fn round_trip_works_without_a_physical_device() {
        let source = temporary_path("zmodem-source");
        let target_directory = temporary_path("zmodem-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let bytes = (0..4097_u32)
            .map(|value| (value.wrapping_mul(17) & 0xff) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(64 * 1024);
        let sender = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            let request = SerialTransferRequest {
                direction: SerialTransferDirection::Send,
                mode: SerialTransferMode::Zmodem,
                local_path: sender_source.to_string_lossy().into_owned(),
                local_paths: Vec::new(),
                xmodem_preserve_padding: true,
            };
            let mut reporter = SerialTransferReporter::disabled(
                SerialTransferDirection::Send,
                SerialTransferMode::Zmodem,
                &request.local_path,
            );
            send(
                &mut sender_stream,
                &request,
                timing,
                &mut budget,
                &mut reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            let mut reporter = SerialTransferReporter::disabled(
                SerialTransferDirection::Receive,
                SerialTransferMode::Zmodem,
                receiver_directory.to_string_lossy().as_ref(),
            );
            receive(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut budget,
                &mut reporter,
                &receiver_cancellation,
            )
            .await
        });
        let (sender_result, receiver_result) = tokio::join!(sender, receiver);
        let sender_result = sender_result.unwrap();
        let receiver_result = receiver_result.unwrap();
        assert_eq!(sender_result, Ok(bytes.len() as u64));
        assert_eq!(receiver_result, Ok(bytes.len() as u64));
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
