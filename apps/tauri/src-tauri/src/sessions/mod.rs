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
    SerialControl {
        action: SerialControlAction,
        value: Option<bool>,
        duration_ms: Option<u64>,
        respond_to: tokio::sync::oneshot::Sender<Result<SerialLineStatus, String>>,
    },
    SerialTransfer {
        request: SerialTransferRequest,
        cancellation: tokio_util::sync::CancellationToken,
        respond_to: tokio::sync::oneshot::Sender<Result<SerialTransferResult, String>>,
    },
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
        use_saved_password: bool,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialControlAction {
    SetDtr,
    SetRts,
    PulseDtr,
    PulseRts,
    SendBreak,
    ClearBuffers,
    Reset,
    Status,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialLineStatus {
    pub dtr: Option<bool>,
    pub rts: Option<bool>,
    pub dtr_readback: bool,
    pub rts_readback: bool,
    pub rts_manual: bool,
    pub cts: Option<bool>,
    pub dsr: Option<bool>,
    pub ring: Option<bool>,
    pub carrier_detect: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialTransferDirection {
    Send,
    Receive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialTransferMode {
    Raw,
    Xmodem,
    Ymodem,
    Zmodem,
    Kermit,
}

#[derive(Clone, Debug)]
pub struct SerialTransferRequest {
    pub direction: SerialTransferDirection,
    pub mode: SerialTransferMode,
    /// Send: the source file. Receive: the exact target file or destination directory.
    pub local_path: String,
    /// Y/ZMODEM and Kermit sends can negotiate multiple files; other modes use the first path.
    pub local_paths: Vec<String>,
    /// XMODEM has no file-size field. Preserve final-block padding by default so a binary
    /// receive never silently drops trailing 0x1A bytes; legacy trimming remains opt-in.
    pub xmodem_preserve_padding: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialTransferResult {
    pub bytes_transferred: u64,
    pub local_path: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialTransferProgress {
    pub tab_id: String,
    pub direction: String,
    pub mode: String,
    pub local_path: String,
    pub status: String,
    pub bytes_transferred: u64,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_second: Option<u64>,
    pub block: Option<u64>,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransferFileStat {
    pub size: u64,
    pub modified_at: Option<u64>,
}
