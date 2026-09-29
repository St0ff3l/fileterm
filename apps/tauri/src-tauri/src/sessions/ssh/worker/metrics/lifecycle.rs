impl MetricsRuntime {
    async fn supervise_attempt(&mut self, retry_rx: &mut mpsc::Receiver<()>) {
        loop {
            if self.cancellation.is_cancelled() || self.handle.is_closed() {
                break;
            }
            if !*self.enabled.borrow() {
                self.status.phase = "stopped".into();
                self.status.next_retry_at = None;
                self.status.reason = None;
                self.gap = true;
                self.health.interrupt();
                if !self.publish(None).await {
                    break;
                }
                tokio::select! {
                    biased;
                    _ = self.cancellation.cancelled() => break,
                    result = self.enabled.changed() => if result.is_err() { break; },
                }
                if !*self.enabled.borrow() {
                    continue;
                }
            }
            // All entry paths (including enable before the child is polled)
            // must confirm startup before opening a collection channel.
            self.status.attempt = 0;
            self.status.phase = "starting".into();
            self.status.next_retry_at = None;
            self.status.reason = None;
            if !self.publish(None).await {
                if !*self.enabled.borrow() {
                    continue;
                }
                break;
            }
            self.attempt_cancellation = self.cancellation.child_token();
            let attempt_cancellation = self.attempt_cancellation.clone();
            let enabled = self.enabled.clone();
            run_until_monitoring_disabled(
                enabled,
                attempt_cancellation,
                self.run_enabled(retry_rx),
            )
            .await;
            if *self.enabled.borrow() {
                break;
            }
        }
    }
}

/// Do not acknowledge stop until the active work has closed its channel.
async fn run_until_monitoring_disabled(
    mut enabled: tokio::sync::watch::Receiver<bool>,
    cancellation: CancellationToken,
    work: impl std::future::Future<Output = ()>,
) {
    tokio::pin!(work);
    tokio::select! {
        biased;
        _ = wait_for_monitoring_disabled(&mut enabled) => {
            cancellation.cancel();
            work.await;
        }
        _ = &mut work => {},
    }
}

async fn wait_for_monitoring_disabled(enabled: &mut tokio::sync::watch::Receiver<bool>) {
    while *enabled.borrow_and_update() {
        if enabled.changed().await.is_err() {
            break;
        }
    }
}
