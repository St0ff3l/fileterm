pub mod ftp;
pub mod local_files;
pub mod local_terminal;
pub mod serial;
pub mod ssh;
pub mod system_metrics;
pub mod telnet;
mod telnet_direct;
pub mod terminal;

pub enum WorkerCmd {
    WriteTerminal(String),
    ResizeTerminal {
        cols: u32,
        rows: u32,
        width: u32,
        height: u32,
    },
    ExecuteRemoteCommand {
        command: String,
        cwd: Option<String>,
        timeout_ms: u64,
        stdin: Option<String>,
        request_pty: bool,
        respond_to: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    /// Execute through a temporary, isolated PTY on the existing SSH
    /// transport. Any password / MFA / confirmation prompt is resolved by a
    /// FileTerm dialog, never by writing to the visible terminal PTY.
    ExecuteInteractiveRemoteCommand {
        /// The current session revision returned by session context. The
        /// worker rechecks it before requesting local user input so answers
        /// cannot be routed to a target that changed after approval.
        expected_session_revision: String,
        command: String,
        cwd: Option<String>,
        timeout_ms: u64,
        audit_context: crate::services::interactive_exec_audit::InteractiveRemoteExecAuditContext,
        respond_to: tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>,
    },
    ListRemoteFiles {
        path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    ReadRemoteFile {
        path: String,
        encoding: String,
        respond_to: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    WriteRemoteFile {
        path: String,
        content: String,
        encoding: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    CreateRemoteDirectory {
        parent_path: String,
        name: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    CreateRemoteFile {
        parent_path: String,
        name: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    CopyRemotePath {
        target_path: String,
        destination_path: String,
        target_type: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    MoveRemotePath {
        target_path: String,
        destination_path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    RenameRemotePath {
        target_path: String,
        new_name: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    DeleteRemotePath {
        target_path: String,
        target_type: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ChangeRemotePermissions {
        target_path: String,
        permissions: u32,
        recursive: bool,
        apply_to: String, // "all" | "files" | "directories"
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    SetRemoteFileAccessMode {
        mode: String,
        root_access_method: Option<String>,
        sudo_user: Option<String>,
        sudo_password: Option<String>,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ListSshTunnels {
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    CreateSshTunnel {
        rule: serde_json::Value,
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    StartSshTunnel {
        rule_id: String,
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    StopSshTunnel {
        rule_id: String,
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    DeleteSshTunnel {
        rule_id: String,
        respond_to: tokio::sync::oneshot::Sender<Result<Vec<serde_json::Value>, String>>,
    },
    StatRemoteFile {
        path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<Option<TransferFileStat>, String>>,
    },
    UploadLocalFile {
        local_path: String,
        remote_path: String,
        resume_offset: u64,
        transfer_id: String,
        cancel: tokio_util::sync::CancellationToken,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    DownloadRemoteFile {
        remote_path: String,
        local_path: String,
        resume_offset: u64,
        transfer_id: String,
        cancel: tokio_util::sync::CancellationToken,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ReplaceRemoteFile {
        partial_path: String,
        destination_path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    CommitRemoteStaging {
        staging_path: String,
        partial_path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    RemoveRemoteFile {
        path: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Disconnect,
}

#[derive(Clone, Debug)]
pub struct TransferFileStat {
    pub size: u64,
    pub modified_at: Option<u64>,
}
