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
let interactive_gateway = startup.interactive_gateway;
// ── Initialize session snapshot ────────────────────────────────────────
let state = startup.state;
{
    let mut sessions = state.sessions.write().await;
    let existing_transcript = sessions
        .get(tab_id)
        .map(|s| s.terminal_transcript.clone())
        .unwrap_or_default();
    let monitoring_stopped = sessions.get(tab_id).and_then(|s| s.monitoring.as_ref()).is_some_and(|s| s.phase == "stopped");
    let monitoring_allowed = monitoring_allowed_at_shell_ready(
        effective_resource_monitoring_enabled(profile),
        sessions.get(tab_id).map(|session| session.capabilities.resource_monitoring),
    );
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
    capabilities.resource_monitoring &= monitoring_allowed;
    if !exec_channel_enabled {
        capabilities.resource_monitoring = false;
        capabilities.shell_integration = false;
    }
    if interactive_gateway {
        // A menu-driven gateway creates a fresh, unselected route for every
        // auxiliary SSH channel. Keep the terminal capability, but do not
        // advertise file or metrics channels that would only observe the
        // gateway menu instead of the target asset.
        capabilities.files = false;
        capabilities.file_access = false;
        capabilities.resource_monitoring = false;
        capabilities.shell_integration = false;
        crate::services::logging::session(
            startup.app,
            "INFO",
            "ssh",
            tab_id,
            format!(
                "session snapshot target route pending route_hint={} resource_monitoring_unavailable_reason=interactive-gateway-target-route-required; capabilities files=false file_access=false resource_monitoring=false shell_integration=false",
                startup.route_hint,
            ),
        );
    }
    let resource_monitoring_unavailable_reason = interactive_gateway
        .then(|| "interactive-gateway-target-route-required".to_string());
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
            follow_shell_cwd: exec_channel_enabled && !interactive_gateway,
            remote_files_loading: effective_sftp_enabled(profile),
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
            // SSH routing usernames may identify a container/asset, not a Unix user.
            // Establish the baseline from the first shell identity marker.
            login_user: None,
            shell_user: None,
            connected: true,
            monitoring: monitoring_allowed.then(|| {
                crate::services::workspace::MonitoringState {
                    generation: state.next_monitoring_generation.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1,
                    revision: 0, phase: if monitoring_stopped { "stopped" } else { "starting" }.into(), attempt: 0, max_attempts: 5,
                    interval_seconds: resource_monitoring_interval_seconds(profile),
                    last_sample_at: None, next_retry_at: None, reason: None,
                }
            }),
            system_metrics: None,
            resource_monitoring_unavailable_reason,
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

/// The connecting snapshot can be disabled by a profile/defaults update while
/// authentication is pending. Do not overwrite that decision with startup's
/// stale profile. Reconnect explicitly resets capabilities for the new attempt.
fn monitoring_allowed_at_shell_ready(configured: bool, existing_capability: Option<bool>) -> bool {
    configured && existing_capability != Some(false)
}

#[cfg(test)]
mod monitoring_startup_config_tests {
    use super::monitoring_allowed_at_shell_ready;

    #[test]
    fn disabled_during_connect_stays_disabled_until_new_attempt() {
        assert!(!monitoring_allowed_at_shell_ready(true, Some(false)));
        assert!(!monitoring_allowed_at_shell_ready(false, Some(true)));
        assert!(!monitoring_allowed_at_shell_ready(false, None));
        assert!(monitoring_allowed_at_shell_ready(true, Some(true)));
        assert!(monitoring_allowed_at_shell_ready(true, None));
    }

    #[tokio::test]
    async fn profile_disable_while_connecting_survives_shell_hydration() {
        let state = crate::services::workspace::WorkspaceState::default();
        let session = serde_json::from_value(serde_json::json!({
            "profileId":"profile", "aiSessionRevision":"0", "accessHost":"host",
            "summary":"", "terminalTranscript":"", "remotePath":"/", "followShellCwd":false,
            "remoteFilesLoading":false, "remoteFiles":[], "fileAccessMode":"user",
            "hasReusableSudoAuth":false, "connected":false,
            "capabilities":crate::services::workspace::ConnectionCapabilities::for_session_type("ssh")
        })).unwrap();
        state.sessions.write().await.insert("tab".into(), session);
        state.stop_monitoring_for_profile("profile").await;
        let mut sessions = state.sessions.write().await;
        let session = sessions.get_mut("tab").unwrap();
        assert!(!monitoring_allowed_at_shell_ready(true, Some(session.capabilities.resource_monitoring)));
        // Reconnect deliberately resets capabilities before shell initialization.
        session.capabilities = crate::services::workspace::ConnectionCapabilities::for_session_type("ssh");
        assert!(monitoring_allowed_at_shell_ready(true, Some(session.capabilities.resource_monitoring)));
    }
}
