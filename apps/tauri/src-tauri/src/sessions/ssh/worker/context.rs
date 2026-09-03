/// A connection that survives this window is considered stable. Reconnect
/// attempts are reset only after a stable connection has existed; a server or
/// middlebox that accepts SSH and immediately drops it therefore cannot reset
/// the backoff on every cycle.
const SSH_CONNECTION_STABILITY_WINDOW: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
enum SshWorkerExitKind {
    Cancelled,
    InputClosed,
    ExplicitDisconnect,
    ShellClosed,
    TransportClosed,
}

#[derive(Clone, Debug)]
struct SshWorkerExit {
    kind: SshWorkerExitKind,
    disconnect_reason: Option<SshDisconnectInfo>,
    connection_was_stable: bool,
}

impl SshWorkerExit {
    fn cancelled() -> Self {
        Self {
            kind: SshWorkerExitKind::Cancelled,
            disconnect_reason: None,
            connection_was_stable: false,
        }
    }

    fn input_closed() -> Self {
        Self {
            kind: SshWorkerExitKind::InputClosed,
            disconnect_reason: None,
            connection_was_stable: false,
        }
    }

    fn explicit_disconnect() -> Self {
        Self {
            kind: SshWorkerExitKind::ExplicitDisconnect,
            disconnect_reason: None,
            connection_was_stable: false,
        }
    }

    fn shell_closed(connection_was_stable: bool) -> Self {
        Self {
            kind: SshWorkerExitKind::ShellClosed,
            disconnect_reason: None,
            connection_was_stable,
        }
    }

    fn transport_closed(
        disconnect_reason: SshDisconnectInfo,
        connection_was_stable: bool,
    ) -> Self {
        Self {
            kind: SshWorkerExitKind::TransportClosed,
            disconnect_reason: Some(disconnect_reason),
            connection_was_stable,
        }
    }

    fn should_reconnect(&self) -> bool {
        self.kind == SshWorkerExitKind::TransportClosed
            && self
                .disconnect_reason
                .as_ref()
                .is_some_and(|reason| reason.kind == SshDisconnectKind::Transport)
    }

    fn description(&self) -> String {
        match (&self.kind, &self.disconnect_reason) {
            (SshWorkerExitKind::TransportClosed, Some(reason)) => format!(
                "transport closed kind={} reason={}",
                reason.kind.as_str(),
                reason.message
            ),
            (SshWorkerExitKind::TransportClosed, None) => {
                "transport closed without a disconnect callback".to_string()
            }
            (SshWorkerExitKind::ShellClosed, _) => "shell channel closed".to_string(),
            (SshWorkerExitKind::InputClosed, _) => "worker command input closed".to_string(),
            (SshWorkerExitKind::ExplicitDisconnect, _) => "explicit disconnect requested".to_string(),
            (SshWorkerExitKind::Cancelled, _) => "worker canceled".to_string(),
        }
    }
}

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
    disconnect_reason: SharedSshDisconnectReason,
    connected_at: Instant,
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
    /// Some SSH servers only allow exec commands attached to a PTY. The
    /// platform probe records that compatibility decision so the persistent
    /// metrics channel uses the same transport instead of exiting immediately.
    metrics_request_pty: bool,
    /// A menu-driven gateway such as JumpServer/KoKo has not routed this
    /// channel to an asset yet. Auxiliary channels would each receive a new
    /// asset-selection menu instead of reaching the selected target.
    interactive_gateway: bool,
    /// Best-effort route classification used only for diagnostics. It is
    /// intentionally computed from profile shape and never changes routing.
    route_hint: &'static str,
    cancellation: &'a CancellationToken,
    state: &'a crate::services::workspace::WorkspaceState,
}
