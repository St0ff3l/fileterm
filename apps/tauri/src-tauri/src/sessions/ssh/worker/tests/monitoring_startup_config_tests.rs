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
