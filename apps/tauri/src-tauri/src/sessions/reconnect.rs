//! Shared reconnect and keepalive policy parsing for network/device sessions.
//!
//! Profiles are persisted as JSON, so this module deliberately accepts missing
//! or malformed values and falls back to bounded defaults.  A bad saved value
//! must never create a busy reconnect loop or an unreasonably long timer.

use std::time::Duration;

use serde_json::Value;

const DEFAULT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(30);
const MIN_INITIAL_DELAY: Duration = Duration::from_millis(250);
const MAX_CONFIGURED_DELAY: Duration = Duration::from_secs(300);
const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const MIN_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
const MAX_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(3600);
const DEFAULT_KEEPALIVE_MAX_MISSES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReconnectPolicy {
    /// `None` means unlimited retries. A profile value of zero has the same
    /// meaning so existing profiles remain compatible.
    pub(crate) max_attempts: Option<u32>,
    pub(crate) initial_delay: Duration,
    pub(crate) max_delay: Duration,
}

impl ReconnectPolicy {
    pub(crate) fn from_profile(profile: &Value) -> Self {
        let max_attempts = profile
            .get("reconnectMaxAttempts")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0);
        let initial_delay = duration_from_millis(
            profile,
            "reconnectInitialDelayMs",
            DEFAULT_INITIAL_DELAY,
            MIN_INITIAL_DELAY,
            MAX_CONFIGURED_DELAY,
        );
        let max_delay = duration_from_millis(
            profile,
            "reconnectMaxDelayMs",
            DEFAULT_MAX_DELAY,
            initial_delay,
            MAX_CONFIGURED_DELAY,
        );
        Self {
            max_attempts,
            initial_delay,
            max_delay,
        }
    }

    pub(crate) fn next_attempt(self, attempt: u32) -> Option<u32> {
        let next = attempt.saturating_add(1);
        if self
            .max_attempts
            .is_some_and(|max_attempts| next > max_attempts)
        {
            None
        } else {
            Some(next)
        }
    }

    pub(crate) fn delay_for_attempt(self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(8);
        let multiplier = 1_u32 << exponent;
        self.initial_delay
            .checked_mul(multiplier)
            .unwrap_or(self.max_delay)
            .min(self.max_delay)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeepalivePolicy {
    pub(crate) interval: Option<Duration>,
    pub(crate) max_misses: usize,
}

impl KeepalivePolicy {
    pub(crate) fn from_profile(profile: &Value) -> Self {
        let enabled = profile
            .get("keepaliveEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let interval_seconds = profile
            .get("keepaliveIntervalSeconds")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_KEEPALIVE_INTERVAL.as_secs());
        let interval = if enabled && interval_seconds > 0 {
            Some(Duration::from_secs(interval_seconds.clamp(
                MIN_KEEPALIVE_INTERVAL.as_secs(),
                MAX_KEEPALIVE_INTERVAL.as_secs(),
            )))
        } else {
            None
        };
        let max_misses = profile
            .get("keepaliveMaxMisses")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_KEEPALIVE_MAX_MISSES)
            .min(32);
        Self {
            interval,
            max_misses,
        }
    }
}

pub(crate) fn seconds_from_profile(
    profile: &Value,
    field: &str,
    default: Duration,
    min: Duration,
    max: Duration,
) -> Duration {
    let value = profile
        .get(field)
        .and_then(Value::as_u64)
        .map(|value| value.clamp(min.as_secs(), max.as_secs()));
    value
        .map(Duration::from_secs)
        .unwrap_or(default)
        .max(min)
        .min(max)
}

pub(crate) fn port_from_profile(
    profile: &Value,
    default: u16,
    protocol: &str,
) -> Result<u16, String> {
    let Some(value) = profile.get("port") else {
        return Ok(default);
    };
    let port = value
        .as_u64()
        .filter(|port| (1..=u16::MAX as u64).contains(port))
        .ok_or_else(|| format!("{protocol} port must be between 1 and 65535"))?;
    Ok(port as u16)
}

fn duration_from_millis(
    profile: &Value,
    field: &str,
    default: Duration,
    min: Duration,
    max: Duration,
) -> Duration {
    let Some(value) = profile.get(field).and_then(Value::as_u64) else {
        return default.max(min).min(max);
    };
    Duration::from_millis(value.clamp(min.as_millis() as u64, max.as_millis() as u64))
}

#[cfg(test)]
mod tests {
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
}
