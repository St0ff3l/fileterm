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
    /// Set in the `shared` phase: the tab whose collector serves this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_from_tab_id: Option<String>,
    /// Captured connection target; profile edits cannot retarget a live session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_key: Option<String>,
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
#[path = "tests/monitoring_control_tests.rs"]
mod monitoring_control_tests;
#[cfg(test)]
#[path = "tests/monitoring_confirmation_tests.rs"]
mod monitoring_confirmation_tests;
