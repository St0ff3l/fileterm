use super::{write_serial_bytes, SerialPacing};
use serde_json::json;
use std::time::Duration;
use tokio::io::duplex;
use tokio_util::sync::CancellationToken;

#[test]
fn validates_profile_delays_without_accepting_unbounded_values() {
    assert_eq!(
        SerialPacing::from_profile(&json!({
            "serialCharDelayMs": 5,
            "serialLineDelayMs": 25
        }))
        .unwrap(),
        SerialPacing {
            char_delay: Duration::from_millis(5),
            line_delay: Duration::from_millis(25),
            write_timeout: Duration::from_secs(30)
        }
    );
    assert!(SerialPacing::from_profile(&json!({ "serialCharDelayMs": 60_001 })).is_err());
}

#[tokio::test]
async fn cancellation_interrupts_paced_write() {
    let (mut writer, _reader) = duplex(1);
    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let write_task = tokio::spawn(async move {
        write_serial_bytes(
            &mut writer,
            &[0x41; 32],
            &worker_cancellation,
            SerialPacing {
                char_delay: Duration::from_millis(1),
                line_delay: Duration::ZERO,
                write_timeout: Duration::from_secs(30),
            },
        )
        .await
    });

    tokio::task::yield_now().await;
    cancellation.cancel();
    assert!(!write_task.await.unwrap().unwrap());
}

#[tokio::test]
async fn blocked_write_returns_a_timeout_instead_of_waiting_forever() {
    let (mut writer, _reader) = duplex(1);
    let cancellation = CancellationToken::new();
    let error = write_serial_bytes(
        &mut writer,
        &[0x41; 32],
        &cancellation,
        SerialPacing {
            char_delay: Duration::ZERO,
            line_delay: Duration::ZERO,
            write_timeout: Duration::from_millis(10),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
}
