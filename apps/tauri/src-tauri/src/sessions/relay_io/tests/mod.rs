use super::*;
#[tokio::test]
async fn copies_exactly_the_requested_range_and_rejects_short_sources() {
    let cancel = CancellationToken::new();
    let mut output = Vec::new();
    copy_exact(
        &mut &b"abcXYZ"[..],
        &mut output,
        3,
        &cancel,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(output, b"abc");
    assert!(copy_exact(
        &mut &b"ab"[..],
        &mut Vec::new(),
        3,
        &cancel,
        Duration::from_secs(1)
    )
    .await
    .is_err());
    cancel.cancel();
    assert!(copy_exact(
        &mut &b"abc"[..],
        &mut Vec::new(),
        3,
        &cancel,
        Duration::from_secs(1)
    )
    .await
    .is_err());
}
#[tokio::test]
async fn cancellation_stops_a_writer_blocked_by_backpressure() {
    let cancel = CancellationToken::new();
    let (_receiver, mut writer) = tokio::io::duplex(1);
    let mut source = &b"large enough to block"[..];
    let copy = copy_exact(
        &mut source,
        &mut writer,
        21,
        &cancel,
        Duration::from_secs(10),
    );
    let stop = async {
        tokio::time::sleep(Duration::from_millis(5)).await;
        cancel.cancel();
    };
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(copy, stop) })
            .await
            .unwrap();
    assert_eq!(result.unwrap_err(), "transfer canceled");
}
