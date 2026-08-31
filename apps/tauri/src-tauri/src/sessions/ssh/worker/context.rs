/// Shared immutable/session-owned resources passed between SSH worker phases.
///
/// Keeping these values together gives the worker event loop a stable boundary:
/// protocol handles and capability flags travel as one context, while mutable
/// prompt/access state remains owned by the event-loop phase.
struct SshSessionContext {
    app: AppHandle,
    tab_id: String,
    profile: Value,
    handle: Arc<Handle<ClientHandler>>,
    shell_writer: Arc<SshShellWriteHalf>,
    sftp: Option<SharedSftpSession>,
    transfer_sftp_slot: TransferSftpSlot,
    operation_timeout: Duration,
    network_device_mode: bool,
    exec_channel_enabled: bool,
    sftp_unavailable_reason: Option<String>,
    cancellation: CancellationToken,
    metrics_shutdown: Arc<tokio::sync::Notify>,
    shell_setup_script: Option<&'static str>,
    terminal_write_tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// Immutable resources shared by the SSH startup phases.
///
/// Keeping the startup inputs together prevents each phase from growing a
/// long positional argument list as capabilities are added.
struct SshWorkerStartupContext<'a> {
    app: &'a AppHandle,
    tab_id: &'a str,
    profile: &'a Value,
    handle: &'a Arc<Handle<ClientHandler>>,
    host: &'a str,
    port: u16,
    username: &'a str,
    platform: &'a str,
    operation_timeout: Duration,
    network_device_mode: bool,
    exec_channel_enabled: bool,
    cancellation: &'a CancellationToken,
    state: &'a crate::services::workspace::WorkspaceState,
}
