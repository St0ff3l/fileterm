use crate::sessions::WorkerCmd;
use crate::storage::read_json_array;
use crate::AppError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
#[cfg(target_os = "windows")]
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{ipc::Channel, AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroize;

/// 等待 worker 接收命令的最大时间。worker 主循环被 SFTP init / shell
/// channel 写阻塞 时，mpsc 一旦满，send 会永久 await，导致前端 invoke
/// 链路整体卡死（多窗口发送后续 tab 全部排队、Cmd+Q 退出无法完成）。
/// 超时后返回显式 busy 错误，绝不静默吞掉输入。SSH 终端输入已经走
/// 独立 channel；这里仍作为 Telnet / Serial 和通用 worker 命令的保护。
const WORKER_CMD_SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// 文件/会话级操作（list/read/write/重连等）容忍更长延迟，但同样不能
/// 永久阻塞——一旦 worker 卡死，应当让前端拿到明确错误。
const WORKER_FILE_CMD_SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Worker 已接收命令后也必须在有限时间内答复。之前仅限制了 mpsc send，
/// 但某个后台 SFTP/exec task 丢失 reply 时，oneshot 会一直 await，导致
/// 删除、打开目录和 Root 弹窗永久 loading。
const WORKER_FILE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// 退出时给 worker 的 Disconnect 命令留 1 秒，超时直接放弃发送：worker
/// 主循环卡死时 channel 满，send 不进去；强行 await 会让 Cmd+Q 整个
/// 退出链路 hang 住，用户只能强制杀进程。drop sender 后 worker 的
/// `cmd_rx.recv()` 会返回 None，自然走清理路径。
const WORKER_DISCONNECT_TIMEOUT: Duration = Duration::from_secs(1);

const SERIAL_TRANSFER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Do not start another connection test for the same endpoint immediately
/// after the previous one. Some SSH servers enforce a strict unauthenticated
/// connection rate and return only `Disconnected` when that limit is hit.
const CONNECTION_TEST_RETRY_COOLDOWN: Duration = Duration::from_secs(5);

/// A local tab should become connected once its PTY transport is ready. The
/// background startup task keeps this bounded window as a guard for a failed
/// readiness signal; the shell's first visible prompt is not a prerequisite
/// for a usable terminal.
const LOCAL_TERMINAL_STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(2);

/// Let a child-window close command resolve its IPC callback before destroying
/// the calling WebView. Destroying synchronously makes WebView2 report a
/// missing callback id and can leave renderer cleanup half-finished.
const CHILD_WINDOW_DESTROY_DELAY: Duration = Duration::from_millis(25);
include!("terminal_input.rs");
include!("connection_preferences.rs");
include!("ui_preferences.rs");
include!("platform_commands.rs");
include!("serial_commands.rs");
include!("ui_preferences_commands.rs");
include!("ai_commands.rs");
include!("workspace_commands.rs");
include!("import_commands.rs");
include!("window_commands.rs");
include!("session_runtime.rs");
include!("session_spawn.rs");
include!("tab_layout.rs");
include!("tab_lifecycle.rs");
include!("terminal_commands.rs");
include!("remote_file_commands.rs");
include!("transfer_commands.rs");
include!("tunnel_commands.rs");
include!("interaction_commands.rs");
include!("profile_commands.rs");
include!("tests.rs");
