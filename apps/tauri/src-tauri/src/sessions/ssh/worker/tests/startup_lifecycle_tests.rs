use super::{run_with_auxiliary_startup, ssh_connect_timeout, wait_for_shell_startup_step};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

#[test]
fn shell_startup_uses_the_bounded_connection_budget() {
    for (profile, seconds) in [
        (json!({}), 30),
        (json!({"connectTimeoutSeconds": 60}), 60),
        (json!({"connectTimeoutSeconds": 1}), 5),
        (json!({"connectTimeoutSeconds": 900}), 300),
        (json!({"connectTimeoutSeconds": "invalid"}), 30),
    ] {
        assert_eq!(ssh_connect_timeout(&profile), Duration::from_secs(seconds));
    }
}

#[tokio::test]
async fn shell_startup_accepts_a_response_after_the_old_eight_second_limit() {
    let cancellation = CancellationToken::new();
    let result = wait_for_shell_startup_step(
        "channel_open_session",
        ssh_connect_timeout(&json!({})),
        &cancellation,
        async {
            tokio::time::sleep(Duration::from_secs(9)).await;
            Ok::<_, String>(42)
        },
    )
    .await;
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn cancelling_stalled_shell_startup_drops_the_operation() {
    let cancellation = CancellationToken::new();
    let (started_tx, started_rx) = oneshot::channel();
    let (held_tx, held_rx) = oneshot::channel::<()>();
    let worker_token = cancellation.clone();
    let worker = tokio::spawn(async move {
        wait_for_shell_startup_step(
            "channel_open_session",
            Duration::from_secs(60),
            &worker_token,
            async move {
                let _held = held_tx;
                started_tx.send(()).unwrap();
                std::future::pending::<Result<(), String>>().await
            },
        )
        .await
    });
    started_rx.await.unwrap();
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), worker)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.unwrap_err(), "SSH connection canceled");
    assert!(held_rx.await.is_err());
}

#[tokio::test]
async fn shell_startup_preserves_success_errors_and_deadlines() {
    let cancellation = CancellationToken::new();
    for stage in ["channel_open_session", "request_pty", "request_shell"] {
        assert_eq!(
            wait_for_shell_startup_step(stage, Duration::from_secs(1), &cancellation, async {
                Ok::<_, String>(42)
            })
            .await
            .unwrap(),
            42
        );
        assert_eq!(
            wait_for_shell_startup_step(stage, Duration::from_secs(1), &cancellation, async {
                Err::<(), _>("rejected")
            })
            .await
            .unwrap_err(),
            format!("Shell startup {stage} failed: rejected")
        );
        assert_eq!(
            wait_for_shell_startup_step(
                stage,
                Duration::ZERO,
                &cancellation,
                std::future::pending::<Result<(), String>>()
            )
            .await
            .unwrap_err(),
            format!("Shell startup {stage} timed out")
        );
    }
    cancellation.cancel();
    assert_eq!(
        wait_for_shell_startup_step(
            "request_shell",
            Duration::from_secs(1),
            &cancellation,
            async {
                assert!(
                    !cancellation.is_cancelled(),
                    "cancelled startup must not poll another request"
                );
                Ok::<(), String>(())
            }
        )
        .await
        .unwrap_err(),
        "SSH connection canceled"
    );
}

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
