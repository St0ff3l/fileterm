#[cfg(test)]
mod monitoring_policy_tests {
    use super::*;

    #[test]
    fn five_intervals_have_distinct_pause_rebuild_and_startup_deadlines() {
        for (interval, pause, rebuild, first) in [
            (1, 15, 60, 30),
            (5, 15, 60, 30),
            (15, 45, 75, 45),
            (30, 90, 150, 90),
            (60, 180, 300, 180),
        ] {
            let policy = MetricsPolicy::new(interval);
            assert_eq!(policy.pause.as_secs(), pause);
            assert_eq!(policy.rebuild.as_secs(), rebuild);
            assert_eq!(policy.first_sample.as_secs(), first);
        }
    }

    #[test]
    fn retries_are_bounded_to_five_attempts() {
        for (attempt, delay) in [2, 5, 10, 20, 30].into_iter().enumerate() {
            assert_eq!(
                MetricsPolicy::retry_delay(attempt as u8).unwrap().as_secs(),
                delay
            );
        }
        assert!(MetricsPolicy::retry_delay(5).is_none());
        assert!(MetricsPolicy::retry_delay(6).is_none());
    }

    #[test]
    fn retry_budget_only_resets_after_both_stability_conditions() {
        let now = tokio::time::Instant::now();
        let mut health = MetricsHealth::default();
        assert!(!health.sample(now));
        assert!(!health.sample(now + Duration::from_secs(60)));
        assert!(health.sample(now + Duration::from_secs(120)));
        health.interrupt();
        assert!(!health.sample(now + Duration::from_secs(180)));
        assert!(!health.sample(now + Duration::from_secs(181)));
        assert!(!health.sample(now + Duration::from_secs(182)));
        assert!(health.sample(now + Duration::from_secs(240)));
    }
}

#[cfg(test)]
mod monitoring_lifecycle_tests {
    use super::*;

    #[test]
    fn noise_and_parser_defaults_do_not_count_as_samples() {
        for block in [
            "",
            "permission denied",
            "__CPU__NaN",
            "__CPU__infinity",
            "__MEM__1|2|garbage",
            "__CPU__101",
        ] {
            assert!(!metrics_block_has_sample(block), "{block}");
        }
        for block in ["__CPU__0\r\n", "__CPU__4.5", "__MEM__100|200|50|0|0|0\r"] {
            assert!(metrics_block_has_sample(block), "{block}");
        }
    }

    #[tokio::test]
    async fn manual_retry_skips_backoff_without_a_second_request() {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(()).await.unwrap();
        assert!(timeout(
            Duration::from_millis(200),
            wait_for_metrics_retry(&cancel, &mut rx, Some(Duration::from_secs(30)))
        )
        .await
        .unwrap());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn exhausted_retry_waits_for_manual_request() {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(1);
        assert!(timeout(
            Duration::from_millis(10),
            wait_for_metrics_retry(&cancel, &mut rx, None)
        )
        .await
        .is_err());
        tx.send(()).await.unwrap();
        assert!(wait_for_metrics_retry(&cancel, &mut rx, None).await);
    }

    #[tokio::test]
    async fn cancellation_wins_over_queued_retry_and_interrupts_setup() {
        let cancel = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(()).await.unwrap();
        cancel.cancel();
        assert!(!wait_for_metrics_retry(&cancel, &mut rx, Some(Duration::ZERO)).await);
        let result = metrics_startup_step(&cancel, std::future::pending::<()>()).await;
        assert!(result.is_err());
    }

    #[test]
    fn history_merge_preserves_recovery_breaks() {
        let previous = serde_json::json!({"networkSamples":[{"rx":1,"tx":2}]});
        let point = serde_json::json!({"rx":3,"tx":4,"breakBefore":true,"sampledAt":100});
        let next = serde_json::json!({"networkSamples":[point.clone()],"networkSamplesByInterface":{"eth0":[point]}});
        let merged = merge_system_metrics_history(Some(&previous), next, 600);
        assert_eq!(merged["networkSamples"][1]["breakBefore"], true);
        assert_eq!(
            merged["networkSamplesByInterface"]["eth0"][0]["sampledAt"],
            100
        );
    }
}

#[cfg(test)]
mod monitoring_deadline_tests {
    use super::*;

    #[test]
    fn deadlines_remain_relative_to_valid_sample_and_recovery_renews_them() {
        let started = tokio::time::Instant::now();
        for interval in [1, 5, 15, 30, 60] {
            let policy = MetricsPolicy::new(interval);
            assert_eq!(
                policy.deadline(started, None, false),
                started + policy.first_sample
            );
            let sample = started + Duration::from_secs(4);
            let paused = policy.deadline(started, Some(sample), false);
            let rebuild = policy.deadline(started, Some(sample), true);
            assert_eq!(paused, sample + policy.pause);
            assert_eq!(rebuild, sample + policy.rebuild);
            // An unrelated byte message has no place in this state or clock.
            assert_eq!(policy.deadline(started, Some(sample), true), rebuild);
            let recovered = sample + policy.pause + Duration::from_secs(1);
            assert_eq!(
                policy.deadline(started, Some(recovered), false),
                recovered + policy.pause
            );
        }
    }
}

#[cfg(test)]
mod monitoring_supervisor_tests {
    use super::*;

    #[tokio::test]
    async fn collector_panic_retains_receiver_and_next_child_consumes_manual_retry() {
        let (tx, rx) = mpsc::channel(1);
        let receiver = Arc::new(Mutex::new(rx));
        let result = run_monitoring_child(receiver.clone(), |receiver| async move {
            let _lock = receiver.lock().await;
            panic!("simulated collector exit");
        })
        .await;
        assert!(result.unwrap_err().is_panic());
        tx.try_send(()).unwrap();
        run_monitoring_child(receiver.clone(), |receiver| async move {
            assert_eq!(receiver.lock().await.recv().await, Some(()));
        })
        .await
        .unwrap();
        assert!(!tx.is_closed());
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        tx.try_send(()).unwrap();
        run_monitoring_child(receiver, move |receiver| async move {
            assert!(!wait_for_metrics_retry(&cancelled, &mut *receiver.lock().await, None).await);
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod monitoring_toggle_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn repeated_stop_cancels_active_work_and_waits_for_cleanup() {
        let (enabled, mut changes) = tokio::sync::watch::channel(true);
        for _ in 0..3 {
            let attempt = CancellationToken::new();
            let work_cancel = attempt.clone();
            let (closed, mut close_event) = tokio::sync::oneshot::channel();
            let worker = tokio::spawn(run_until_monitoring_disabled(changes.clone(), attempt.clone(), async move {
                work_cancel.cancelled().await;
                tokio::task::yield_now().await;
                closed.send(()).unwrap();
            }));
            enabled.send_replace(false);
            timeout(Duration::from_millis(200), worker).await.unwrap().unwrap();
            assert!(attempt.is_cancelled());
            assert!(close_event.try_recv().is_ok());
            changes.borrow_and_update();
            enabled.send_replace(true);
            changes.changed().await.unwrap();
            assert!(*changes.borrow());
        }
    }
}
