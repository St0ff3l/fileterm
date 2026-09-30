impl MetricsRuntime {
    /// Keep the control receiver and SSH context outside the fallible collector.
    /// A completed/panicked collector can be replaced without restarting the shell.
    async fn supervise(&mut self, retry_rx: mpsc::Receiver<()>) {
        let retry_rx = Arc::new(Mutex::new(retry_rx));
        loop {
            if self.cancellation.is_cancelled() || self.handle.is_closed() {
                break;
            }
            let mut collector = self.clone();
            let receiver = retry_rx.clone();
            let outcome = run_monitoring_child(receiver, move |receiver| async move {
                let mut receiver = receiver.lock().await;
                collector.supervise_attempt(&mut receiver).await;
            })
            .await;
            // Recover the last published revision even if the child panicked.
            let state = self
                .app
                .state::<crate::services::workspace::WorkspaceState>();
            let current = state
                .sessions
                .read()
                .await
                .get(&self.tab_id)
                .and_then(|session| {
                    session
                        .monitoring
                        .as_ref()
                        .filter(|s| s.generation == self.status.generation)
                        .map(|status| (session.connected, status.clone()))
                });
            let Some((connected, status)) = current else {
                break;
            };
            self.status = status;
            if self.cancellation.is_cancelled() || self.handle.is_closed() || !connected {
                self.status.phase = if *self.enabled.borrow() {
                    "disconnected"
                } else {
                    "stopped"
                }
                .into();
                self.status.reason = (*self.enabled.borrow()).then(|| "ssh-disconnected".into());
                self.status.next_retry_at = None;
                self.publish(None).await;
                break;
            }
            if self.status.phase == "unsupported" {
                break;
            }
            self.gap = true;
            self.health = MetricsHealth::default();
            if !*self.enabled.borrow() {
                // Stop owns the next transition, including cancellation cleanup.
                continue;
            }
            self.status.phase = "disconnected".into();
            self.status.reason = Some(
                if outcome.is_err() {
                    "monitoring-task-failed"
                } else {
                    "monitoring-task-ended"
                }
                .into(),
            );
            self.status.next_retry_at = None;
            if !self.publish(None).await {
                continue;
            }
            let mut receiver = retry_rx.lock().await;
            let mut enabled = self.enabled.clone();
            let retry = tokio::select! {
                biased;
                _ = self.cancellation.cancelled() => false,
                _ = wait_for_monitoring_disabled(&mut enabled) => true,
                request = receiver.recv() => request.is_some(),
            };
            if !retry {
                break;
            }
            self.status.attempt = 0;
            self.status.next_retry_at = None;
            self.status.reason = None;
            self.status.phase = "starting".into();
            // Publishing the busy state serializes duplicate retry requests.
            if *self.enabled.borrow() {
                self.publish(None).await;
            }
            while receiver.try_recv().is_ok() {}
        }
        let state = self
            .app
            .state::<crate::services::workspace::WorkspaceState>();
        let mut controls = state.monitoring_controls.write().await;
        if controls
            .get(&self.tab_id)
            .is_some_and(|c| c.generation == self.status.generation)
        {
            controls.remove(&self.tab_id);
        }
    }
}

/// Joining before replacement prevents overlapping collectors; the parent
/// retains the receiver even when the child unwinds.
async fn run_monitoring_child<F, Fut>(
    receiver: Arc<Mutex<mpsc::Receiver<()>>>,
    work: F,
) -> Result<(), tokio::task::JoinError>
where
    F: FnOnce(Arc<Mutex<mpsc::Receiver<()>>>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(work(receiver)).await
}
