use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

/// Transmission pacing is deliberately kept separate from the serial worker
/// so file transfer and macro playback can reuse the same cancellation-safe
/// byte writer later.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SerialPacing {
    pub(super) char_delay: Duration,
    pub(super) line_delay: Duration,
}

impl SerialPacing {
    pub(super) fn from_profile(profile: &Value) -> Result<Self, String> {
        let char_delay_ms = bounded_delay(profile, "serialCharDelayMs")?;
        let line_delay_ms = bounded_delay(profile, "serialLineDelayMs")?;
        Ok(Self {
            char_delay: Duration::from_millis(char_delay_ms),
            line_delay: Duration::from_millis(line_delay_ms),
        })
    }
}

fn bounded_delay(profile: &Value, field: &str) -> Result<u64, String> {
    let value = profile.get(field).and_then(Value::as_u64).unwrap_or(0);
    if value > 60_000 {
        return Err(format!("串口发送延迟不能超过 60000 毫秒：{field}"));
    }
    Ok(value)
}

async fn wait_with_cancellation(delay: Duration, cancellation: &CancellationToken) -> bool {
    if delay.is_zero() {
        return !cancellation.is_cancelled();
    }
    tokio::select! {
        _ = cancellation.cancelled() => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

/// Write and flush a serial payload while respecting both worker shutdown and
/// optional per-byte/per-line pacing. `false` means cancellation won the race.
pub(super) async fn write_serial_bytes<W>(
    writer: &mut W,
    bytes: &[u8],
    cancellation: &CancellationToken,
    pacing: SerialPacing,
) -> Result<bool, std::io::Error>
where
    W: AsyncWrite + Unpin,
{
    if bytes.is_empty() {
        return Ok(true);
    }

    if pacing.char_delay.is_zero() && pacing.line_delay.is_zero() {
        return tokio::select! {
            _ = cancellation.cancelled() => Ok(false),
            result = writer.write_all(bytes) => {
                result?;
                tokio::select! {
                    _ = cancellation.cancelled() => Ok(false),
                    result = writer.flush() => result.map(|_| true),
                }
            }
        };
    }

    let mut previous_was_cr = false;
    for byte in bytes {
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(false),
            result = writer.write_all(std::slice::from_ref(byte)) => result?,
        }

        let is_line_end = matches!(byte, b'\n' | b'\r');
        let is_crlf_second_byte = *byte == b'\n' && previous_was_cr;
        previous_was_cr = *byte == b'\r';

        if !wait_with_cancellation(pacing.char_delay, cancellation).await {
            return Ok(false);
        }
        if is_line_end
            && !is_crlf_second_byte
            && !wait_with_cancellation(pacing.line_delay, cancellation).await
        {
            return Ok(false);
        }
    }

    tokio::select! {
        _ = cancellation.cancelled() => Ok(false),
        result = writer.flush() => result.map(|_| true),
    }
}

#[cfg(test)]
mod tests {
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
                line_delay: Duration::from_millis(25)
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
                },
            )
            .await
        });

        tokio::task::yield_now().await;
        cancellation.cancel();
        assert!(!write_task.await.unwrap().unwrap());
    }
}
