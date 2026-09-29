/// Runtime monitoring state mirrors ResourceMonitoringState in packages/core.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringState {
    pub generation: u64,
    pub revision: u64,
    pub phase: String,
    pub attempt: u8,
    pub max_attempts: u8,
    pub interval_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sample_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone)]
pub struct MonitoringControl {
    pub generation: u64,
    pub retry: tokio::sync::mpsc::Sender<()>,
    pub enabled: tokio::sync::watch::Sender<bool>,
    /// Confirmed by the collector after starting or closing its channel.
    pub settled: tokio::sync::watch::Receiver<Option<bool>>,
    pub cancellation: CancellationToken,
}

impl WorkspaceState {
    /// Serialize a manual request with publication of recovery state. A busy
    /// attempt rejects duplicate clicks even before the renderer updates.
    pub async fn retry_monitoring(&self, tab_id: &str, generation: u64) -> Result<(), String> {
        let controls = self.monitoring_controls.read().await;
        let control = controls.get(tab_id).ok_or("Monitoring is unavailable")?;
        if control.generation != generation
            || control.cancellation.is_cancelled()
            || !*control.enabled.borrow()
        {
            return Err("Monitoring session has changed".into());
        }
        let sessions = self.sessions.read().await;
        let session = sessions.get(tab_id).ok_or("Session not found")?;
        let monitoring = session
            .monitoring
            .as_ref()
            .ok_or("Monitoring is unavailable")?;
        if !session.connected
            || !session.capabilities.resource_monitoring
            || monitoring.generation != generation
        {
            return Err("Monitoring is unavailable".into());
        }
        if !matches!(
            monitoring.phase.as_str(),
            "paused" | "waiting" | "failed" | "disconnected"
        ) || monitoring.reason.as_deref() == Some("ssh-disconnected")
        {
            return Err("Monitoring is already running or recovering".into());
        }
        match control.retry.try_send(()) {
            Ok(()) | Err(tokio::sync::mpsc::error::TrySendError::Full(())) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => {
                Err("Monitoring has stopped".into())
            }
        }
    }
}

impl WorkspaceState {
    pub async fn wait_for_monitoring_enabled(
        &self,
        tab_id: &str,
        generation: u64,
        enabled: bool,
    ) -> Result<(), String> {
        let control = self
            .monitoring_controls
            .read()
            .await
            .get(tab_id)
            .filter(|control| control.generation == generation)
            .cloned()
            .ok_or("Monitoring session has changed")?;
        wait_for_monitoring_settled(control, enabled, std::time::Duration::from_secs(15)).await
    }

    /// Returns true when intent was recorded before the collector exists;
    /// otherwise the caller must wait for the running collector to confirm it.
    pub async fn set_monitoring_enabled(
        &self,
        tab_id: &str,
        generation: u64,
        enabled: bool,
    ) -> Result<bool, String> {
        let controls = self.monitoring_controls.read().await;
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(tab_id).ok_or("Session not found")?;
        if !session.connected
            || !session.capabilities.resource_monitoring
            || session.monitoring.as_ref().map(|s| s.generation) != Some(generation)
        {
            return Err("Monitoring session is unavailable or has changed".into());
        }
        if enabled
            && session
                .monitoring
                .as_ref()
                .is_none_or(|s| s.phase != "stopped")
        {
            return Err("Monitoring has not stopped".into());
        }
        let control = controls.get(tab_id).filter(|c| c.generation == generation);
        let Some(control) = control else {
            // The shell snapshot is visible before platform/SFTP setup finishes.
            // Serialize intent with collector installation using the same locks.
            let status = session
                .monitoring
                .as_mut()
                .ok_or("Monitoring is unavailable")?;
            if !matches!(status.phase.as_str(), "starting" | "stopped") {
                return Err("Monitoring controller is unavailable".into());
            }
            status.phase = if enabled { "starting" } else { "stopped" }.into();
            status.revision += 1;
            status.next_retry_at = None;
            status.reason = None;
            return Ok(true);
        };
        if control.cancellation.is_cancelled() {
            return Err("Monitoring session has ended".into());
        }
        control.enabled.send_if_modified(|current| {
            if *current == enabled {
                false
            } else {
                *current = enabled;
                true
            }
        });
        Ok(false)
    }

    pub async fn stop_monitoring_for_profile(&self, profile_id: &str) {
        let controls = self.monitoring_controls.read().await;
        let mut sessions = self.sessions.write().await;
        for (tab_id, session) in sessions
            .iter_mut()
            .filter(|(_, s)| s.profile_id == profile_id)
        {
            if let Some(control) = controls.get(tab_id) {
                control.cancellation.cancel();
            }
            session.monitoring = None;
            // Configuration changes take effect on the next SSH session.
            // Keep cached samples, but never expose them as live after re-enabling.
            session.capabilities.resource_monitoring = false;
        }
    }
}

async fn wait_for_monitoring_settled(
    mut control: MonitoringControl,
    enabled: bool,
    deadline: std::time::Duration,
) -> Result<(), String> {
    tokio::select! {
        biased;
        _ = control.cancellation.cancelled() => Err("Monitoring session has ended".into()),
        result = tokio::time::timeout(deadline, control.settled.wait_for(|value| *value == Some(enabled))) => {
            match result {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(_)) => Err("Monitoring controller has stopped".into()),
                Err(_) => Err("Monitoring state change timed out".into()),
            }
        }
    }
}

#[cfg(test)]
mod monitoring_control_tests {
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
}

#[cfg(test)]
mod monitoring_confirmation_tests {
    use super::*;

    #[tokio::test]
    async fn confirmation_waits_for_collector_and_handles_both_directions() {
        let (retry, _requests) = tokio::sync::mpsc::channel(1);
        let (enabled, _enabled) = tokio::sync::watch::channel(true);
        let (confirmed, settled) = tokio::sync::watch::channel(Some(true));
        let control = MonitoringControl {
            generation: 1,
            retry,
            enabled,
            settled,
            cancellation: CancellationToken::new(),
        };
        let deadline = std::time::Duration::from_millis(200);
        let stopping = tokio::spawn(wait_for_monitoring_settled(
            control.clone(),
            false,
            deadline,
        ));
        tokio::task::yield_now().await;
        assert!(!stopping.is_finished());
        confirmed.send_replace(Some(false));
        stopping.await.unwrap().unwrap();
        let starting = tokio::spawn(wait_for_monitoring_settled(control.clone(), true, deadline));
        tokio::task::yield_now().await;
        assert!(!starting.is_finished());
        confirmed.send_replace(Some(true));
        starting.await.unwrap().unwrap();
        assert!(wait_for_monitoring_settled(
            control.clone(),
            false,
            std::time::Duration::from_millis(10)
        )
        .await
        .unwrap_err()
        .contains("timed out"));
        control.cancellation.cancel();
        assert!(wait_for_monitoring_settled(control, true, deadline)
            .await
            .is_err());
    }
}
