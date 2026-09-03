const METRICS_MAX_BLOCK_BYTES: usize = 256 * 1024;
const METRICS_MAX_BUFFER_BYTES: usize = 1_000_000;
const METRICS_BUFFER_TARGET_BYTES: usize = 500_000;
const METRICS_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const METRICS_STDERR_EXTENDED_DATA_TYPE: u32 = 1;
/// Keep enough of the remote stderr tail to identify a missing command,
/// shell syntax error, or permission failure without allowing a noisy remote
/// process to turn diagnostics into an unbounded log stream. The logger also
/// redacts common secret-labelled values before writing the line.
const METRICS_STDERR_TAIL_BYTES: usize = 8 * 1024;
/// PTY-required servers write their refusal to the regular data stream rather
/// than SSH extended stderr. Keep a bounded stdout tail so a zero-sample exit
/// exposes the remote reason in `app.log` without allowing unbounded output.
const METRICS_STDOUT_TAIL_BYTES: usize = 8 * 1024;

fn append_metrics_stderr_tail(tail: &mut Vec<u8>, chunk: &[u8]) {
    if chunk.len() >= METRICS_STDERR_TAIL_BYTES {
        tail.clear();
        tail.extend_from_slice(&chunk[chunk.len() - METRICS_STDERR_TAIL_BYTES..]);
        return;
    }

    let required_drop = tail
        .len()
        .saturating_add(chunk.len())
        .saturating_sub(METRICS_STDERR_TAIL_BYTES);
    if required_drop > 0 {
        tail.drain(..required_drop);
    }
    tail.extend_from_slice(chunk);
}

fn metrics_stderr_preview(tail: &[u8]) -> String {
    if tail.is_empty() {
        "<empty>".to_string()
    } else {
        String::from_utf8_lossy(tail).into_owned()
    }
}

fn metrics_stdout_preview(tail: &[u8], sample_count: u64) -> String {
    if sample_count == 0 {
        metrics_stderr_preview(tail)
    } else {
        "<suppressed-after-first-sample>".to_string()
    }
}

fn metrics_identity_field(value: &serde_json::Value, key: &str) -> String {
    // The parser keeps host identity under the `identity` object. Keep a
    // top-level fallback for older/alternate payloads so diagnostics remain
    // useful if a collector returns a flattened value.
    let raw = value
        .get("identity")
        .and_then(|identity| identity.get(key))
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get(key).and_then(serde_json::Value::as_str));
    let Some(raw) = raw else {
        return "<unknown>".to_string();
    };
    let compact = raw
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect::<String>();
    if compact.is_empty() {
        "<unknown>".to_string()
    } else {
        compact
    }
}

fn metrics_identity_is_unknown(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "-" | "<unknown>" | "unknown" | "n/a" | "na" | "null"
        )
}

/// A parsed metrics block is only trusted after it identifies the machine
/// that owns the long-lived channel. This prevents a PTY gateway banner (or a
/// synthetic zero-valued response) from opening the sidebar with data that
/// actually belongs to the jump host.
fn target_identity_is_valid(value: &serde_json::Value, platform: &str) -> bool {
    let hostname = metrics_identity_field(value, "hostname");
    let os_name = metrics_identity_field(value, "osName");
    let kernel_name = metrics_identity_field(value, "kernelName");
    if metrics_identity_is_unknown(&hostname)
        || metrics_identity_is_unknown(&os_name)
        || metrics_identity_is_unknown(&kernel_name)
    {
        return false;
    }

    let os_lower = os_name.to_ascii_lowercase();
    let kernel_lower = kernel_name.to_ascii_lowercase();
    let linux_family_os = [
        "linux",
        "centos",
        "red hat",
        "rhel",
        "fedora",
        "debian",
        "ubuntu",
        "rocky",
        "alma linux",
        "amazon linux",
        "suse",
        "opensuse",
        "alpine",
        "arch linux",
        "gentoo",
        "kali",
        "openwrt",
        "buildroot",
        "busybox",
    ]
    .iter()
    .any(|marker| os_lower.contains(marker));
    match platform {
        "windows" => {
            os_lower.contains("windows")
                || os_lower.contains("microsoft")
                || kernel_lower.contains("windows")
        }
        "freebsd" => os_lower.contains("freebsd") || kernel_lower.contains("freebsd"),
        "darwin" => {
            os_lower.contains("macos")
                || os_lower.contains("mac os")
                || kernel_lower.contains("darwin")
        }
        "busybox" => linux_family_os,
        // CentOS 7 normally reports `CentOS Linux ...`. A kernel name alone is
        // deliberately not enough: a gateway can also run on Linux. The OS
        // identity itself must name a Linux-family system so a
        // `JumpServer`/`Go` banner cannot open the sidebar as a target.
        _ => linux_family_os,
    }
}

fn metrics_exit_status_label(exit_status: Option<u32>) -> String {
    exit_status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn should_log_metrics_anomaly(count: u64) -> bool {
    count <= 3 || count.is_power_of_two()
}

async fn close_metrics_channel(
    channel: &mut Channel<russh::client::Msg>,
    app: &AppHandle,
    tab_id: &str,
    reason: &str,
) {
    match timeout(METRICS_CLOSE_TIMEOUT, channel.close()).await {
        Ok(Ok(())) => crate::services::logging::session(
            app,
            "DEBUG",
            "metrics",
            tab_id,
            format!("collector channel closed reason={reason}"),
        ),
        Ok(Err(error)) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!("collector channel close failed reason={reason} error={error}"),
        ),
        Err(_) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!(
                "collector channel close timed out reason={reason} timeout_secs={}",
                METRICS_CLOSE_TIMEOUT.as_secs()
            ),
        ),
    }
}

#[cfg(test)]
mod metrics_tests {
    use super::{
        append_metrics_stderr_tail, metrics_identity_field, target_identity_is_valid,
        METRICS_STDERR_TAIL_BYTES,
    };

    #[test]
    fn stderr_tail_is_bounded_and_keeps_latest_bytes() {
        let mut tail = Vec::new();
        append_metrics_stderr_tail(&mut tail, &[b'a'; METRICS_STDERR_TAIL_BYTES]);
        append_metrics_stderr_tail(&mut tail, b"tail");

        assert_eq!(tail.len(), METRICS_STDERR_TAIL_BYTES);
        assert_eq!(&tail[tail.len() - 4..], b"tail");
        assert!(tail[..tail.len() - 4].iter().all(|byte| *byte == b'a'));
    }

    #[test]
    fn stderr_tail_discards_old_chunk_when_new_chunk_exceeds_cap() {
        let mut tail = Vec::new();
        let chunk = (0..METRICS_STDERR_TAIL_BYTES + 32)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        append_metrics_stderr_tail(&mut tail, &chunk);

        assert_eq!(tail.len(), METRICS_STDERR_TAIL_BYTES);
        assert_eq!(tail, chunk[chunk.len() - METRICS_STDERR_TAIL_BYTES..]);
    }

    #[test]
    fn identity_fields_are_read_from_nested_identity_payload() {
        let value = serde_json::json!({
            "identity": {
                "hostname": "centos-target",
                "osName": "CentOS Linux 7 (Core)",
                "kernelName": "Linux"
            }
        });

        assert_eq!(metrics_identity_field(&value, "hostname"), "centos-target");
        assert_eq!(
            metrics_identity_field(&value, "osName"),
            "CentOS Linux 7 (Core)"
        );
        assert_eq!(metrics_identity_field(&value, "kernelName"), "Linux");
    }

    #[test]
    fn identity_field_keeps_flattened_payload_compatibility() {
        let value = serde_json::json!({"hostname": "legacy-target"});

        assert_eq!(metrics_identity_field(&value, "hostname"), "legacy-target");
    }

    #[test]
    fn target_identity_requires_a_real_linux_identity_for_posix_metrics() {
        let target = serde_json::json!({
            "identity": {
                "hostname": "centos-target",
                "osName": "CentOS Linux 7 (Core)",
                "kernelName": "Linux"
            }
        });
        let gateway = serde_json::json!({
            "identity": {
                "hostname": "koko",
                "osName": "JumpServer",
                "kernelName": "Linux"
            }
        });
        let ubuntu = serde_json::json!({
            "identity": {
                "hostname": "ubuntu-target",
                "osName": "Ubuntu 24.04.2 LTS",
                "kernelName": "Linux"
            }
        });

        assert!(target_identity_is_valid(&target, "linux"));
        assert!(target_identity_is_valid(&ubuntu, "linux"));
        assert!(!target_identity_is_valid(&gateway, "linux"));
    }

    #[test]
    fn target_identity_rejects_missing_identity_fields() {
        let value = serde_json::json!({
            "identity": {
                "hostname": "centos-target",
                "osName": "-",
                "kernelName": "Linux"
            }
        });

        assert!(!target_identity_is_valid(&value, "linux"));
    }
}
