/// Rehydrate the public session snapshot after the SSH shell is ready.
///
/// Keeping snapshot construction outside the worker loop makes the startup
/// boundary explicit and keeps credential state worker-local.
async fn initialize_ssh_session_snapshot(startup: &SshWorkerStartupContext<'_>) {
let tab_id = startup.tab_id;
let profile = startup.profile;
let host = startup.host;
let port = startup.port;
let username = startup.username;
let network_device_mode = startup.network_device_mode;
let exec_channel_enabled = startup.exec_channel_enabled;
// ── Initialize session snapshot ────────────────────────────────────────
let state = startup.state;
{
    let mut sessions = state.sessions.write().await;
    let existing_transcript = sessions
        .get(tab_id)
        .map(|s| s.terminal_transcript.clone())
        .unwrap_or_default();
    let existing_reconnect_mode = sessions
        .get(tab_id)
        .and_then(|session| session.reconnect_mode.clone());
    let existing_remote_path = sessions
        .get(tab_id)
        .map(|session| session.remote_path.clone())
        .unwrap_or_else(|| {
            crate::services::workspace::initial_remote_path_for_profile(profile)
        });
    let existing_shell_cwd = if network_device_mode {
        None
    } else {
        sessions
            .get(tab_id)
            .and_then(|session| session.shell_cwd.clone())
    };
    let mut capabilities =
        crate::services::workspace::ConnectionCapabilities::for_profile(profile);
    if !exec_channel_enabled {
        capabilities.resource_monitoring = false;
        capabilities.shell_integration = false;
    }
    sessions.insert(
        tab_id.to_string(),
        crate::services::SessionSnapshot {
            profile_id: profile
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("")
                .to_string(),
            ai_session_revision: state.ai_session_revision(tab_id).await.to_string(),
            device_mode: crate::services::workspace::configured_device_mode_for_profile(
                profile,
            ),
            access_host: format!("{}:{}", host, port),
            summary: format!("{}@{}", username, host),
            terminal_transcript: existing_transcript,
            remote_path: existing_remote_path,
            shell_cwd: existing_shell_cwd,
            follow_shell_cwd: exec_channel_enabled,
            remote_files_loading: false,
            remote_files: Vec::new(),
            sftp_unavailable_reason: None,
            file_access_mode: "user".to_string(),
            sudo_user: None,
            // A saved sudo password is already a reusable credential for
            // the file toolbar. Keep only this non-secret presence bit in
            // the public snapshot; the password itself stays worker-local.
            has_reusable_sudo_auth: !network_device_mode
                && profile
                    .get("sudoPassword")
                    .and_then(Value::as_str)
                    .is_some_and(|password| !password.is_empty()),
            login_user: profile
                .get("username")
                .and_then(Value::as_str)
                .map(|s| s.to_string()),
            shell_user: None,
            connected: true,
            system_metrics: None,
            capabilities,
            remote_capabilities: None,
            reconnect_mode: existing_reconnect_mode
                .or_else(|| crate::services::workspace::reconnect_mode_for_profile(profile)),
        },
    );
}

state
    .connection_operations
    .publish_for_tab(
        tab_id,
        crate::services::connection_operations::ConnectionOperationState::Connected,
    )
    .await;

}
