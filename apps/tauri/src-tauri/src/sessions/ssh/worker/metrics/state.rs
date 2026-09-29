#[derive(Clone)]
struct MetricsRuntime {
    app: AppHandle,
    tab_id: String,
    handle: Arc<Handle<ClientHandler>>,
    platform: String,
    host: String,
    port: u16,
    request_pty: bool,
    route_hint: &'static str,
    cancellation: CancellationToken,
    attempt_cancellation: CancellationToken,
    enabled: tokio::sync::watch::Receiver<bool>,
    settled: tokio::sync::watch::Sender<Option<bool>>,
    channel_open_gate: Arc<tokio::sync::Semaphore>,
    connection_cancellation: CancellationToken,
    status: crate::services::workspace::MonitoringState,
    policy: MetricsPolicy,
    health: MetricsHealth,
    started_at: tokio::time::Instant,
    gap: bool,
}

fn metrics_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl MetricsRuntime {
    async fn failure(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        crate::services::logging::session(&self.app, "WARN", "metrics", &self.tab_id, &reason);
        let classification = classify_resource_monitoring_unavailable_reason(&reason);
        self.status.reason = Some(classification.into());
        if matches!(
            classification,
            "interactive-gateway-target-route-required" | "target-identity-invalid"
        ) {
            self.status.phase = "unsupported".into();
            self.publish(None).await;
        }
    }

    fn next_deadline(&self) -> tokio::time::Instant {
        self.policy.deadline(
            self.started_at,
            self.health.last_sample,
            self.status.phase == "paused",
        )
    }

    /// Returns true when the channel must be rebuilt. Byte traffic does not
    /// affect this clock: only a validated sample can establish freshness.
    async fn mark_stalled(&mut self) -> bool {
        if self.health.last_sample.is_none() {
            return true;
        }
        if self.status.phase == "paused" {
            return true;
        }
        let last = self.health.last_sample.expect("checked above");
        self.health.interrupt();
        self.gap = true;
        self.status.phase = "paused".into();
        self.status.reason = Some("sample-timeout".into());
        self.status.next_retry_at = (self.status.attempt < self.status.max_attempts).then(|| {
            metrics_now_ms()
                + (last + self.policy.rebuild)
                    .saturating_duration_since(tokio::time::Instant::now())
                    .as_millis() as u64
        });
        !self.publish(None).await
    }

    async fn sample(&mut self, mut value: Value) -> bool {
        if self.attempt_cancellation.is_cancelled() || !*self.enabled.borrow() {
            return false;
        }
        let now = metrics_now_ms();
        // Stamp real samples in the runtime, never invent zero-rate points.
        if let Some(samples) = value
            .get_mut("networkSamples")
            .and_then(Value::as_array_mut)
        {
            if let Some(point) = samples.last_mut() {
                point["sampledAt"] = now.into();
                point["breakBefore"] = self.gap.into();
            }
        }
        if let Some(interfaces) = value
            .get_mut("networkSamplesByInterface")
            .and_then(Value::as_object_mut)
        {
            for samples in interfaces.values_mut() {
                if let Some(point) = samples.as_array_mut().and_then(|s| s.last_mut()) {
                    point["sampledAt"] = now.into();
                    point["breakBefore"] = self.gap.into();
                }
            }
        }
        if self.health.sample(tokio::time::Instant::now()) {
            self.status.attempt = 0;
        }
        self.status.phase = "healthy".into();
        self.status.last_sample_at = Some(now);
        self.status.next_retry_at = None;
        self.status.reason = None;
        self.gap = false;
        self.publish(Some(value)).await
    }

    async fn publish(&mut self, value: Option<Value>) -> bool {
        let state = self
            .app
            .state::<crate::services::workspace::WorkspaceState>();
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(&self.tab_id) else {
            return false;
        };
        if session.monitoring.as_ref().map(|s| s.generation) != Some(self.status.generation)
            || (self.cancellation.is_cancelled()
                && !matches!(self.status.phase.as_str(), "disconnected" | "stopped"))
            || (!session.connected
                && !matches!(self.status.phase.as_str(), "disconnected" | "stopped"))
        {
            return false;
        }
        if !*self.enabled.borrow()
            && !matches!(self.status.phase.as_str(), "stopped" | "disconnected")
        {
            return false;
        }
        self.status.revision += 1;
        if session
            .monitoring
            .as_ref()
            .is_none_or(|s| s.phase != self.status.phase)
        {
            crate::services::logging::session(
                &self.app,
                "INFO",
                "metrics",
                &self.tab_id,
                format!(
                    "monitoring phase={} generation={} attempt={} reason={:?}",
                    self.status.phase,
                    self.status.generation,
                    self.status.attempt,
                    self.status.reason
                ),
            );
        }
        session.monitoring = Some(self.status.clone());
        match self.status.phase.as_str() {
            "stopped" => { self.settled.send_replace(Some(false)); }
            "starting" => { self.settled.send_replace(Some(true)); }
            _ => {}
        }
        if self.status.phase == "unsupported" {
            session.capabilities.resource_monitoring = false;
            session.resource_monitoring_unavailable_reason = self.status.reason.clone();
        }
        let mut payload =
            serde_json::json!({"tabId": self.tab_id, "monitoring": self.status, "mode": "append"});
        if let Some(value) = value {
            session.system_metrics = Some(merge_system_metrics_history(
                session.system_metrics.as_ref(),
                value.clone(),
                600,
            ));
            payload["systemMetrics"] = value;
        }
        // Publish while holding the state lock so later snapshots cannot precede
        // this version's state update. Renderer also checks generation/revision.
        if let Err(error) = self.app.emit("workspace:sessionMetrics", payload) {
            crate::services::logging::session(
                &self.app,
                "WARN",
                "metrics",
                &self.tab_id,
                format!("monitoring event emission failed: {error}"),
            );
        }
        true
    }
}
