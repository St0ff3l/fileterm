use std::time::Duration;

use serde_json::Value;

const DEFAULT_RAW_IDLE_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_WRITE_TIMEOUT_MS: u64 = 30_000;
const MIN_RAW_IDLE_TIMEOUT_MS: u64 = 250;
const MAX_RAW_IDLE_TIMEOUT_MS: u64 = 600_000;
const MIN_PACKET_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PACKET_TIMEOUT: Duration = Duration::from_secs(120);

/// Timing information for a serial transfer. Packet protocols need a timeout
/// derived from the wire speed; a fixed ten-second timeout is shorter than a
/// single 1K block at common low baud rates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SerialTransferTiming {
    pub(super) baud_rate: u32,
    pub(super) bits_per_byte: u8,
    pub(super) raw_idle_timeout: Duration,
    pub(super) write_timeout: Duration,
}

impl SerialTransferTiming {
    pub(super) fn from_profile(
        profile: &Value,
        baud_rate: u32,
        data_bits: u8,
        stop_bits: u8,
        has_parity: bool,
    ) -> Result<Self, String> {
        let raw_idle_timeout_ms = profile
            .get("serialReceiveIdleTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_RAW_IDLE_TIMEOUT_MS);
        if !(MIN_RAW_IDLE_TIMEOUT_MS..=MAX_RAW_IDLE_TIMEOUT_MS).contains(&raw_idle_timeout_ms) {
            return Err(format!(
                "串口接收空闲超时必须在 {MIN_RAW_IDLE_TIMEOUT_MS} 到 {MAX_RAW_IDLE_TIMEOUT_MS} 毫秒之间"
            ));
        }
        let write_timeout_ms = profile
            .get("serialWriteTimeoutMs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_WRITE_TIMEOUT_MS);
        if !(MIN_RAW_IDLE_TIMEOUT_MS..=MAX_RAW_IDLE_TIMEOUT_MS).contains(&write_timeout_ms) {
            return Err(format!(
                "串口写入超时必须在 {MIN_RAW_IDLE_TIMEOUT_MS} 到 {MAX_RAW_IDLE_TIMEOUT_MS} 毫秒之间"
            ));
        }

        // Start bit + data bits + optional parity + stop bits. This is a
        // conservative estimate used only for timeout calculation.
        let bits_per_byte = 1_u8
            .saturating_add(data_bits)
            .saturating_add(u8::from(has_parity))
            .saturating_add(stop_bits);
        Ok(Self {
            baud_rate,
            bits_per_byte,
            raw_idle_timeout: Duration::from_millis(raw_idle_timeout_ms),
            write_timeout: Duration::from_millis(write_timeout_ms),
        })
    }

    pub(super) fn control_timeout(self) -> Duration {
        MIN_PACKET_TIMEOUT
    }

    pub(super) fn packet_timeout(self, block_size: usize, trailer_bytes: usize) -> Duration {
        let frame_bytes = 3_u64
            .saturating_add(block_size as u64)
            .saturating_add(trailer_bytes as u64);
        let wire_seconds =
            (frame_bytes * u64::from(self.bits_per_byte)) as f64 / f64::from(self.baud_rate.max(1));
        let seconds = (wire_seconds * 3.0 + 2.0).clamp(
            MIN_PACKET_TIMEOUT.as_secs_f64(),
            MAX_PACKET_TIMEOUT.as_secs_f64(),
        );
        Duration::from_secs_f64(seconds)
    }
}

#[cfg(test)]
mod tests {
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
}
