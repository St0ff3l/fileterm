include!("policy.rs");
include!("target.rs");
include!("attempt.rs");
include!("tests/attempt.rs");
include!("state.rs");
include!("recovery.rs");
include!("startup.rs");
include!("channel_open.rs");
include!("stream.rs");
include!("tests/mod.rs");
include!("lifecycle.rs");
include!("tests/lifecycle.rs");
include!("tests/channel_open.rs");

/// One supervisor owns the monitoring channel, retry queue and retry budget.
/// Terminal input never waits on this task.
async fn spawn_metrics_collector(
    startup: &SshWorkerStartupContext<'_>,
    metrics_shutdown: CancellationToken,
) {
    if startup.interactive_gateway
        || !effective_resource_monitoring_enabled(startup.profile)
        || startup.cancellation.is_cancelled()
    {
        return;
    }
    let state = startup.state;
    let cancellation = metrics_shutdown.child_token();
    let (retry, retry_rx) = mpsc::channel(1);
    let (enabled, enabled_rx) = tokio::sync::watch::channel(true);
    let (settled, settled_rx) = tokio::sync::watch::channel(None);
    let interval = resource_monitoring_interval_seconds(startup.profile);
    let status = {
        let mut controls = state.monitoring_controls.write().await;
        let mut sessions = state.sessions.write().await;
        if startup.cancellation.is_cancelled() || metrics_shutdown.is_cancelled() {
            return;
        }
        let Some(session) = sessions
            .get_mut(startup.tab_id)
            .filter(|s| s.connected && s.monitoring.is_some())
        else {
            return;
        };
        if session
            .monitoring
            .as_ref()
            .is_some_and(|s| s.phase == "stopped")
        {
            enabled.send_replace(false);
        }
        let status = session.monitoring.clone().expect("monitoring checked above");
        if let Some(old) = controls.insert(
            startup.tab_id.to_string(),
            crate::services::workspace::MonitoringControl {
                generation: status.generation,
                retry,
                enabled,
                settled: settled_rx,
                cancellation: cancellation.clone(),
            },
        ) {
            old.cancellation.cancel();
        }
        status
    };
    let mut runtime = MetricsRuntime {
        app: startup.app.clone(),
        tab_id: startup.tab_id.to_string(),
        handle: startup.handle.clone(),
        platform: startup.platform.to_string(),
        host: startup.host.to_string(),
        port: startup.port,
        request_pty: startup.metrics_request_pty,
        route_hint: startup.route_hint,
        cancellation: cancellation.clone(),
        attempt_cancellation: cancellation.child_token(),
        enabled: enabled_rx,
        settled,
        channel_open_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        connection_cancellation: startup.cancellation.clone(),
        status,
        policy: MetricsPolicy::new(interval),
        health: MetricsHealth::default(),
        started_at: tokio::time::Instant::now(),
        gap: false,
    };
    tokio::spawn(async move {
        runtime.supervise(retry_rx).await;
    });
}

impl MetricsRuntime {

    async fn prepare(&mut self) -> Result<PreparedMetrics, ()> {
        prepare_metrics_channel(self).await
    }

    /// Followers publish `shared` and wait until the machine's leader releases
    /// its lease. Returns None when this attempt ends while waiting.
    async fn acquire_monitoring_lease(
        &mut self,
    ) -> Option<Option<crate::services::workspace::MonitoringLease>> {
        let Some(key) = self.status.target_key.clone() else {
            return Some(None);
        };
        let leases = self
            .app
            .state::<crate::services::workspace::WorkspaceState>()
            .monitoring_leases
            .clone();
        let mut released = leases.subscribe();
        loop {
            match leases.try_acquire(&key, &self.tab_id) {
                Ok(lease) => {
                    if self.status.phase == "shared" {
                        self.status.phase = "starting".into();
                        if !self.publish(None).await {
                            return None;
                        }
                    }
                    return Some(Some(lease));
                }
                Err(holder) => {
                    if self.status.phase != "shared"
                        || self.status.shared_from_tab_id.as_deref() != Some(holder.as_str())
                    {
                        self.status.phase = "shared".into();
                        self.status.shared_from_tab_id = Some(holder);
                        self.status.next_retry_at = None;
                        self.status.reason = None;
                        self.gap = true;
                        self.health.interrupt();
                        if !self.publish(None).await {
                            return None;
                        }
                    }
                }
            }
            tokio::select! {
                biased;
                _ = self.attempt_cancellation.cancelled() => return None,
                result = released.changed() => if result.is_err() { return None; },
            }
        }
    }
}
