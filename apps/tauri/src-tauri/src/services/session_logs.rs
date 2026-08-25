//! Per-session terminal transcript logging.
//!
//! Session logs are deliberately separate from the diagnostic logger. They
//! contain decoded terminal output only, never terminal input, passwords, or
//! other renderer-side secrets. Automatic logging uses a small async writer
//! queue so a slow disk cannot block an SSH/serial worker.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

enum SessionLogMessage {
    Chunk(String),
    Sync(oneshot::Sender<()>),
    Flush(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct SessionLogHandle {
    sender: mpsc::UnboundedSender<SessionLogMessage>,
    options: SessionLogOptions,
    path: PathBuf,
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
    let timestamp = unix_millis();
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
            "# FileTerm 会话日志\r\n# 连接: {name}\r\n# 串口日志包含 RX{}，原始字节: {}，时间戳: {}\r\n\r\n",
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

    let (sender, receiver) = mpsc::unbounded_channel();
    let handle = SessionLogHandle {
        sender: sender.clone(),
        options,
        path: path.clone(),
    };
    state
        .session_log_writers
        .write()
        .await
        .insert(tab_id.to_string(), handle);

    let app = app.clone();
    let tab_id = tab_id.to_string();
    tauri::async_runtime::spawn(async move {
        run_writer(file, receiver, &app, &tab_id, &path).await;
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
        let _ = handle
            .sender
            .send(SessionLogMessage::Chunk(chunk.to_string()));
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
    if bytes.is_empty() {
        return;
    }
    let state = app.state::<WorkspaceState>();
    let handle = state.session_log_writers.read().await.get(tab_id).cloned();
    let Some(handle) = handle.filter(|handle| {
        handle.options.serial
            && (direction == SerialLogDirection::Rx || handle.options.include_input)
    }) else {
        return;
    };

    let payload = if handle.options.raw {
        format_hex(bytes)
    } else {
        decoded
            .map(str::to_string)
            .unwrap_or_else(|| decode_terminal(bytes, encoding))
    };
    let timestamp = if handle.options.timestamps {
        format!("[{}] ", unix_millis())
    } else {
        String::new()
    };
    let direction = match direction {
        SerialLogDirection::Rx => "RX",
        SerialLogDirection::Tx => "TX",
    };
    let _ = handle.sender.send(SessionLogMessage::Chunk(format!(
        "{timestamp}{direction}: {payload}\r\n"
    )));
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
        tokio::fs::copy(&handle.path, target.path())
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
    if handle.sender.send(SessionLogMessage::Flush(sender)).is_ok() {
        let _ = tokio::time::timeout(FLUSH_TIMEOUT, receiver).await;
    }
}

async fn sync_handle(handle: &SessionLogHandle) {
    let (sender, receiver) = oneshot::channel();
    if handle.sender.send(SessionLogMessage::Sync(sender)).is_ok() {
        let _ = tokio::time::timeout(FLUSH_TIMEOUT, receiver).await;
    }
}

async fn run_writer(
    mut file: tokio::fs::File,
    mut receiver: mpsc::UnboundedReceiver<SessionLogMessage>,
    app: &AppHandle,
    tab_id: &str,
    path: &Path,
) {
    while let Some(message) = receiver.recv().await {
        match message {
            SessionLogMessage::Chunk(chunk) => {
                if let Err(error) = file.write_all(chunk.as_bytes()).await {
                    crate::services::logging::warn(
                        app,
                        "session-log",
                        format!("写入会话日志失败 tab={tab_id}: {error}"),
                    );
                    break;
                }
            }
            SessionLogMessage::Sync(sender) => {
                let _ = file.flush().await;
                let _ = sender.send(());
            }
            SessionLogMessage::Flush(sender) => {
                let _ = file.flush().await;
                let _ = sender.send(());
                break;
            }
        }
    }

    if let Err(error) = file.flush().await {
        crate::services::logging::warn(
            app,
            "session-log",
            format!(
                "刷新会话日志失败 tab={tab_id} path={}: {error}",
                path.display()
            ),
        );
    }
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
    use super::{sanitize_filename, short_tab_id};

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
}
