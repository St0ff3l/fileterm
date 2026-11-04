#[cfg(test)]
mod monitoring_failed_lease_tests {
    use super::*;
    use crate::services::workspace::{MonitoringLease, MonitoringLeases, MonitoringState};

    struct TestHandle;
    impl TestHandle {
        fn is_closed(&self) -> bool {
            false
        }
    }

    struct MetricsRuntime {
        handle: TestHandle,
        attempt_cancellation: CancellationToken,
        status: MonitoringState,
        started_at: tokio::time::Instant,
        health: MetricsHealth,
        gap: bool,
        leases: Arc<MonitoringLeases>,
    }

    // Exercise the production retry loop; replace only SSH and publication boundaries.
    include!("../attempt.rs");

    impl MetricsRuntime {
        async fn acquire_monitoring_lease(&mut self) -> Option<Option<MonitoringLease>> {
            Some(Some(self.leases.try_acquire("host", "failed").unwrap()))
        }
        async fn prepare(&mut self) -> Result<(), ()> {
            Err(())
        }
        async fn publish(&mut self, _: Option<()>) -> bool {
            true
        }
    }

    async fn read_metrics_channel(
        _: &mut MetricsRuntime,
        _: (),
        _: &mut mpsc::Receiver<()>,
    ) -> &'static str {
        unreachable!("the channel fails to open")
    }

    #[tokio::test]
    async fn terminal_failure_releases_lease_before_waiting_for_manual_retry() {
        let leases = Arc::new(MonitoringLeases::default());
        let mut released = leases.subscribe();
        let cancellation = CancellationToken::new();
        let mut runtime = MetricsRuntime {
            handle: TestHandle,
            attempt_cancellation: cancellation.clone(),
            status: serde_json::from_value(serde_json::json!({
                "generation": 1, "revision": 0, "phase": "recovering", "attempt": 5,
                "maxAttempts": 5, "intervalSeconds": 1
            }))
            .unwrap(),
            started_at: tokio::time::Instant::now(),
            health: MetricsHealth::default(),
            gap: false,
            leases: leases.clone(),
        };
        let (_retry, mut requests) = mpsc::channel(1);
        let worker = tokio::spawn(async move {
            runtime.run_enabled(&mut requests).await;
        });
        timeout(Duration::from_secs(1), released.changed())
            .await
            .unwrap()
            .unwrap();
        let replacement = leases
            .try_acquire("host", "healthy")
            .expect("a peer must be able to take over");
        assert!(
            !worker.is_finished(),
            "the failed collector still waits for user intent"
        );
        cancellation.cancel();
        timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap();
        drop(replacement);
    }
}
