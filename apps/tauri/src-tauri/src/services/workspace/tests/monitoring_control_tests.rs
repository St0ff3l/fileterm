use super::*;

#[tokio::test]
async fn early_stop_and_start_are_preserved_before_collector_installation() {
    let (state, _) = fixture().await;
    state.monitoring_controls.write().await.clear();
    state
        .sessions
        .write()
        .await
        .get_mut("tab")
        .unwrap()
        .monitoring
        .as_mut()
        .unwrap()
        .phase = "starting".into();
    assert!(state.set_monitoring_enabled("tab", 2, false).await.unwrap());
    {
        let sessions = state.sessions.read().await;
        let status = sessions["tab"].monitoring.as_ref().unwrap();
        assert_eq!(status.phase, "stopped");
        assert_eq!(status.generation, 2);
        assert_eq!(status.revision, 4);
        assert!(status.next_retry_at.is_none());
    }
    assert!(state.set_monitoring_enabled("tab", 2, true).await.unwrap());
    assert_eq!(
        state.sessions.read().await["tab"]
            .monitoring
            .as_ref()
            .unwrap()
            .phase,
        "starting"
    );
    assert!(state.set_monitoring_enabled("tab", 1, false).await.is_err());
    state
        .sessions
        .write()
        .await
        .get_mut("tab")
        .unwrap()
        .connected = false;
    assert!(state.set_monitoring_enabled("tab", 2, false).await.is_err());
}

async fn fixture() -> (WorkspaceState, tokio::sync::mpsc::Receiver<()>) {
    let state = WorkspaceState::default();
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let monitoring = MonitoringState {
        generation: 2,
        revision: 3,
        phase: "paused".into(),
        attempt: 0,
        max_attempts: 5,
        interval_seconds: 1,
        last_sample_at: Some(100),
        next_retry_at: Some(200),
        reason: None,
        shared_from_tab_id: None,
        target_key: None,
    };
    let session = serde_json::from_value(serde_json::json!({
        "profileId":"profile", "aiSessionRevision":"0", "accessHost":"host",
        "summary":"", "terminalTranscript":"", "remotePath":"/", "followShellCwd":false,
        "remoteFilesLoading":false, "remoteFiles":[], "fileAccessMode":"user",
        "hasReusableSudoAuth":false, "connected":true,
        "capabilities":ConnectionCapabilities::for_session_type("ssh"),
        "monitoring":monitoring, "systemMetrics":{"cpuPercent":12}
    }))
    .unwrap();
    state.sessions.write().await.insert("tab".into(), session);
    state.monitoring_controls.write().await.insert(
        "tab".into(),
        MonitoringControl {
            generation: 2,
            retry: tx,
            enabled: tokio::sync::watch::channel(true).0,
            settled: tokio::sync::watch::channel(Some(true)).1,
            cancellation: CancellationToken::new(),
        },
    );
    (state, rx)
}

#[tokio::test]
async fn duplicate_manual_requests_coalesce_and_do_not_clear_data() {
    let (state, mut rx) = fixture().await;
    state.retry_monitoring("tab", 2).await.unwrap();
    state.retry_monitoring("tab", 2).await.unwrap();
    assert_eq!(rx.recv().await, Some(()));
    assert!(rx.try_recv().is_err());
    assert_eq!(
        state.sessions.read().await["tab"]
            .system_metrics
            .as_ref()
            .unwrap()["cpuPercent"],
        12
    );
}

#[tokio::test]
async fn busy_disconnected_and_old_generation_requests_are_rejected() {
    let (state, mut rx) = fixture().await;
    assert!(state.retry_monitoring("tab", 1).await.is_err());
    for phase in ["starting", "recovering", "healthy", "unsupported"] {
        state
            .sessions
            .write()
            .await
            .get_mut("tab")
            .unwrap()
            .monitoring
            .as_mut()
            .unwrap()
            .phase = phase.into();
        assert!(state.retry_monitoring("tab", 2).await.is_err());
    }
    {
        let mut sessions = state.sessions.write().await;
        let session = sessions.get_mut("tab").unwrap();
        session.monitoring.as_mut().unwrap().phase = "failed".into();
        session.connected = false;
    }
    assert!(state.retry_monitoring("tab", 2).await.is_err());
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn disabling_profile_cancels_monitoring_but_keeps_terminal_and_metrics() {
    let (state, _) = fixture().await;
    let token = state.monitoring_controls.read().await["tab"]
        .cancellation
        .clone();
    state.stop_monitoring_for_profile("profile").await;
    assert!(token.is_cancelled());
    let sessions = state.sessions.read().await;
    assert!(sessions["tab"].connected);
    assert!(sessions["tab"].monitoring.is_none());
    assert!(!sessions["tab"].capabilities.resource_monitoring);
    assert!(sessions["tab"].system_metrics.is_some());
    drop(sessions);
    assert!(state.retry_monitoring("tab", 2).await.is_err());
}
#[tokio::test]
async fn disconnected_collector_can_retry_but_ssh_and_manual_stop_cannot() {
    let (state, mut rx) = fixture().await;
    {
        let mut sessions = state.sessions.write().await;
        let status = sessions
            .get_mut("tab")
            .unwrap()
            .monitoring
            .as_mut()
            .unwrap();
        status.phase = "disconnected".into();
        status.reason = Some("monitoring-task-ended".into());
    }
    state.retry_monitoring("tab", 2).await.unwrap();
    state.retry_monitoring("tab", 2).await.unwrap();
    assert_eq!(rx.recv().await, Some(()));
    assert!(rx.try_recv().is_err());
    assert!(state.sessions.read().await["tab"].connected);
    assert!(state.sessions.read().await["tab"].system_metrics.is_some());
    state.set_monitoring_enabled("tab", 2, false).await.unwrap();
    assert!(state.retry_monitoring("tab", 2).await.is_err());
    state.monitoring_controls.read().await["tab"]
        .enabled
        .send_replace(true);
    state
        .sessions
        .write()
        .await
        .get_mut("tab")
        .unwrap()
        .monitoring
        .as_mut()
        .unwrap()
        .reason = Some("ssh-disconnected".into());
    assert!(state.retry_monitoring("tab", 2).await.is_err());
    assert!(rx.try_recv().is_err());
}
