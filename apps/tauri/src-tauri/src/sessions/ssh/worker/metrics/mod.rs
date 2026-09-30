include!("policy.rs");
include!("state.rs");
include!("recovery.rs");
include!("startup.rs");
include!("channel_open.rs");
include!("stream.rs");
include!("tests.rs");
include!("lifecycle.rs");
include!("lifecycle_tests.rs");
include!("channel_open_tests.rs");

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
    async fn run_enabled(&mut self, retry_rx: &mut mpsc::Receiver<()>) {
        while retry_rx.try_recv().is_ok() {}
        loop {
            if self.attempt_cancellation.is_cancelled() || self.handle.is_closed() {
                break;
            }
            let prepared = self.prepare().await;
            let reason = match prepared {
                Ok(prepared) => {
                    self.started_at = tokio::time::Instant::now();
                    self.health.last_sample = None;
                    read_metrics_channel(self, prepared, retry_rx).await
                }
                Err(()) => "startup-failed",
            };
            if self.attempt_cancellation.is_cancelled() || self.handle.is_closed() {
                break;
            }
            if self.status.phase == "unsupported" || reason == "session-replaced" {
                break;
            }
            self.gap = true;
            self.health.interrupt();
            if self.status.reason.is_none() {
                self.status.reason = Some(reason.into());
            }
            let manual = reason == "manual-retry";
            if manual && self.status.attempt >= self.status.max_attempts {
                self.status.attempt = 0;
            }
            let immediate = manual || reason == "stalled-channel";
            if let Some(delay) = MetricsPolicy::retry_delay(self.status.attempt) {
                let delay = if immediate { Duration::ZERO } else { delay };
                self.status.phase = "waiting".into();
                self.status.next_retry_at = Some(metrics_now_ms() + delay.as_millis() as u64);
                if !self.publish(None).await {
                    break;
                }
                if !wait_for_metrics_retry(&self.attempt_cancellation, retry_rx, Some(delay)).await
                {
                    break;
                }
            } else {
                self.status.phase = "failed".into();
                self.status.next_retry_at = None;
                if !self.publish(None).await {
                    break;
                }
                if !wait_for_metrics_retry(&self.attempt_cancellation, retry_rx, None).await {
                    break;
                }
                self.status.attempt = 0;
            }
            self.status.attempt += 1;
            self.status.phase = "recovering".into();
            self.status.next_retry_at = None;
            self.status.reason = None;
            if !self.publish(None).await {
                break;
            }
            // Discard clicks accepted just before the busy state was published.
            while retry_rx.try_recv().is_ok() {}
        }
    }

    async fn prepare(&mut self) -> Result<PreparedMetrics, ()> {
        prepare_metrics_channel(self).await
    }
}
