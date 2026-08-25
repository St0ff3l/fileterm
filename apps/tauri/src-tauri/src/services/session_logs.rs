//! Per-session terminal transcript logging.
//!
//! Session logs are deliberately separate from the diagnostic logger. They
//! contain decoded terminal output by default. Serial logs can explicitly opt
//! into TX bytes and raw wire records; passwords and other renderer-side
//! secrets are never added by this service. Automatic logging uses a small
//! async writer queue so a slow disk cannot block an SSH/serial worker.

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{Local, SecondsFormat};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};

use crate::services::workspace::WorkspaceState;
use crate::sessions::terminal::decode_terminal;
use crate::AppError;

const DEFAULT_DIRECTORY_NAME: &str = "session-logs";
const FLUSH_TIMEOUT: Duration = Duration::from_secs(2);
const LOG_QUEUE_CAPACITY: usize = 256;
const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

enum SessionLogMessage {
    Chunk(String),
    Sync(oneshot::Sender<()>),
    Flush(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct SessionLogHandle {
    sender: mpsc::Sender<SessionLogMessage>,
    options: SessionLogOptions,
    path: PathBuf,
    dropped_chunks: Arc<AtomicU64>,
}

/// A non-blocking sink used by serial transport adapters. It lets protocol
/// traffic be recorded from `poll_read`/`poll_write` without awaiting disk IO
/// on the serial worker.
#[derive(Clone)]
pub struct SerialLogSink {
    handle: SessionLogHandle,
}

#[derive(Clone, Debug)]
struct SessionLogOptions {
    serial: bool,
    include_input: bool,
    timestamps: bool,
    raw: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialLogDirection {
    Rx,
    Tx,
}

impl SerialLogSink {
    pub fn append(
        &self,
        direction: SerialLogDirection,
        bytes: &[u8],
        decoded: Option<&str>,
        encoding: &str,
    ) {
        if bytes.is_empty()
            || (direction == SerialLogDirection::Tx && !self.handle.options.include_input)
        {
            return;
        }
        let payload = if self.handle.options.raw {
            format_hex(bytes)
        } else {
            decoded
                .map(str::to_string)
                .unwrap_or_else(|| decode_terminal(bytes, encoding))
        };
        let timestamp = if self.handle.options.timestamps {
            format!("[{}] ", timestamp_rfc3339())
        } else {
            String::new()
        };
        let direction = match direction {
            SerialLogDirection::Rx => "RX",
            SerialLogDirection::Tx => "TX",
        };
        enqueue_chunk(
            &self.handle,
            format!("{timestamp}{direction}: {payload}\r\n"),
        );
    }

    /// Append bytes as they appeared on the configured serial wire. This is
    /// intentionally emitted only for raw logs so parity bits and protocol
    /// control bytes are never mistaken for decoded terminal text.
    pub fn append_wire(&self, direction: SerialLogDirection, bytes: &[u8]) {
        if bytes.is_empty()
            || !self.handle.options.raw
            || (direction == SerialLogDirection::Tx && !self.handle.options.include_input)
        {
            return;
        }
        let timestamp = if self.handle.options.timestamps {
            format!("[{}] ", timestamp_rfc3339())
        } else {
            String::new()
        };
        let direction = match direction {
            SerialLogDirection::Rx => "WIRE RX",
            SerialLogDirection::Tx => "WIRE TX",
        };
        enqueue_chunk(
            &self.handle,
            format!("{timestamp}{direction}: {}\r\n", format_hex(bytes)),
        );
    }
}

pub async fn serial_log_sink(app: &AppHandle, tab_id: &str) -> Option<SerialLogSink> {
    let state = app.state::<WorkspaceState>();
    let sink = state
        .session_log_writers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .filter(|handle| handle.options.serial)
        .map(|handle| SerialLogSink { handle });
    sink
}

/// Start or stop the automatic writer for one profile-backed terminal tab.
pub async fn start_for_tab(
    app: &AppHandle,
    state: &WorkspaceState,
    tab_id: &str,
    profile: &Value,
) -> Result<(), AppError> {
    stop_for_tab(state, tab_id).await;

    let enabled = profile
        .get("sessionLogEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let session_type = profile.get("type").and_then(Value::as_str).unwrap_or("");
    if !enabled || session_type == "ftp" {
        return Ok(());
    }

    let directory = configured_directory(app, profile)?;
    fs::create_dir_all(&directory)
        .await
        .map_err(|error| AppError::Storage(format!("无法创建会话日志目录: {error}")))?;

    let name = profile
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("session");
    let options = SessionLogOptions {
        serial: session_type == "serial",
        include_input: profile
            .get("sessionLogIncludeInput")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        timestamps: profile
            .get("sessionLogTimestamps")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        raw: profile
            .get("sessionLogRaw")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    let timestamp = local_filename_timestamp();
    let path = directory.join(format!(
        "{}-{}-{}.log",
        sanitize_filename(name),
        timestamp,
        sanitize_filename(&short_tab_id(tab_id))
    ));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
        .map_err(|error| AppError::Storage(format!("无法打开会话日志文件: {error}")))?;
    let header = if options.serial {
        format!(
            "# FileTerm 会话日志\r\n# 连接: {name}\r\n# 串口日志包含 RX{}，原始字节: {}，时间戳: {}（本机时区）\r\n\r\n",
            if options.include_input { " / TX" } else { "" },
            if options.raw { "Hex" } else { "否" },
            if options.timestamps { "是" } else { "否" },
        )
    } else {
        "# FileTerm 会话日志\r\n# 连接: ".to_string()
            + name
            + "\r\n# 只记录终端输出，不记录键盘输入\r\n\r\n"
    };
    file.write_all(header.as_bytes())
        .await
        .map_err(|error| AppError::Storage(format!("无法写入会话日志文件: {error}")))?;
    file.flush()
        .await
        .map_err(|error| AppError::Storage(format!("无法刷新会话日志文件: {error}")))?;

    let (sender, receiver) = mpsc::channel(LOG_QUEUE_CAPACITY);
    let dropped_chunks = Arc::new(AtomicU64::new(0));
    let handle = SessionLogHandle {
        sender: sender.clone(),
        options,
        path: path.clone(),
        dropped_chunks: dropped_chunks.clone(),
    };
    state
        .session_log_writers
        .write()
        .await
        .insert(tab_id.to_string(), handle);

    let app = app.clone();
    let tab_id = tab_id.to_string();
    tauri::async_runtime::spawn(async move {
        run_writer(file, receiver, dropped_chunks, &app, &tab_id, &path).await;
    });

    Ok(())
}

/// Append decoded terminal output without waiting for file IO.
pub async fn append_chunk(app: &AppHandle, tab_id: &str, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    let state = app.state::<WorkspaceState>();
    let handle = state.session_log_writers.read().await.get(tab_id).cloned();
    if let Some(handle) = handle.filter(|handle| !handle.options.serial) {
        enqueue_chunk(&handle, chunk.to_string());
    }
}

/// Append a serial RX/TX record. The byte-level path is intentionally
/// separate from `append_chunk`: it can preserve raw bytes and direction
/// without making SSH/Telnet logs collect keyboard input.
pub async fn append_serial_bytes(
    app: &AppHandle,
    tab_id: &str,
    direction: SerialLogDirection,
    bytes: &[u8],
    decoded: Option<&str>,
    encoding: &str,
) {
    if let Some(sink) = serial_log_sink(app, tab_id).await {
        sink.append(direction, bytes, decoded, encoding);
    }
}

fn enqueue_chunk(handle: &SessionLogHandle, chunk: String) {
    if handle
        .sender
        .try_send(SessionLogMessage::Chunk(chunk))
        .is_err()
    {
        handle.dropped_chunks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Flush and remove the writer for a tab. It is safe to call this for tabs
/// which never had automatic logging enabled.
pub async fn stop_for_tab(state: &WorkspaceState, tab_id: &str) {
    let handle = state.session_log_writers.write().await.remove(tab_id);
    if let Some(handle) = handle {
        flush_handle(handle).await;
    }
}

/// Flush every automatic writer during application shutdown.
pub async fn shutdown(state: &WorkspaceState) {
    let handles = state
        .session_log_writers
        .write()
        .await
        .drain()
        .map(|(_, handle)| handle)
        .collect::<Vec<_>>();
    for handle in handles {
        flush_handle(handle).await;
    }
}

/// Open a native save dialog and save the current in-memory transcript.
/// Automatic logging is streamed separately, so this manual action can still
/// export a bounded snapshot without changing the live writer.
pub async fn save_current_session(
    app: &AppHandle,
    tab_id: &str,
) -> Result<Option<String>, AppError> {
    let state = app.state::<WorkspaceState>();
    let (title, session_type, profile_id, transcript) = {
        let (title, session_type, profile_id) = {
            let tabs = state.tabs.read().await;
            let tab = tabs
                .iter()
                .find(|tab| tab.id == tab_id)
                .ok_or_else(|| AppError::Storage("会话不存在".to_string()))?;
            (
                tab.title.clone(),
                tab.session_type.clone(),
                tab.profile_id.clone(),
            )
        };
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(tab_id)
            .ok_or_else(|| AppError::Storage("会话状态不存在".to_string()))?;
        (
            title,
            session_type,
            profile_id,
            session.terminal_transcript.clone(),
        )
    };
    let automatic_serial_log = state
        .session_log_writers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .filter(|handle| handle.options.serial);

    if session_type == "ftp" {
        return Err(AppError::Command("FTP 会话没有终端日志".to_string()));
    }

    let default_directory = crate::storage::read_json_array(app, "profiles.json")?
        .into_iter()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id.as_str()))
        .and_then(|profile| {
            profile
                .get("sessionLogDirectory")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        });

    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("保存会话日志")
        .set_file_name(format!(
            "{}-{}.log",
            sanitize_filename(&title),
            unix_millis()
        ))
        .add_filter("会话日志", &["log", "txt"]);
    if let Some(directory) = default_directory.filter(|path| path.is_dir()) {
        dialog = dialog.set_directory(directory);
    }
    let Some(target) = dialog.save_file().await else {
        return Ok(None);
    };

    let header = format!(
        "# FileTerm 会话日志\r\n# 连接: {title}\r\n# 只记录终端输出，不记录键盘输入\r\n\r\n"
    );
    if let Some(handle) = automatic_serial_log {
        sync_handle(&handle).await;
        let backup = PathBuf::from(format!("{}.1", handle.path.to_string_lossy()));
        let mut snapshot = Vec::new();
        match fs::read(&backup).await {
            Ok(previous) => snapshot.extend(previous),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::Storage(format!("无法读取轮转会话日志: {error}"))),
        }
        let current = fs::read(&handle.path)
            .await
            .map_err(|error| AppError::Storage(format!("无法读取会话日志: {error}")))?;
        snapshot.extend(current);
        fs::write(target.path(), snapshot)
            .await
            .map_err(|error| AppError::Storage(format!("无法保存会话日志: {error}")))?;
    } else {
        let content = format!("{header}{transcript}");
        tokio::fs::write(target.path(), content.as_bytes())
            .await
            .map_err(|error| AppError::Storage(format!("无法保存会话日志: {error}")))?;
    }
    Ok(Some(target.path().to_string_lossy().into_owned()))
}

async fn flush_handle(handle: SessionLogHandle) {
    let (sender, receiver) = oneshot::channel();
    if matches!(
        tokio::time::timeout(
            FLUSH_TIMEOUT,
            handle.sender.send(SessionLogMessage::Flush(sender))
        )
        .await,
        Ok(Ok(()))
    ) {
        let _ = tokio::time::timeout(FLUSH_TIMEOUT, receiver).await;
    }
}

async fn sync_handle(handle: &SessionLogHandle) {
    let (sender, receiver) = oneshot::channel();
    if matches!(
        tokio::time::timeout(
            FLUSH_TIMEOUT,
            handle.sender.send(SessionLogMessage::Sync(sender))
        )
        .await,
        Ok(Ok(()))
    ) {
        let _ = tokio::time::timeout(FLUSH_TIMEOUT, receiver).await;
    }
}

async fn run_writer(
    file: tokio::fs::File,
    mut receiver: mpsc::Receiver<SessionLogMessage>,
    dropped_chunks: Arc<AtomicU64>,
    app: &AppHandle,
    tab_id: &str,
    path: &Path,
) {
    let mut bytes_written = file
        .metadata()
        .await
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    let mut file = Some(file);
    while let Some(message) = receiver.recv().await {
        let dropped = dropped_chunks.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            let notice = format!("# FileTerm 警告：日志队列繁忙，已丢弃 {dropped} 个输出片段\r\n");
            if bytes_written.saturating_add(notice.len() as u64) > MAX_LOG_BYTES {
                match rotate_log(file.take().expect("session log file exists"), path).await {
                    Ok((next, size)) => {
                        file = Some(next);
                        bytes_written = size;
                    }
                    Err(error) => {
                        notify_writer_failure(app, tab_id, format!("轮转会话日志失败：{error}"))
                            .await;
                        break;
                    }
                }
            }
            let Some(file_ref) = file.as_mut() else {
                break;
            };
            if let Err(error) = file_ref.write_all(notice.as_bytes()).await {
                notify_writer_failure(app, tab_id, format!("写入日志队列丢弃提示失败：{error}"))
                    .await;
                break;
            }
            bytes_written = bytes_written.saturating_add(notice.len() as u64);
        }
        match message {
            SessionLogMessage::Chunk(chunk) => {
                if bytes_written.saturating_add(chunk.len() as u64) > MAX_LOG_BYTES {
                    match rotate_log(file.take().expect("session log file exists"), path).await {
                        Ok((next, size)) => {
                            file = Some(next);
                            bytes_written = size;
                        }
                        Err(error) => {
                            notify_writer_failure(
                                app,
                                tab_id,
                                format!("轮转会话日志失败：{error}"),
                            )
                            .await;
                            break;
                        }
                    }
                }
                let Some(file_ref) = file.as_mut() else {
                    break;
                };
                if let Err(error) = file_ref.write_all(chunk.as_bytes()).await {
                    notify_writer_failure(app, tab_id, format!("写入会话日志失败：{error}")).await;
                    break;
                }
                bytes_written = bytes_written.saturating_add(chunk.len() as u64);
            }
            SessionLogMessage::Sync(sender) => {
                if let Some(file_ref) = file.as_mut() {
                    let _ = file_ref.flush().await;
                }
                let _ = sender.send(());
            }
            SessionLogMessage::Flush(sender) => {
                if let Some(file_ref) = file.as_mut() {
                    let _ = file_ref.flush().await;
                }
                let _ = sender.send(());
                break;
            }
        }
    }

    if let Some(mut file) = file {
        if let Err(error) = file.flush().await {
            notify_writer_failure(
                app,
                tab_id,
                format!("刷新会话日志失败（{}）：{error}", path.display()),
            )
            .await;
        }
    }
    let state = app.state::<WorkspaceState>();
    let mut writers = state.session_log_writers.write().await;
    if writers
        .get(tab_id)
        .is_some_and(|handle| handle.path == path)
    {
        writers.remove(tab_id);
    }
}

async fn notify_writer_failure(app: &AppHandle, tab_id: &str, message: String) {
    crate::services::logging::warn(
        app,
        "session-log",
        format!("session log writer failed tab={tab_id}: {message}"),
    );
    // A failed automatic writer must be visible in the active tab. The
    // diagnostic log is useful for developers, but otherwise the user can
    // keep working while assuming that the file is still being saved.
    crate::sessions::terminal::emit_terminal_data(
        app,
        tab_id,
        &format!("\r\n[会话日志] 会话日志写入失败：{message}\r\n"),
    )
    .await;
}

async fn rotate_log(
    file: tokio::fs::File,
    path: &Path,
) -> Result<(tokio::fs::File, u64), std::io::Error> {
    let mut file = file;
    file.flush().await?;
    drop(file);
    let backup = PathBuf::from(format!("{}.1", path.to_string_lossy()));
    let _ = fs::remove_file(&backup).await;
    fs::rename(path, &backup).await?;
    let mut next = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    let header = format!(
        "# FileTerm 会话日志继续写入\r\n# 轮转时间（本机时区）: {}\r\n\r\n",
        timestamp_rfc3339()
    );
    next.write_all(header.as_bytes()).await?;
    next.flush().await?;
    let size = header.len() as u64;
    Ok((next, size))
}

fn configured_directory(app: &AppHandle, profile: &Value) -> Result<PathBuf, AppError> {
    if let Some(path) = profile
        .get("sessionLogDirectory")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return Ok(PathBuf::from(path));
    }
    Ok(crate::storage::state_path(app)?.with_file_name(DEFAULT_DIRECTORY_NAME))
}

fn format_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02X}");
    }
    output
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn local_filename_timestamp() -> String {
    Local::now().format("%Y%m%d-%H%M%S%.3f%z").to_string()
}

fn timestamp_rfc3339() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn short_tab_id(tab_id: &str) -> String {
    tab_id
        .rsplit('-')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("tab")
        .chars()
        .take(12)
        .collect()
}

fn sanitize_filename(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(|character| character == '.' || character == ' ');
    let trimmed = trimmed.chars().take(80).collect::<String>();
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::{local_filename_timestamp, sanitize_filename, short_tab_id, timestamp_rfc3339};

    #[test]
    fn sanitizes_cross_platform_filename_characters() {
        assert_eq!(sanitize_filename("设备:/测试?"), "设备__测试_");
        assert_eq!(sanitize_filename("..."), "session");
        assert!(sanitize_filename(&"x".repeat(100)).len() <= 80);
    }

    #[test]
    fn keeps_a_short_tab_suffix() {
        assert_eq!(short_tab_id("tab-1234567890abcdef"), "1234567890ab");
        assert_eq!(short_tab_id("local"), "local");
    }

    #[test]
    fn formats_local_timestamp_with_an_explicit_offset() {
        let timestamp = timestamp_rfc3339();
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
        assert!(timestamp.contains('T'));
        assert!(local_filename_timestamp().contains('-'));
    }
}
