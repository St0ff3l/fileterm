use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use serde::{Serialize, Deserialize};
use tauri::ipc::Channel;
use tokio::sync::{oneshot, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use crate::sessions::WorkerCmd;
use crate::services::transfers::TransferTask;

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTab {
    pub id: String,
    pub profile_id: String,
    pub session_type: String,
    pub title: String,
    pub layout: String, // "terminal-file" | "file-only" | "terminal-only"
    pub status: String, // "connecting" | "connected" | "disconnected"
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
    pub terminal_inputs:
        Arc<RwLock<HashMap<String, tokio::sync::mpsc::UnboundedSender<String>>>>,
    /// Tauri IPC channels are the ordered streaming boundary for terminal
    /// output. Ordinary app events remain appropriate for low-frequency state
    /// updates, but can fall behind sustained PTY traffic in WKWebView.
    pub terminal_output_channels:
        Arc<StdMutex<HashMap<u32, Channel<serde_json::Value>>>>,
    /// Cancels the runtime owned by each worker. Dropping the command sender
    /// alone cannot interrupt a worker that is currently parsing a large
    /// remote metrics payload or waiting on an SSH operation.
    pub worker_controls: Arc<RwLock<HashMap<String, CancellationToken>>>,
    /// Pending SSH interaction requests (host-key verification, MFA prompts).
    /// The renderer resolves each one via `app_resolve_ssh_interaction`.
    pub pending_interactions:
        Arc<RwLock<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    pub remote_forwards: Arc<RwLock<HashMap<String, Vec<RemoteForwardTarget>>>>,
    /// Transfer snapshots are durable domain state. Cancellation tokens are
    /// runtime-only and never serialized to the renderer or journal.
    pub transfers: Arc<RwLock<Vec<TransferTask>>>,
    pub transfer_controls: Arc<RwLock<HashMap<String, CancellationToken>>>,
    pub transfer_journal_loaded: Arc<Mutex<bool>>,
    pub transfer_last_event: Arc<Mutex<HashMap<String, std::time::Instant>>>,
    /// Import plans retain sanitized source data in main process until the
    /// renderer confirms a selected subset and conflict strategy.
    pub connection_import_plans: Arc<RwLock<HashMap<String, Vec<ConnectionImportPlanEntry>>>>,
    pub update_status: Arc<RwLock<Option<serde_json::Value>>>,
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
            transfer_controls: Arc::new(RwLock::new(HashMap::new())),
            transfer_journal_loaded: Arc::new(Mutex::new(false)),
            transfer_last_event: Arc::new(Mutex::new(HashMap::new())),
            connection_import_plans: Arc::new(RwLock::new(HashMap::new())),
            update_status: Arc::new(RwLock::new(None)),
        }
    }
}

impl WorkspaceState {
    pub fn register_terminal_output_channel(&self, channel: Channel<serde_json::Value>) {
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            channels.insert(channel.id(), channel);
        }
    }

    pub fn publish_terminal_output(&self, tab_id: &str, chunk: &str) {
        let payload = serde_json::json!({ "tabId": tab_id, "chunk": chunk });
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            channels.retain(|_, channel| channel.send(payload.clone()).is_ok());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionCapabilities, WorkspaceState};
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
}
