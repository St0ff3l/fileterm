use std::time::Duration;

use serde_json::json;

use super::{port_from_profile, KeepalivePolicy, ReconnectPolicy};

#[test]
fn defaults_to_bounded_exponential_backoff() {
    let policy = ReconnectPolicy::from_profile(&json!({}));
    assert_eq!(policy.next_attempt(0), Some(1));
    assert_eq!(policy.delay_for_attempt(1), Duration::from_secs(2));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_secs(4));
    assert_eq!(policy.delay_for_attempt(5), Duration::from_secs(30));
}

#[test]
fn accepts_configured_backoff_and_attempt_limit() {
    let policy = ReconnectPolicy::from_profile(&json!({
        "reconnectInitialDelayMs": 1000,
        "reconnectMaxDelayMs": 5000,
        "reconnectMaxAttempts": 3
    }));
    assert_eq!(policy.delay_for_attempt(1), Duration::from_secs(1));
    assert_eq!(policy.delay_for_attempt(4), Duration::from_secs(5));
    assert_eq!(policy.next_attempt(3), None);
}

#[test]
fn unlimited_policy_has_a_safety_circuit_for_unstable_connections() {
    let policy = ReconnectPolicy::from_profile(&json!({}));
    assert_eq!(policy.next_attempt_after_connection(4, false), Some(5));
    assert_eq!(policy.next_attempt_after_connection(5, false), None);
    assert_eq!(policy.next_attempt_after_connection(5, true), Some(6));
}

#[test]
fn keepalive_can_be_disabled_and_is_bounded() {
    let disabled = KeepalivePolicy::from_profile(&json!({
        "keepaliveEnabled": false
    }));
    assert_eq!(disabled.interval, None);

    let bounded = KeepalivePolicy::from_profile(&json!({
        "keepaliveIntervalSeconds": 1,
        "keepaliveMaxMisses": 100
    }));
    assert_eq!(bounded.interval, Some(Duration::from_secs(5)));
    assert_eq!(bounded.max_misses, 32);
}

#[test]
fn rejects_invalid_profile_ports_instead_of_wrapping() {
    assert_eq!(port_from_profile(&json!({}), 22, "SSH").unwrap(), 22);
    assert_eq!(
        port_from_profile(&json!({"port": 65535}), 22, "SSH").unwrap(),
        65535
    );
    assert!(port_from_profile(&json!({"port": 0}), 22, "SSH").is_err());
    assert!(port_from_profile(&json!({"port": 65536}), 22, "SSH").is_err());
    assert!(port_from_profile(&json!({"port": -1}), 22, "SSH").is_err());
}
