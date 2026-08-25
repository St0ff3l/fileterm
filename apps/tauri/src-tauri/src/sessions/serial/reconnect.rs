use std::time::Duration;

use serde_json::Value;

const DEFAULT_INITIAL_DELAY: Duration = Duration::from_secs(2);
const MAX_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReconnectPolicy {
    /// `None` means unlimited retries. A profile value of zero has the same
    /// meaning so old profiles remain compatible with the new field.
    pub(super) max_attempts: Option<u32>,
    pub(super) initial_delay: Duration,
    pub(super) max_delay: Duration,
}

impl ReconnectPolicy {
    pub(super) fn from_profile(profile: &Value) -> Self {
        let max_attempts = profile
            .get("reconnectMaxAttempts")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0);
        Self {
            max_attempts,
            initial_delay: DEFAULT_INITIAL_DELAY,
            max_delay: MAX_DELAY,
        }
    }

    pub(super) fn next_attempt(self, attempt: u32) -> Option<u32> {
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

    pub(super) fn delay_for_attempt(self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(4);
        let multiplier = 1_u32 << exponent;
        self.initial_delay
            .checked_mul(multiplier)
            .unwrap_or(self.max_delay)
            .min(self.max_delay)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::ReconnectPolicy;

    #[test]
    fn defaults_to_unlimited_exponential_backoff() {
        let policy = ReconnectPolicy::from_profile(&json!({}));
        assert_eq!(policy.next_attempt(0), Some(1));
        assert_eq!(policy.delay_for_attempt(1).as_secs(), 2);
        assert_eq!(policy.delay_for_attempt(2).as_secs(), 4);
        assert_eq!(policy.delay_for_attempt(5).as_secs(), 30);
        assert_eq!(policy.delay_for_attempt(20).as_secs(), 30);
    }

    #[test]
    fn zero_means_unlimited_and_positive_values_limit_attempts() {
        assert_eq!(
            ReconnectPolicy::from_profile(&json!({ "reconnectMaxAttempts": 0 })).next_attempt(10),
            Some(11)
        );
        let policy = ReconnectPolicy::from_profile(&json!({ "reconnectMaxAttempts": 3 }));
        assert_eq!(policy.next_attempt(2), Some(3));
        assert_eq!(policy.next_attempt(3), None);
    }
}
