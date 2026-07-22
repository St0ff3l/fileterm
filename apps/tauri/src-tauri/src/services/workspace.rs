use crate::services::transfers::TransferTask;
use crate::sessions::WorkerCmd;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
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

impl ConnectionCapabilities {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_mode: Option<String>,
}

/// Return the reconnect policy that belongs to a persisted connection
/// profile. Non-SSH sessions do not expose terminal reconnect actions.
pub fn reconnect_mode_for_profile(profile: &serde_json::Value) -> Option<String> {
    if profile.get("type").and_then(serde_json::Value::as_str) != Some("ssh") {
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

pub struct WorkspaceState {
    pub tabs: Arc<RwLock<Vec<WorkspaceTab>>>,
    pub active_tab_id: Arc<RwLock<Option<String>>>,
    pub sessions: Arc<RwLock<HashMap<String, SessionSnapshot>>>,
    pub workers: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<WorkerCmd>>>>,
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
    /// Pending SSH interaction requests (host-key verification, MFA prompts).
    /// The renderer resolves each one via `app_resolve_ssh_interaction`.
    pub pending_interactions: Arc<RwLock<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
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
            terminal_inputs: Arc::new(RwLock::new(HashMap::new())),
            terminal_output_channels: Arc::new(StdMutex::new(HashMap::new())),
            worker_controls: Arc::new(RwLock::new(HashMap::new())),
            pending_interactions: Arc::new(RwLock::new(HashMap::new())),
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
            library_mutation: Arc::new(Mutex::new(())),
            update_status: Arc::new(RwLock::new(None)),
            update_check: Arc::new(Mutex::new(())),
            update_operation: Arc::new(Mutex::new(())),
            #[cfg(target_os = "windows")]
            windows_downloaded_update: Arc::new(Mutex::new(None)),
        }
    }
}

impl WorkspaceState {
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
        initial_remote_path_for_profile, reconnect_mode_for_profile, ConnectionCapabilities,
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
    fn reconnect_mode_is_present_only_for_ssh_profiles() {
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
