/// The terminal owns the session lifetime. Slow auxiliary initialization must
/// neither delay IO nor outlive a closed/cancelled terminal worker.
async fn run_with_auxiliary_startup<T>(
    terminal: impl std::future::Future<Output = T>,
    auxiliary: impl std::future::Future<Output = ()>,
) -> T {
    tokio::select! {
        result = terminal => result,
        _ = async {
            auxiliary.await;
            std::future::pending::<()>().await;
        } => unreachable!("completed auxiliary initialization remains pending"),
    }
}

#[cfg(test)]
mod startup_lifecycle_tests {
    use super::run_with_auxiliary_startup;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use std::time::Duration;
    use tokio::sync::{mpsc, oneshot};

    #[tokio::test]
    async fn stalled_auxiliary_channels_do_not_delay_terminal_io_and_drop_on_exit() {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = Guard(dropped.clone());
        let (input_tx, mut input_rx) = mpsc::channel::<&str>(4);
        let (output_tx, mut output_rx) = mpsc::channel(4);
        let (started_tx, started_rx) = oneshot::channel();
        let worker = tokio::spawn(run_with_auxiliary_startup(
            async move {
                while let Some(input) = input_rx.recv().await {
                    if input == "exit" {
                        return;
                    }
                    output_tx.send(input).await.unwrap();
                }
            },
            async move {
                let _guard = guard;
                let _ = started_tx.send(());
                std::future::pending::<()>().await;
            },
        ));
        tokio::time::timeout(Duration::from_secs(2), started_rx)
            .await
            .unwrap()
            .unwrap();
        for input in ["echo ready", "\u{3}"] {
            input_tx.send(input).await.unwrap();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), output_rx.recv())
                    .await
                    .unwrap(),
                Some(input)
            );
            assert!(!dropped.load(Ordering::SeqCst));
        }
        input_tx.send("exit").await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap();
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn auxiliary_completion_does_not_close_the_terminal() {
        let (ready_tx, ready_rx) = oneshot::channel();
        assert_eq!(
            run_with_auxiliary_startup(
                async {
                    ready_rx.await.unwrap();
                    "terminal remains usable"
                },
                async {
                    ready_tx.send(()).unwrap();
                },
            )
            .await,
            "terminal remains usable"
        );
    }
}
