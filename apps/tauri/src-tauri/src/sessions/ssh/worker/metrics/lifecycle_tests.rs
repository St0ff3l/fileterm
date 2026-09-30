#[cfg(test)]
mod monitoring_start_confirmation_tests {
    use std::sync::Arc;
    use tokio::sync::{mpsc, watch, Mutex};
    use tokio_util::sync::CancellationToken;

    struct TestHandle;
    impl TestHandle {
        fn is_closed(&self) -> bool {
            false
        }
    }
    struct TestHealth;
    impl TestHealth {
        fn interrupt(&mut self) {}
    }
    struct TestStatus {
        phase: String,
        attempt: u8,
        next_retry_at: Option<u64>,
        reason: Option<String>,
    }
    struct MetricsRuntime {
        enabled: watch::Receiver<bool>,
        cancellation: CancellationToken,
        attempt_cancellation: CancellationToken,
        handle: TestHandle,
        health: TestHealth,
        status: TestStatus,
        gap: bool,
        settled: watch::Sender<Option<bool>>,
        publications: Arc<Mutex<Vec<String>>>,
    }

    // Exercise the production lifecycle, replacing only SSH work/publication.
    include!("lifecycle.rs");

    impl MetricsRuntime {
        async fn publish(&mut self, _: Option<()>) -> bool {
            self.publications
                .lock()
                .await
                .push(self.status.phase.clone());
            match self.status.phase.as_str() {
                "stopped" => {
                    self.settled.send_replace(Some(false));
                }
                "starting" => {
                    self.settled.send_replace(Some(true));
                }
                _ => {}
            }
            true
        }
        async fn run_enabled(&mut self, _: &mut mpsc::Receiver<()>) {
            assert_eq!(
                *self.settled.borrow(),
                Some(true),
                "must confirm before collecting"
            );
            self.publications.lock().await.push("collecting".into());
        }
    }

    #[tokio::test]
    async fn enable_before_first_poll_and_after_stopped_both_confirm_start() {
        for enable_before_poll in [true, false] {
            let (enabled, receiver) = watch::channel(false);
            let (settled, mut confirmation) = watch::channel(None);
            let publications = Arc::new(Mutex::new(Vec::new()));
            let mut runtime = MetricsRuntime {
                enabled: receiver,
                cancellation: CancellationToken::new(),
                attempt_cancellation: CancellationToken::new(),
                handle: TestHandle,
                health: TestHealth,
                status: TestStatus {
                    phase: "stopped".into(),
                    attempt: 4,
                    next_retry_at: Some(1),
                    reason: Some("old".into()),
                },
                gap: false,
                settled,
                publications: publications.clone(),
            };
            if enable_before_poll {
                enabled.send_replace(true);
            }
            let (_retry, mut requests) = mpsc::channel(1);
            let worker = tokio::spawn(async move {
                runtime.supervise_attempt(&mut requests).await;
            });
            if !enable_before_poll {
                tokio::time::timeout(
                    std::time::Duration::from_secs(1),
                    confirmation.wait_for(|value| *value == Some(false)),
                )
                .await
                .unwrap()
                .unwrap();
                enabled.send_replace(true);
            }
            tokio::time::timeout(std::time::Duration::from_secs(1), worker)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(*confirmation.borrow(), Some(true));
            let states = publications.lock().await;
            assert_eq!(&states[states.len() - 2..], ["starting", "collecting"]);
            if enable_before_poll {
                assert!(!states.iter().any(|phase| phase == "stopped"));
            }
        }
    }
}
