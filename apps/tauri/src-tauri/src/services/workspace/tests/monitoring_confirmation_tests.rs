use super::*;

#[tokio::test]
async fn confirmation_waits_for_collector_and_handles_both_directions() {
    let (retry, _requests) = tokio::sync::mpsc::channel(1);
    let (enabled, _enabled) = tokio::sync::watch::channel(true);
    let (confirmed, settled) = tokio::sync::watch::channel(Some(true));
    let control = MonitoringControl {
        generation: 1,
        retry,
        enabled,
        settled,
        cancellation: CancellationToken::new(),
    };
    let deadline = std::time::Duration::from_millis(200);
    let stopping = tokio::spawn(wait_for_monitoring_settled(
        control.clone(),
        false,
        deadline,
    ));
    tokio::task::yield_now().await;
    assert!(!stopping.is_finished());
    confirmed.send_replace(Some(false));
    stopping.await.unwrap().unwrap();
    let starting = tokio::spawn(wait_for_monitoring_settled(control.clone(), true, deadline));
    tokio::task::yield_now().await;
    assert!(!starting.is_finished());
    confirmed.send_replace(Some(true));
    starting.await.unwrap().unwrap();
    assert!(wait_for_monitoring_settled(
        control.clone(),
        false,
        std::time::Duration::from_millis(10)
    )
    .await
    .unwrap_err()
    .contains("timed out"));
    control.cancellation.cancel();
    assert!(wait_for_monitoring_settled(control, true, deadline)
        .await
        .is_err());
}
