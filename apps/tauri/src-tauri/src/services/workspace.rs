use crate::services::connection_operations::ConnectionOperationRegistry;
use crate::services::transfers::TransferTask;
use crate::sessions::WorkerCmd;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::ipc::Channel;
use tokio::sync::{oneshot, watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct TransferRunHandle {
    pub generation: u64,
    pub cancel: CancellationToken,
    pub settled: watch::Receiver<bool>,
}

/// Coordinates output from one local PTY runtime with shutdown/reconnect.
///
/// The reader runs on a native thread while reconnect and close are async
/// commands. Keeping an async lock around the final output publication lets a
/// shutdown wait briefly for an in-flight chunk, then deactivate the old
/// runtime before a replacement shell is installed for the same tab.
pub struct LocalTerminalRuntimeGate {
    pub(crate) active: AtomicBool,
    pub(crate) emit_lock: Mutex<()>,
}

impl LocalTerminalRuntimeGate {
    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(true),
            emit_lock: Mutex::new(()),
        }
    }

    pub async fn deactivate(&self) {
        // Flip the fast-path first so a reader that wakes while shutdown is
        // waiting cannot enqueue another chunk. The short lock wait lets an
        // already-publishing chunk finish in the normal case, but a stalled
        // renderer channel must never make close/reconnect hang forever.
        self.active.store(false, Ordering::Release);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(250), self.emit_lock.lock())
            .await;
    }
}

impl Default for LocalTerminalRuntimeGate {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferRunHandle {
    pub async fn wait_until_settled(mut self) {
        if *self.settled.borrow() {
            return;
        }
        while self.settled.changed().await.is_ok() {
            if *self.settled.borrow() {
                return;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TransferProgressSample {
    pub bytes: u64,
    pub sampled_at: std::time::Instant,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTab {
    pub id: String,
    pub profile_id: String,
    pub session_type: String,
    pub title: String,
    pub layout: String, // "terminal-file" | "file-only" | "terminal-only"
    pub status: WorkspaceTabStatus,
    /// 分屏树根节点；普通 tab 为 None。只有分屏的根 tab 持有。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_root: Option<PaneNode>,
    /// 分屏 leaf 所属的顶层 workspace tab。leaf 仍保留独立 session，但绝不作为顶栏 tab。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_root_tab_id: Option<String>,
}

/// 分屏方向：row = 左右分（垂直分屏），column = 上下分（水平分屏）
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SplitDirection {
    Row,
    Column,
}

/// 分屏树节点。leaf 引用一个真实 tab id；split 递归持有子节点。
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PaneNode {
    Leaf {
        // The renderer's core contract uses `tabId`. Keep accepting the
        // previous Rust-shaped spelling so a restored in-memory snapshot does
        // not make the layout unreadable during an upgrade.
        #[serde(rename = "tabId", alias = "tab_id")]
        tab_id: String,
    },
    Split {
        direction: SplitDirection,
        children: Vec<PaneNode>,
        /// 每个子节点占比，长度与 children 一致，和为 1。
        weights: Vec<f32>,
    },
}

impl PaneNode {
    /// 递归收集所有 leaf 的 tab_id。
    pub fn leaf_tab_ids(&self) -> Vec<String> {
        match self {
            PaneNode::Leaf { tab_id } => vec![tab_id.clone()],
            PaneNode::Split { children, .. } => {
                children.iter().flat_map(|c| c.leaf_tab_ids()).collect()
            }
        }
    }

    /// 递归查找并替换指定 leaf 节点为新子树。返回是否替换成功。
    /// 用于分屏：把当前 leaf 替换为 split(leaf, new_leaf)。
    pub fn replace_leaf(&mut self, target_tab_id: &str, replacement: PaneNode) -> bool {
        match self {
            PaneNode::Leaf { tab_id } if tab_id == target_tab_id => {
                *self = replacement;
                true
            }
            PaneNode::Leaf { .. } => false,
            PaneNode::Split { children, .. } => {
                for child in children.iter_mut() {
                    if child.replace_leaf(target_tab_id, replacement.clone()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// 递归移除指定 leaf 节点。移除后规整：
    ///
    /// - split 只剩一个子节点时，用唯一子节点替换该 split
    ///
    /// 返回 true 表示子树中发生了移除。
    pub fn remove_leaf(&mut self, target_tab_id: &str) -> bool {
        match self {
            PaneNode::Leaf { tab_id } if tab_id == target_tab_id => true,
            PaneNode::Leaf { .. } => false,
            PaneNode::Split {
                children, weights, ..
            } => {
                // 先检查直接 children 里有没有匹配的 leaf
                let direct_idx = children.iter().position(|c| match c {
                    PaneNode::Leaf { tab_id } => tab_id == target_tab_id,
                    PaneNode::Split { .. } => false,
                });

                if let Some(idx) = direct_idx {
                    children.remove(idx);
                    if weights.len() > idx {
                        weights.remove(idx);
                    }
                } else {
                    // 递归到子 split
                    let mut found_in_child = false;
                    for child in children.iter_mut() {
                        if child.remove_leaf(target_tab_id) {
                            found_in_child = true;
                            break;
                        }
                    }
                    if !found_in_child {
                        return false;
                    }
                }

                normalize_weights(weights);
                // 规整：split 只剩一个子节点时用唯一子节点替换自身
                if children.len() == 1 {
                    let only = children.pop().unwrap();
                    *self = only;
                }
                true
            }
        }
    }

    /// 更新指定 split 节点的 weights。`pane_path` 使用从根节点到该 split 的 child index。
    pub fn set_split_weights_at_path(&mut self, pane_path: &[usize], next_weights: &[f32]) -> bool {
        let PaneNode::Split {
            children, weights, ..
        } = self
        else {
            return false;
        };

        if pane_path.is_empty() {
            if weights.len() != next_weights.len() {
                return false;
            }
            weights.clone_from_slice(next_weights);
            normalize_weights(weights);
            return true;
        }

        let Some(child) = children.get_mut(pane_path[0]) else {
            return false;
        };
        child.set_split_weights_at_path(&pane_path[1..], next_weights)
    }
}

/// 归一化 weights 数组：确保和为 1，且长度 >= 2 时每项 > 0。
fn normalize_weights(weights: &mut [f32]) {
    if weights.is_empty() {
        return;
    }
    let sum: f32 = weights.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        let n = weights.len() as f32;
        for w in weights.iter_mut() {
            *w = 1.0 / n;
        }
        return;
    }
    for w in weights.iter_mut() {
        *w /= sum;
    }
}

/// Rust-side mirror of `packages/core::TabStatus`. Keeping this as an enum
/// prevents backend-only strings such as `disconnected` from leaking into the
/// renderer and silently breaking menus/status views.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceTabStatus {
    Idle,
    Connecting,
    Connected,
    Error,
    Closed,
}

impl WorkspaceTabStatus {
    pub fn is_connected(self) -> bool {
        self == Self::Connected
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCapabilities {
    pub terminal: bool,
    pub files: bool,
    pub resource_monitoring: bool,
    pub shell_integration: bool,
    pub file_access: bool,
    pub tunnels: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDiskSpace {
    pub available_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileCapabilities {
    pub protocol: String,
    pub protocol_version: Option<String>,
    pub extensions: Vec<String>,
    pub checksum_algorithms: Vec<String>,
    pub disk_space: Option<RemoteDiskSpace>,
    pub server_copy: bool,
    pub symlink: bool,
    pub hardlink: bool,
}

impl ConnectionCapabilities {
    /// Return whether an SSH profile explicitly targets an interactive network
    /// device instead of a POSIX/Windows server. Missing and unknown values
    /// deliberately fall back to the legacy server behavior.
    pub fn is_network_device_profile(profile: &serde_json::Value) -> bool {
        profile.get("type").and_then(serde_json::Value::as_str) == Some("ssh")
            && profile
                .get("deviceMode")
                .and_then(serde_json::Value::as_str)
                == Some("network-device")
    }

    pub fn for_profile(profile: &serde_json::Value) -> Self {
        if Self::is_network_device_profile(profile) {
            return Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: true,
            };
        }

        Self::for_session_type(
            profile
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("ssh"),
        )
    }

    pub fn for_session_type(session_type: &str) -> Self {
        match session_type {
            "ssh" => Self {
                terminal: true,
                files: true,
                resource_monitoring: true,
                shell_integration: true,
                file_access: true,
                tunnels: true,
            },
            "ftp" => Self {
                terminal: false,
                files: true,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
            "local" => Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
            _ => Self {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub profile_id: String,
    /// Monotonic terminal-target identity. This must not change for ordinary
    /// terminal output; it changes only when the interactive target changes.
    pub ai_session_revision: String,
    /// Effective SSH session mode after banner resolution. `auto` is never
    /// exposed here: a connected session is either a normal server or a
    /// network device, while a connecting legacy snapshot keeps this empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_mode: Option<String>,
    pub access_host: String,
    pub summary: String,
    pub terminal_transcript: String,
    pub remote_path: String,
    pub shell_cwd: Option<String>,
    pub follow_shell_cwd: bool,
    pub remote_files_loading: bool,
    pub remote_files: Vec<serde_json::Value>,
    pub sftp_unavailable_reason: Option<String>,
    pub file_access_mode: String, // "user" | "root"
    pub sudo_user: Option<String>,
    pub has_reusable_sudo_auth: bool,
    /// 登录用户（首次 OSC 1337 RemoteUser= 观察到的用户，或 profile.username）。
    /// 用于判断 shell 用户是否变化以自动切 root 视角。
    pub login_user: Option<String>,
    /// 当前 shell 观察到的用户（OSC 1337 RemoteUser=）。与 sudo_user 分开：
    /// sudo_user 是用户显式配置的 sudo 目标，shell_user 是终端实际运行用户。
    pub shell_user: Option<String>,
    pub connected: bool,
    pub system_metrics: Option<serde_json::Value>,
    pub capabilities: ConnectionCapabilities,
    pub remote_capabilities: Option<RemoteFileCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_mode: Option<String>,
}

/// Return the reconnect policy that belongs to a persisted network/device
/// profile. The runtime uses this for the status snapshot as well as for the
/// reconnect action; local sessions deliberately have no network policy.
pub fn reconnect_mode_for_profile(profile: &serde_json::Value) -> Option<String> {
    let profile_type = profile.get("type").and_then(serde_json::Value::as_str);
    if !matches!(profile_type, Some("ssh" | "ftp" | "telnet" | "serial")) {
        return None;
    }

    Some(
        profile
            .get("reconnectMode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("none")
            .to_string(),
    )
}

/// Return the explicit SSH mode that is safe to expose while a session is
/// reconnecting. Auto detection is resolved only after the SSH handshake, so
/// callers must clear the old effective mode until the new banner is known.
pub fn configured_device_mode_for_profile(profile: &serde_json::Value) -> Option<String> {
    if profile.get("type").and_then(serde_json::Value::as_str) != Some("ssh") {
        return None;
    }

    match profile
        .get("deviceMode")
        .and_then(serde_json::Value::as_str)
    {
        Some("server") => Some("server".to_string()),
        Some("network-device") => Some("network-device".to_string()),
        _ => None,
    }
}

/// Initial browser path for file-capable sessions. SSH follows Electron's
/// `currentRemotePath` default (`.`), while FTP keeps its protocol root `/`.
pub fn initial_remote_path_for_profile(profile: &serde_json::Value) -> String {
    if let Some(path) = profile
        .get("remotePath")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        return path.to_string();
    }
    match profile
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ssh")
    {
        "ssh" => ".".to_string(),
        "ftp" => "/".to_string(),
        _ => String::new(),
    }
}

/// The local endpoint to connect when an SSH server opens a `forwarded-tcpip`
/// channel for a remote (`-R`) rule. These stay main-process only; renderer
/// receives the public tunnel snapshot through the command result instead.
#[derive(Clone, Debug)]
pub struct RemoteForwardTarget {
    pub bind_host: String,
    pub bind_port: u32,
    pub target_host: String,
    pub target_port: u16,
}

#[derive(Clone, Debug)]
pub struct ConnectionImportPlanEntry {
    pub preview: serde_json::Value,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPasswordResponse {
    pub cancelled: bool,
    pub value: Option<String>,
}

pub struct PendingBackupPassword {
    pub sender: oneshot::Sender<BackupPasswordResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SudoPasswordResponse {
    pub cancelled: bool,
    pub value: Option<String>,
    pub save: bool,
}

pub struct PendingSudoPassword {
    pub tab_id: String,
    pub expected_session_revision: String,
    pub sender: oneshot::Sender<SudoPasswordResponse>,
}

pub struct WorkspaceState {
    pub tabs: Arc<RwLock<Vec<WorkspaceTab>>>,
    pub active_tab_id: Arc<RwLock<Option<String>>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionSnapshot>>>,
    pub workers: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<WorkerCmd>>>>,
    /// Async writers for opt-in per-device terminal output logs.
    pub session_log_writers:
        Arc<RwLock<HashMap<String, crate::services::session_logs::SessionLogHandle>>>,
    /// High-frequency SSH keystrokes bypass the general worker command queue.
    /// The SSH worker drains and coalesces this channel before writing to the
    /// PTY, so file commands cannot fill the bounded queue and reject input.
    pub terminal_inputs: Arc<RwLock<HashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>>,
    /// Tauri IPC channels are the ordered streaming boundary for terminal
    /// output. Ordinary app events remain appropriate for low-frequency state
    /// updates, but can fall behind sustained PTY traffic in WKWebView.
    pub terminal_output_channels: Arc<StdMutex<HashMap<u32, Channel<serde_json::Value>>>>,
    /// Cancels the runtime owned by each worker. Dropping the command sender
    /// alone cannot interrupt a worker that is currently parsing a large
    /// remote metrics payload or waiting on an SSH operation.
    pub worker_controls: Arc<RwLock<HashMap<String, CancellationToken>>>,
    /// Number of consecutive automatic serial reconnect attempts per tab.
    /// This is runtime-only state; a successful connection or an explicit
    /// disconnect clears it so a later outage starts with the initial delay.
    pub serial_reconnect_attempts: Arc<RwLock<HashMap<String, u32>>>,
    /// Cancellation tokens for the one active serial transfer per tab.
    /// Keeping this separate from the worker token lets the renderer cancel a
    /// transfer without tearing down the serial session itself.
    pub serial_transfer_cancellations: Arc<RwLock<HashMap<String, (String, CancellationToken)>>>,
    /// Identifies the live local PTY for each local tab. Native-thread cleanup
    /// must never remove a newer shell restarted in the same tab.
    pub local_terminal_runtime_ids: Arc<RwLock<HashMap<String, String>>>,
    /// Gates terminal output from each local PTY runtime. The gate is separate
    /// from the id map so a stale reader can be stopped before a replacement
    /// runtime is allowed to publish output for the same tab.
    pub local_terminal_runtime_gates: Arc<RwLock<HashMap<String, Arc<LocalTerminalRuntimeGate>>>>,
    /// One-shot launch configuration retained only for the lifetime of a local
    /// tab. It is needed when that tab is reconnected, but must never be part
    /// of the renderer snapshot or persisted connection profiles.
    pub local_terminal_launches:
        Arc<RwLock<HashMap<String, crate::sessions::local_terminal::LocalTerminalLaunch>>>,
    /// Pending SSH interaction requests (host-key verification, MFA prompts).
    /// The renderer resolves each one via `app_resolve_ssh_interaction`.
    pub pending_interactions: Arc<RwLock<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    /// Event-driven state for connection opens requested by CLI/MCP callers.
    /// It carries no credentials or terminal output.
    pub connection_operations: Arc<ConnectionOperationRegistry>,
    /// One-time password prompts for cross-device remote backup encryption.
    /// These are intentionally separate from terminal and remote-exec input.
    pub pending_backup_passwords: Arc<RwLock<HashMap<String, PendingBackupPassword>>>,
    pub backup_password_renderer_registration: Arc<RwLock<Option<String>>>,
    /// One-time sudo/su prompts for the normal isolated exec channel. These
    /// values are never routed through terminal input or an Agent context.
    pub pending_sudo_passwords: Arc<RwLock<HashMap<String, PendingSudoPassword>>>,
    pub sudo_password_renderer_registration: Arc<RwLock<Option<String>>>,
    /// Pending one-time action approvals. MCP and Copilot share this
    /// queue; the renderer resolves each request through
    /// `app_resolve_action_approval` or the Copilot terminal-handoff command.
    /// Dropping or timing out a request denies it, so there is no durable
    /// approval state.
    pub pending_action_approvals: Arc<
        RwLock<
            HashMap<
                String,
                oneshot::Sender<crate::services::action_review::ActionApprovalDecision>,
            >,
        >,
    >,
    pub remote_forwards: Arc<RwLock<HashMap<String, Vec<RemoteForwardTarget>>>>,
    /// Transfer snapshots are durable domain state. Run handles are
    /// runtime-only and never serialized to the renderer or journal. A
    /// generation prevents an older run from deleting a newer run's handle.
    pub transfers: Arc<RwLock<Vec<TransferTask>>>,
    pub transfer_runs: Arc<RwLock<HashMap<String, TransferRunHandle>>>,
    /// Serializes user-visible transfer lifecycle transitions. Commands can
    /// arrive concurrently from the main window and transfer popovers; this
    /// guard makes pause/resume/discard/clear/shutdown compare-and-set as one
    /// operation instead of allowing a new run between cancel and persist.
    pub transfer_lifecycle: Arc<Mutex<()>>,
    pub next_transfer_generation: Arc<AtomicU64>,
    pub transfer_journal_loaded: Arc<Mutex<bool>>,
    /// Serializes the complete journal snapshot write. Multiple independent
    /// transfers can finish on different runtime threads; without this guard
    /// their shared temp/backup files and stale snapshots can overwrite one
    /// another.
    pub transfer_journal_write: Arc<Mutex<()>>,
    pub transfer_last_event: Arc<Mutex<HashMap<String, std::time::Instant>>>,
    pub transfer_progress_samples: Arc<Mutex<HashMap<String, TransferProgressSample>>>,
    /// Import plans retain sanitized source data in main process until the
    /// renderer confirms a selected subset and conflict strategy.
    pub connection_import_plans: Arc<RwLock<HashMap<String, Vec<ConnectionImportPlanEntry>>>>,
    /// At most one connection test may be active for a profile/endpoint.
    /// Keeping this in the backend also covers duplicate clicks from multiple
    /// renderer windows and prevents a stalled host-key prompt from opening a
    /// burst of SSH handshakes against the same server.
    pub connection_tests_in_flight: Arc<Mutex<HashSet<String>>>,
    /// Keep a short per-endpoint cooldown after a test starts. A fast remote
    /// rejection can resolve the command before the renderer has time to
    /// repaint its disabled button, so sequential clicks still need a
    /// backend-side guard against hammering an SSHD's unauthenticated limit.
    pub connection_tests_last_started: Arc<Mutex<HashMap<String, std::time::Instant>>>,
    /// Serializes profile/folder/command read-modify-write transactions from
    /// independent Tauri windows. Unlike Electron's single main event loop,
    /// Tauri commands can otherwise overwrite each other's JSON snapshots.
    pub library_mutation: Arc<Mutex<()>>,
    pub update_status: Arc<RwLock<Option<serde_json::Value>>>,
    /// Matches Electron's update check single-flight promise. Concurrent UI
    /// clicks wait for the active check and reuse its final status.
    pub update_check: Arc<Mutex<()>>,
    /// Serializes updater downloads and installation so a double click cannot
    /// start competing installers or overwrite a verified package in memory.
    pub update_operation: Arc<Mutex<()>>,
    /// Serializes snapshot assembly so the revision assigned to a snapshot
    /// follows the order in which snapshots are captured, even when multiple
    /// Tauri commands and background workers request one concurrently.
    pub workspace_snapshot_lock: Arc<Mutex<()>>,
    /// Monotonic revision carried by every workspace snapshot. The renderer
    /// uses it to ignore a late IPC response that was captured before a newer
    /// workspace event.
    pub next_workspace_snapshot_revision: Arc<AtomicU64>,
    /// 分屏 root tabId -> 当前活跃 leaf tabId。用于终端输入/文件操作/命令发送定位。
    pub active_pane_tab_id_by_root: Arc<RwLock<HashMap<String, String>>>,
    /// Monotonic identity revision for terminal targets exposed to the AI
    /// context-preview contract. It deliberately does not change for every
    /// terminal output chunk; it changes when the connected target, shell
    /// identity, or shell CWD changes so a reviewed snapshot cannot silently
    /// cross a reconnect or target transition.
    pub ai_session_revisions: Arc<RwLock<HashMap<String, u64>>>,
    /// Windows keeps the verified updater payload in memory until the user
    /// confirms the restart. It is intentionally never persisted to user data.
    #[cfg(target_os = "windows")]
    pub windows_downloaded_update:
        Arc<Mutex<Option<crate::services::updates::WindowsDownloadedUpdate>>>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            tabs: Arc::new(RwLock::new(Vec::new())),
            active_tab_id: Arc::new(RwLock::new(None)),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            session_log_writers: Arc::new(RwLock::new(HashMap::new())),
            terminal_inputs: Arc::new(RwLock::new(HashMap::new())),
            terminal_output_channels: Arc::new(StdMutex::new(HashMap::new())),
            worker_controls: Arc::new(RwLock::new(HashMap::new())),
            serial_reconnect_attempts: Arc::new(RwLock::new(HashMap::new())),
            serial_transfer_cancellations: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_runtime_ids: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_runtime_gates: Arc::new(RwLock::new(HashMap::new())),
            local_terminal_launches: Arc::new(RwLock::new(HashMap::new())),
            pending_interactions: Arc::new(RwLock::new(HashMap::new())),
            connection_operations: Arc::new(ConnectionOperationRegistry::default()),
            pending_backup_passwords: Arc::new(RwLock::new(HashMap::new())),
            backup_password_renderer_registration: Arc::new(RwLock::new(None)),
            pending_sudo_passwords: Arc::new(RwLock::new(HashMap::new())),
            sudo_password_renderer_registration: Arc::new(RwLock::new(None)),
            pending_action_approvals: Arc::new(RwLock::new(HashMap::new())),
            remote_forwards: Arc::new(RwLock::new(HashMap::new())),
            transfers: Arc::new(RwLock::new(Vec::new())),
            transfer_runs: Arc::new(RwLock::new(HashMap::new())),
            transfer_lifecycle: Arc::new(Mutex::new(())),
            next_transfer_generation: Arc::new(AtomicU64::new(0)),
            transfer_journal_loaded: Arc::new(Mutex::new(false)),
            transfer_journal_write: Arc::new(Mutex::new(())),
            transfer_last_event: Arc::new(Mutex::new(HashMap::new())),
            transfer_progress_samples: Arc::new(Mutex::new(HashMap::new())),
            connection_import_plans: Arc::new(RwLock::new(HashMap::new())),
            connection_tests_in_flight: Arc::new(Mutex::new(HashSet::new())),
            connection_tests_last_started: Arc::new(Mutex::new(HashMap::new())),
            library_mutation: Arc::new(Mutex::new(())),
            update_status: Arc::new(RwLock::new(None)),
            update_check: Arc::new(Mutex::new(())),
            update_operation: Arc::new(Mutex::new(())),
            workspace_snapshot_lock: Arc::new(Mutex::new(())),
            next_workspace_snapshot_revision: Arc::new(AtomicU64::new(0)),
            active_pane_tab_id_by_root: Arc::new(RwLock::new(HashMap::new())),
            ai_session_revisions: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(target_os = "windows")]
            windows_downloaded_update: Arc::new(Mutex::new(None)),
        }
    }
}

impl WorkspaceState {
    pub async fn set_backup_password_renderer_ready(&self, registration_id: &str, ready: bool) {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() || registration_id.len() > 200 {
            return;
        }
        let mut active = self.backup_password_renderer_registration.write().await;
        if ready {
            *active = Some(registration_id.to_string());
            return;
        }
        if active.as_deref() != Some(registration_id) {
            return;
        }
        *active = None;
        self.pending_backup_passwords.write().await.clear();
    }

    pub async fn insert_pending_backup_password(
        &self,
        request_id: String,
        pending: PendingBackupPassword,
    ) -> bool {
        let active = self.backup_password_renderer_registration.read().await;
        if active.is_none() {
            return false;
        }
        self.pending_backup_passwords
            .write()
            .await
            .insert(request_id, pending);
        true
    }

    pub async fn set_sudo_password_renderer_ready(&self, registration_id: &str, ready: bool) {
        let registration_id = registration_id.trim();
        if registration_id.is_empty() || registration_id.len() > 200 {
            return;
        }
        let mut active = self.sudo_password_renderer_registration.write().await;
        if ready {
            *active = Some(registration_id.to_string());
            return;
        }
        if active.as_deref() != Some(registration_id) {
            return;
        }
        *active = None;
        self.pending_sudo_passwords.write().await.clear();
    }

    pub async fn insert_pending_sudo_password(
        &self,
        request_id: String,
        pending: PendingSudoPassword,
    ) -> bool {
        // Keep the registration write lock across the readiness check and
        // pending-map insertion. Renderer teardown takes the same lock before
        // clearing pending senders, so a prompt can never be inserted after
        // readiness has been withdrawn.
        let active = self.sudo_password_renderer_registration.write().await;
        if active.is_none() {
            return false;
        }
        self.pending_sudo_passwords
            .write()
            .await
            .insert(request_id, pending);
        true
    }

    pub async fn has_sudo_password_renderer(&self) -> bool {
        self.sudo_password_renderer_registration
            .read()
            .await
            .is_some()
    }

    pub async fn ai_session_revision(&self, tab_id: &str) -> u64 {
        self.ai_session_revisions
            .read()
            .await
            .get(tab_id)
            .copied()
            .unwrap_or_default()
    }

    pub async fn touch_ai_session_revision(&self, tab_id: &str) -> u64 {
        let mut revisions = self.ai_session_revisions.write().await;
        let revision = revisions.entry(tab_id.to_string()).or_default();
        *revision = revision.saturating_add(1);
        *revision
    }

    pub async fn remove_ai_session_revision(&self, tab_id: &str) {
        self.ai_session_revisions.write().await.remove(tab_id);
    }

    pub fn register_terminal_output_channel(&self, channel: Channel<serde_json::Value>) {
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            channels.insert(channel.id(), channel);
        }
    }

    /// Broadcast a terminal output chunk to every registered renderer channel.
    ///
    /// The std Mutex is held only long enough to clone the channel list out;
    /// the per-channel `send` (which serializes the JSON payload and pushes
    /// it through Tauri's IPC bridge) runs **outside** the lock. Holding the
    /// lock during `send` was the original cause of multi-second worker-loop
    /// stalls when the webview fell behind on high-throughput output (e.g.
    /// `pacman-key --populate`): a single slow `channel.send` blocked the
    /// Tokio worker thread, which blocked `flush_batch`, which blocked the
    /// SSH `select!` from polling `terminal_input_rx` — so Ctrl+C stopped
    /// responding until the webview caught up.
    pub fn publish_terminal_output(&self, tab_id: &str, chunk: &str) {
        let payload = serde_json::json!({ "tabId": tab_id, "chunk": chunk });
        let snapshot: Vec<Channel<serde_json::Value>> = match self.terminal_output_channels.lock() {
            Ok(channels) => channels.values().cloned().collect(),
            Err(_) => return,
        };
        let mut dead_ids: Vec<u32> = Vec::new();
        for channel in &snapshot {
            if channel.send(payload.clone()).is_err() {
                dead_ids.push(channel.id());
            }
        }
        if !dead_ids.is_empty() {
            if let Ok(mut channels) = self.terminal_output_channels.lock() {
                for id in &dead_ids {
                    channels.remove(id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        configured_device_mode_for_profile, initial_remote_path_for_profile,
        reconnect_mode_for_profile, ConnectionCapabilities, PaneNode, SplitDirection,
        TransferRunHandle, WorkspaceState, WorkspaceTabStatus,
    };
    use std::sync::{Arc, Mutex};
    use tauri::ipc::Channel;

    #[test]
    fn ssh_is_the_only_session_type_with_tunnel_capability() {
        assert!(ConnectionCapabilities::for_session_type("ssh").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("ftp").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("telnet").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("serial").tunnels);
    }

    #[test]
    fn capabilities_serialize_with_the_core_camel_case_shape() {
        let value = serde_json::to_value(ConnectionCapabilities::for_session_type("ssh")).unwrap();

        assert_eq!(value["resourceMonitoring"], true);
        assert_eq!(value["shellIntegration"], true);
        assert_eq!(value["fileAccess"], true);
        assert_eq!(value["tunnels"], true);
    }

    #[test]
    fn network_device_profiles_expose_only_terminal_and_tunnels() {
        let profile = serde_json::json!({
            "type": "ssh",
            "deviceMode": "network-device"
        });

        assert!(ConnectionCapabilities::is_network_device_profile(&profile));
        assert_eq!(
            ConnectionCapabilities::for_profile(&profile),
            ConnectionCapabilities {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: true,
            }
        );
    }

    #[test]
    fn missing_or_auto_device_mode_keeps_legacy_server_capabilities() {
        for profile in [
            serde_json::json!({ "type": "ssh" }),
            serde_json::json!({ "type": "ssh", "deviceMode": "auto" }),
        ] {
            assert!(!ConnectionCapabilities::is_network_device_profile(&profile));
            assert_eq!(
                ConnectionCapabilities::for_profile(&profile),
                ConnectionCapabilities::for_session_type("ssh")
            );
        }
    }

    #[test]
    fn tab_status_serializes_to_the_core_union_values() {
        let statuses = [
            (WorkspaceTabStatus::Idle, "idle"),
            (WorkspaceTabStatus::Connecting, "connecting"),
            (WorkspaceTabStatus::Connected, "connected"),
            (WorkspaceTabStatus::Error, "error"),
            (WorkspaceTabStatus::Closed, "closed"),
        ];
        for (status, expected) in statuses {
            assert_eq!(serde_json::to_value(status).unwrap(), expected);
        }
    }

    #[test]
    fn local_terminal_capabilities_expose_only_the_terminal_surface() {
        assert_eq!(
            ConnectionCapabilities::for_session_type("local"),
            ConnectionCapabilities {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            }
        );
    }

    #[test]
    fn reconnect_mode_is_present_for_network_profiles() {
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "reconnectMode": "enter"
            })),
            Some("enter".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({ "type": "ssh" })),
            Some("none".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(
                &serde_json::json!({ "type": "ftp", "reconnectMode": "auto" })
            ),
            Some("auto".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({
                "type": "serial",
                "reconnectMode": "auto"
            })),
            Some("auto".to_string())
        );
    }

    #[test]
    fn configured_device_mode_does_not_publish_auto_before_handshake() {
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device"
            })),
            Some("network-device".to_string())
        );
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "auto"
            })),
            None
        );
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({ "type": "ftp" })),
            None
        );
    }

    #[test]
    fn initial_remote_path_respects_profile_and_protocol_defaults() {
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({
                "type": "ssh",
                "remotePath": "/srv/app"
            })),
            "/srv/app"
        );
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({ "type": "ssh" })),
            "."
        );
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({ "type": "ftp" })),
            "/"
        );
    }

    #[test]
    fn terminal_output_channel_preserves_stream_order_under_load() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_messages = Arc::clone(&received);
        let channel = Channel::new(move |body| {
            let payload: serde_json::Value = body.deserialize().unwrap();
            received_messages.lock().unwrap().push(payload);
            Ok(())
        });
        let state = WorkspaceState::default();
        state.register_terminal_output_channel(channel);

        for index in 0..2_000 {
            state.publish_terminal_output("tab-load", &format!("{index}\r\n"));
        }

        let messages = received.lock().unwrap();
        assert_eq!(messages.len(), 2_000);
        for (index, payload) in messages.iter().enumerate() {
            assert_eq!(payload["tabId"], "tab-load");
            assert_eq!(payload["chunk"], format!("{index}\r\n"));
        }
    }

    #[tokio::test]
    async fn ai_session_revision_ignores_output_and_changes_on_target_transition() {
        let state = WorkspaceState::default();

        state.publish_terminal_output("tab-target", "prompt\r\n");
        assert_eq!(state.ai_session_revision("tab-target").await, 0);

        assert_eq!(state.touch_ai_session_revision("tab-target").await, 1);
        state.publish_terminal_output("tab-target", "command output\r\n");
        assert_eq!(state.ai_session_revision("tab-target").await, 1);

        assert_eq!(state.touch_ai_session_revision("tab-target").await, 2);
    }

    #[test]
    fn split_weights_update_only_the_targeted_nested_split() {
        let mut pane_root = PaneNode::Split {
            direction: SplitDirection::Row,
            weights: vec![0.5, 0.5],
            children: vec![
                PaneNode::Leaf {
                    tab_id: "left".to_string(),
                },
                PaneNode::Split {
                    direction: SplitDirection::Column,
                    weights: vec![0.5, 0.5],
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "top-right".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "bottom-right".to_string(),
                        },
                    ],
                },
            ],
        };

        assert!(pane_root.set_split_weights_at_path(&[1], &[0.25, 0.75]));

        let PaneNode::Split {
            weights, children, ..
        } = pane_root
        else {
            panic!("root should remain a split");
        };
        assert_eq!(weights, vec![0.5, 0.5]);
        let PaneNode::Split {
            weights: nested_weights,
            ..
        } = &children[1]
        else {
            panic!("right pane should remain a split");
        };
        assert_eq!(nested_weights, &vec![0.25, 0.75]);
    }

    #[test]
    fn pane_nodes_serialize_with_the_core_camel_case_shape() {
        let value = serde_json::to_value(PaneNode::Leaf {
            tab_id: "pane-1".to_string(),
        })
        .unwrap();

        assert_eq!(value["kind"], "leaf");
        assert_eq!(value["tabId"], "pane-1");
        assert!(value.get("tab_id").is_none());
    }

    #[tokio::test]
    async fn transfer_run_handle_exposes_cancel_and_waits_for_settlement() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let (settled_tx, settled_rx) = tokio::sync::watch::channel(false);
        let handle = TransferRunHandle {
            generation: 7,
            cancel: cancel.clone(),
            settled: settled_rx,
        };

        handle.cancel.cancel();
        assert!(cancel.is_cancelled());
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = settled_tx.send(true);
        });

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            handle.wait_until_settled(),
        )
        .await
        .expect("run settlement should wake all waiters");
    }
}
