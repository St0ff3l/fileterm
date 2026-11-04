use serde_json::json;
use std::time::Duration;

use super::SerialTransferTiming;

#[test]
fn low_baud_packet_timeout_covers_a_full_one_kilobyte_frame() {
    let timing = SerialTransferTiming::from_profile(&json!({}), 300, 8, 1, false).unwrap();
    assert!(timing.packet_timeout(1024, 2) > Duration::from_secs(30));
}

#[test]
fn raw_idle_timeout_is_configurable_and_bounded() {
    let timing = SerialTransferTiming::from_profile(
        &json!({ "serialReceiveIdleTimeoutMs": 12_000 }),
        115_200,
        8,
        1,
        false,
    )
    .unwrap();
    assert_eq!(timing.raw_idle_timeout, Duration::from_secs(12));
    assert_eq!(timing.write_timeout, Duration::from_secs(30));
    assert!(SerialTransferTiming::from_profile(
        &json!({ "serialReceiveIdleTimeoutMs": 100 }),
        115_200,
        8,
        1,
        false,
    )
    .is_err());
}
