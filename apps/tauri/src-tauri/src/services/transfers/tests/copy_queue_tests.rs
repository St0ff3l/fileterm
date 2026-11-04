use super::*;
#[tokio::test]
async fn opposite_directions_queue_without_blocking_unrelated_endpoints_and_cancel_cleanly() {
    let gates = CopyIoGates::default();
    let cancel = CancellationToken::new();
    let first = gates.acquire("a", "b", &cancel).await.unwrap();
    let waiting = gates.acquire("b", "a", &cancel);
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    let unrelated = gates.acquire("c", "d", &cancel).await.unwrap();
    drop(unrelated);
    cancel.cancel();
    assert!(waiting.await.is_err());
    drop(first);
    let fresh = CancellationToken::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        gates.acquire("b", "a", &fresh),
    )
    .await
    .unwrap()
    .unwrap();
}
