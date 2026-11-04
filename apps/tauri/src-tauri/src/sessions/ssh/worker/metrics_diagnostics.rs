const METRICS_MAX_BLOCK_BYTES: usize = 256 * 1024;
const METRICS_MAX_BUFFER_BYTES: usize = 1_000_000;
const METRICS_BUFFER_TARGET_BYTES: usize = 500_000;
const METRICS_CLOSE_TIMEOUT: Duration = Duration::from_secs(3);
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
        if interactive_gateway_menu_detected(tail, sample_count) {
            "<interactive-gateway-menu-suppressed>".to_string()
        } else if target_shell_prompt_detected(tail, sample_count) {
            "<target-shell-prompt>".to_string()
        } else {
            metrics_stderr_preview(tail)
        }
    } else {
        "<suppressed-after-first-sample>".to_string()
    }
}

/// A missed platform probe can still leave the auxiliary metrics channel at
/// an interactive JumpServer menu. Detect that response while reading the
/// stream so the collector fails closed immediately instead of waiting for
/// the idle watchdog to expire.
fn interactive_gateway_menu_detected(tail: &[u8], sample_count: u64) -> bool {
    if sample_count != 0 {
        return false;
    }
    let output = visible_shell_text(&String::from_utf8_lossy(tail));
    crate::sessions::system_metrics::detect_interactive_gateway(&output).is_some()
}

/// Some KoKo/asset combinations accept an exec request with a PTY but start
/// the asset's interactive shell instead of running the requested command.
/// The prompt is the safe point at which the collector can retry by writing
/// the script through that same PTY. Only classify a prompt before the first
/// marker; a normal collector can legitimately print `$` or `#` in its data.
fn target_shell_prompt_detected(tail: &[u8], sample_count: u64) -> bool {
    if sample_count != 0 {
        return false;
    }
    let output = visible_shell_text(&String::from_utf8_lossy(tail));
    output
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .rev()
        .take(3)
        .any(|line| {
            let line = line.trim();
            let prompt = line.strip_suffix(['#', '$', '%']);
            prompt.is_some_and(|prefix| {
                let prefix = prefix.trim_end();
                !prefix.is_empty() && (prefix.contains('@') || prefix.starts_with('['))
            })
        })
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
        // ID_LIKE is a list: dropping a tab would join adjacent distro IDs.
        .map(|character| {
            if key == "osIdLike" && character.is_ascii_whitespace() {
                ' '
            } else {
                character
            }
        })
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
    // A known gateway banner must not become a target just because it claims
    // a Linux kernel or includes a distro family in its payload.
    if os_lower.contains("jumpserver") || matches!(os_lower.trim(), "go" | "koko") {
        return false;
    }
    let os_id = metrics_identity_field(value, "osId").to_ascii_lowercase();
    let os_id_like = metrics_identity_field(value, "osIdLike").to_ascii_lowercase();
    let linux_family_id = std::iter::once(os_id.as_str())
        .chain(os_id_like.split_ascii_whitespace())
        .any(|id| {
            matches!(
                id,
                "linux" | "debian" | "ubuntu" | "rhel" | "fedora" | "centos"
                    | "rocky" | "almalinux" | "amzn" | "suse" | "sles" | "sled"
                    | "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "alpine"
                    | "arch" | "gentoo" | "nixos" | "void" | "slackware" | "solus"
                    | "mageia" | "mandriva" | "openmandriva" | "clear-linux-os"
                    | "openwrt" | "buildroot" | "busybox" | "armbian" | "photon"
                    | "mariner" | "azurelinux" | "openeuler"
            )
        });
    let linux_family_os = [
        "linux",
        "centos",
        "red hat",
        "rhel",
        "fedora",
        "debian",
        "ubuntu",
        "armbian",
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
        "linux" | "busybox" => {
            // Older collectors lack distro IDs. Keep the name fallback, but
            // neither a kernel alone nor an unknown platform proves a target.
            kernel_lower == "linux" && (linux_family_id || linux_family_os)
        }
        _ => false,
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

async fn send_metrics_pty_fallback(
    channel: &mut Channel<russh::client::Msg>,
    app: &AppHandle,
    tab_id: &str,
    script: &str,
) -> Result<(), String> {
    match timeout(SHELL_INIT_STEP_TIMEOUT, channel.data(script.as_bytes())).await {
        Ok(Ok(())) => {
            crate::services::logging::session(
                app,
                "INFO",
                "metrics",
                tab_id,
                format!(
                    "collector PTY stdin fallback sent flow_role=target script_bytes={} transport=pty-shell-stdin",
                    script.len(),
                ),
            );
            Ok(())
        }
        Ok(Err(error)) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "metrics",
                tab_id,
                format!(
                    "collector PTY stdin fallback failed flow_role=target error={error} transport=pty-shell-stdin"
                ),
            );
            Err(format!("write collector script through PTY failed: {error}"))
        }
        Err(_) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "metrics",
                tab_id,
                format!(
                    "collector PTY stdin fallback timed out timeout_secs={} transport=pty-shell-stdin",
                    SHELL_INIT_STEP_TIMEOUT.as_secs(),
                ),
            );
            Err("write collector script through PTY timed out".to_string())
        }
    }
}

#[cfg(test)]
#[path = "tests/metrics_tests.rs"]
mod metrics_tests;
