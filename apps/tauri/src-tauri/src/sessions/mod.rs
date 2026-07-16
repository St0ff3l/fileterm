pub mod ssh;
pub mod system_metrics;
pub mod local_files;
pub mod serial;
pub mod telnet;
pub mod terminal;
pub mod ftp;

pub enum WorkerCmd {
    WriteTerminal(String),
    ResizeTerminal {
        cols: u32,
        rows: u32,
        width: u32,
        height: u32,
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
