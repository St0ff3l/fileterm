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

pub struct PendingActionApproval {
    pub sender: oneshot::Sender<crate::services::action_review::ActionApprovalDecision>,
    /// Copilot-only binding used by the atomic visible-terminal handoff.
    /// External MCP approvals intentionally leave this unset.
    pub terminal_handoff: Option<crate::services::action_review::ActionApprovalTargetBinding>,
    /// Serializes the approval timeout's final claim with the terminal
    /// handoff's validation and PTY write. The handoff holds this through its
    /// bounded write so an expiry cannot drop the receiver mid-operation.
    pub handoff_gate: Arc<Mutex<()>>,
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
    pub pending_action_approvals: Arc<RwLock<HashMap<String, PendingActionApproval>>>,
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
