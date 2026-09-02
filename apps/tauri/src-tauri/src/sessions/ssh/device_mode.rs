fn is_network_device_profile(profile: &Value) -> bool {
    ConnectionCapabilities::is_network_device_profile(profile)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedSshDeviceMode {
    Server,
    NetworkDevice,
}

impl ResolvedSshDeviceMode {
    fn as_profile_value(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::NetworkDevice => "network-device",
        }
    }

    fn as_log_value(self) -> &'static str {
        self.as_profile_value()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SshDeviceModeResolution {
    mode: ResolvedSshDeviceMode,
    source: &'static str,
    family: Option<&'static str>,
}

/// Keep the SSH identification safe for diagnostics and matching. russh has
/// already validated the protocol line, but retaining only printable ASCII
/// and a bounded length keeps a malformed peer from injecting log controls.
fn normalize_ssh_identification(remote_sshid: &[u8]) -> String {
    let mut normalized = String::with_capacity(remote_sshid.len().min(255));
    for character in String::from_utf8_lossy(remote_sshid).chars() {
        if !(character == ' ' || character.is_ascii_graphic()) {
            continue;
        }
        if normalized.len() + character.len_utf8() > 255 {
            break;
        }
        normalized.push(character);
    }
    normalized
}

fn starts_with_ascii_word_boundary(value: &str, prefix: &str) -> bool {
    let Some(rest) = value.strip_prefix(prefix) else {
        return false;
    };

    rest.chars()
        .next()
        .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_')
}

/// Match the conservative vendor-level SSH software identifiers used by
/// Netcatty's real banner classifier. The SSH protocol prefix is stripped
/// before matching because both Netcatty and russh expose the software part
/// separately in different code paths. A match only selects the raw terminal
/// path; it never emits a vendor command.
fn detect_network_device_family(remote_sshid: &str) -> Option<&'static str> {
    let identification = remote_sshid.trim().to_ascii_lowercase();
    let software = identification
        .strip_prefix("ssh-2.0-")
        .or_else(|| identification.strip_prefix("ssh-1.99-"))
        .unwrap_or(identification.as_str());

    if software.is_empty() {
        return None;
    }

    // Keep these prefixes aligned with Netcatty's detectVendorFromSshVersion
    // rules. In particular, do not classify arbitrary OpenSSH strings that
    // merely contain "ios", "cisco" or "comware".
    if software.starts_with("cisco-")
        || software.starts_with("cisco_")
        || software.starts_with("ciscoios_")
        || starts_with_ascii_word_boundary(software, "cisco_wlc")
    {
        return Some("cisco");
    }
    if starts_with_ascii_word_boundary(software, "netscreen") {
        return Some("juniper");
    }

    // Older Huawei VRP firmware can advertise a dash-only software name,
    // resulting in the full identification `SSH-2.0--` or `SSH-1.99--`.
    if software == "-"
        || software.starts_with("huawei-")
        || software.starts_with("huawei_")
        || software.starts_with("vrp-")
    {
        return Some("huawei");
    }
    if software.starts_with("h3c-")
        || software.starts_with("h3c_")
        || software.starts_with("h3c ")
        || software.starts_with("comware-")
        || software.starts_with("3com") && software[4..].trim_start().starts_with("os")
    {
        return Some("h3c-comware");
    }
    if software.starts_with("mpssh_") {
        // mpSSH is HPE iLO, not H3C/Comware.
        return Some("hpe");
    }
    if starts_with_ascii_word_boundary(software, "rosssh") {
        return Some("mikrotik");
    }
    if software.starts_with("fortissh_") {
        return Some("fortinet");
    }
    if software.starts_with("paloaltonetworks_") || software.starts_with("paloaltonetworks-") {
        return Some("paloalto");
    }
    if software.starts_with("zyxel") && software[5..].trim_start().starts_with("ssh") {
        return Some("zyxel");
    }
    if starts_with_ascii_word_boundary(software, "rgos_ssh") {
        return Some("ruijie");
    }

    None
}

/// Turn an explicit vendor selection into a conservative mode hint. The
/// default `auto` value intentionally returns `None`: an unrecognised banner
/// must continue to use the legacy server fallback. A non-default selection
/// is an explicit user choice, so it may classify a device whose SSH
/// identification is generic (for example, an embedded Dropbear build)
/// without sending any vendor-specific command.
fn configured_network_device_vendor(profile: &Value) -> Option<&'static str> {
    match profile.get("networkDeviceVendor").and_then(Value::as_str) {
        Some("generic") => Some("generic"),
        Some("cisco") => Some("cisco"),
        Some("huawei") => Some("huawei"),
        Some("h3c-comware") => Some("h3c-comware"),
        Some("custom") => Some("custom"),
        _ => None,
    }
}

fn resolve_ssh_device_mode(profile: &Value, remote_sshid: &[u8]) -> SshDeviceModeResolution {
    match profile.get("deviceMode").and_then(Value::as_str) {
        Some("network-device") => SshDeviceModeResolution {
            mode: ResolvedSshDeviceMode::NetworkDevice,
            source: "manual",
            family: configured_network_device_vendor(profile),
        },
        Some("auto") => {
            let identification = normalize_ssh_identification(remote_sshid);
            match detect_network_device_family(&identification) {
                Some(family) => SshDeviceModeResolution {
                    mode: ResolvedSshDeviceMode::NetworkDevice,
                    source: "banner",
                    family: Some(family),
                },
                None => match configured_network_device_vendor(profile) {
                    Some(family) => SshDeviceModeResolution {
                        mode: ResolvedSshDeviceMode::NetworkDevice,
                        source: "vendor-hint",
                        family: Some(family),
                    },
                    None => SshDeviceModeResolution {
                        mode: ResolvedSshDeviceMode::Server,
                        source: "auto-fallback",
                        family: None,
                    },
                },
            }
        }
        Some("server") => SshDeviceModeResolution {
            mode: ResolvedSshDeviceMode::Server,
            source: "manual",
            family: None,
        },
        None | Some(_) => SshDeviceModeResolution {
            mode: ResolvedSshDeviceMode::Server,
            source: "legacy-default",
            family: None,
        },
    }
}

fn profile_with_resolved_device_mode(
    profile: &Value,
    resolution: SshDeviceModeResolution,
) -> Value {
    let mut effective_profile = profile.clone();
    if let Some(object) = effective_profile.as_object_mut() {
        object.insert(
            "deviceMode".to_string(),
            Value::String(resolution.mode.as_profile_value().to_string()),
        );
    }
    effective_profile
}

fn log_ssh_device_mode_resolution(
    app: &AppHandle,
    tab_id: &str,
    profile: &Value,
    remote_sshid: &[u8],
    resolution: SshDeviceModeResolution,
) {
    let configured_mode = match profile.get("deviceMode").and_then(Value::as_str) {
        Some("auto") => "auto",
        Some("network-device") => "network-device",
        Some("server") => "server",
        _ => "legacy-default",
    };
    let identification = normalize_ssh_identification(remote_sshid);
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "device mode resolved mode={} source={} configured={} family={} identification={}",
            resolution.mode.as_log_value(),
            resolution.source,
            configured_mode,
            resolution.family.unwrap_or("unknown"),
            if identification.is_empty() {
                "unknown"
            } else {
                identification.as_str()
            }
        ),
    );
}

async fn apply_resolved_device_mode_to_workspace(app: &AppHandle, tab_id: &str, profile: &Value) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut capabilities = ConnectionCapabilities::for_profile(profile);
    if !effective_exec_channel_enabled(profile) {
        capabilities.resource_monitoring = false;
        capabilities.shell_integration = false;
    }
    let layout = if capabilities.files {
        "terminal-file"
    } else {
        "terminal-only"
    };

    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
            tab.layout = layout.to_string();
        }
    }
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.device_mode =
                crate::services::workspace::configured_device_mode_for_profile(profile);
            session.capabilities = capabilities.clone();
            session.follow_shell_cwd = capabilities.shell_integration;
            if is_network_device_profile(profile) {
                session.shell_cwd = None;
                session.shell_user = None;
                session.remote_files_loading = false;
                session.remote_files.clear();
                session.remote_capabilities = None;
                session.system_metrics = None;
                session.sftp_unavailable_reason = None;
                session.has_reusable_sudo_auth = false;
            }
        }
    }
}

fn effective_exec_channel_enabled(profile: &Value) -> bool {
    !is_network_device_profile(profile) && exec_channel_enabled(profile)
}

fn effective_resource_monitoring_enabled(profile: &Value) -> bool {
    effective_exec_channel_enabled(profile) && resource_monitoring_enabled(profile)
}

const RESOURCE_MONITORING_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(5);

async fn disable_resource_monitoring_capability(
    app: &AppHandle,
    tab_id: &str,
    reason: impl Into<String>,
) {
    let reason = reason.into();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let changed = {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            let changed = session.capabilities.resource_monitoring;
            session.capabilities.resource_monitoring = false;
            if changed {
                session.system_metrics = None;
            }
            Some(changed)
        } else {
            None
        }
    };

    let Some(changed) = changed else {
        crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!("resource monitoring disable skipped session_not_found reason={reason}"),
        );
        return;
    };

    if !changed {
        crate::services::logging::debug(
            app,
            &format!("metrics:{tab_id}"),
            format!("resource monitoring already disabled reason={reason}"),
        );
        return;
    }

    crate::services::logging::session(
        app,
        "WARN",
        "metrics",
        tab_id,
        format!(
            "resource monitoring capability transition from=true to=false reason={reason}"
        ),
    );
    crate::services::logging::session(
        app,
        "DEBUG",
        "metrics",
        tab_id,
        "resource monitoring snapshot build started capability=false",
    );
    match timeout(
        RESOURCE_MONITORING_SNAPSHOT_TIMEOUT,
        crate::commands::get_workspace_snapshot(app.clone()),
    )
    .await
    {
        Ok(Ok(snapshot)) => {
            let workspace_revision = snapshot
                .get("workspaceRevision")
                .and_then(Value::as_u64)
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            match app.emit("workspace:snapshot", snapshot) {
                Ok(()) => crate::services::logging::session(
                    app,
                    "INFO",
                    "metrics",
                    tab_id,
                    format!(
                        "resource monitoring disabled snapshot emitted resource_monitoring=false workspace_revision={workspace_revision}"
                    ),
                ),
                Err(error) => crate::services::logging::session(
                    app,
                    "WARN",
                    "metrics",
                    tab_id,
                    format!(
                        "resource monitoring disabled snapshot emission failed resource_monitoring=false workspace_revision={workspace_revision} error={error}"
                    ),
                ),
            }
        }
        Ok(Err(error)) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!(
                "resource monitoring disabled snapshot build failed resource_monitoring=false error={error}"
            ),
        ),
        Err(_) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!(
                "resource monitoring disabled snapshot build timed out resource_monitoring=false timeout_secs={}",
                RESOURCE_MONITORING_SNAPSHOT_TIMEOUT.as_secs()
            ),
        ),
    }
}

fn effective_sftp_enabled(profile: &Value) -> bool {
    !is_network_device_profile(profile)
        && profile.get("sftpEnabled").and_then(Value::as_bool) != Some(false)
}

fn ssh_terminal_type(profile: &Value) -> &'static str {
    let default_terminal_type = if is_network_device_profile(profile) {
        "vt100"
    } else {
        "xterm-256color"
    };

    match profile.get("terminalType").and_then(Value::as_str) {
        Some("xterm-256color") => "xterm-256color",
        Some("xterm") => "xterm",
        Some("vt100") => "vt100",
        Some("vt220") => "vt220",
        Some("ansi") => "ansi",
        Some("linux") => "linux",
        _ => default_terminal_type,
    }
}

const DEFAULT_RESOURCE_MONITORING_INTERVAL_SECONDS: u64 = 1;

fn resource_monitoring_interval_seconds(profile: &Value) -> u64 {
    match profile
        .get("resourceMonitoringIntervalSeconds")
        .and_then(Value::as_u64)
    {
        Some(interval @ (1 | 5 | 15 | 30 | 60)) => interval,
        _ => DEFAULT_RESOURCE_MONITORING_INTERVAL_SECONDS,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Merge network sample history from the previous metrics into the next.
///
/// Mirrors `mergeSystemMetricsHistory` from `packages/core` so the session
/// snapshot retains the rolling `networkSamples` / `networkSamplesByInterface`
/// history. Static filesystem sections are also retained when a partial
/// refresh omits them.
fn merge_system_metrics_history(
    previous: Option<&serde_json::Value>,
    next: serde_json::Value,
    history_limit: usize,
) -> serde_json::Value {
    let mut merged = next.clone();
    if let Some(prev) = previous {
        let prev_samples = prev
            .get("networkSamples")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let next_point = next
            .get("networkSamples")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.last())
            .cloned()
            .unwrap_or(serde_json::json!({ "rx": 0, "tx": 0 }));

        let mut combined = prev_samples;
        combined.push(next_point);
        if combined.len() > history_limit {
            combined = combined[combined.len() - history_limit..].to_vec();
        }
        merged["networkSamples"] = serde_json::Value::Array(combined);

        // Per-interface accumulation
        let prev_by_iface = prev
            .get("networkSamplesByInterface")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(next_by_iface) = next
            .get("networkSamplesByInterface")
            .and_then(|v| v.as_object())
            .cloned()
        {
            let mut merged_by_iface = serde_json::Map::new();
            for (name, samples_val) in next_by_iface.iter() {
                let next_iface_point = samples_val
                    .as_array()
                    .and_then(|arr| arr.last())
                    .cloned()
                    .unwrap_or(serde_json::json!({ "rx": 0, "tx": 0 }));
                let prev_iface_samples = prev_by_iface
                    .get(name)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut combined = prev_iface_samples;
                combined.push(next_iface_point);
                if combined.len() > history_limit {
                    combined = combined[combined.len() - history_limit..].to_vec();
                }
                merged_by_iface.insert(name.clone(), serde_json::Value::Array(combined));
            }
            merged["networkSamplesByInterface"] = serde_json::Value::Object(merged_by_iface);
        }

        // A streaming metrics block can be partial. Preserve the last
        // non-empty filesystem sections instead of replacing the sidebar
        // table with its eight empty placeholder rows.
        for field in ["diskRows", "fileSystemRows"] {
            let next_has_rows = merged
                .get(field)
                .and_then(|value| value.as_array())
                .is_some_and(|rows| !rows.is_empty());
            if !next_has_rows {
                if let Some(previous_rows) = prev.get(field).and_then(|value| value.as_array()) {
                    if !previous_rows.is_empty() {
                        merged[field] = serde_json::Value::Array(previous_rows.clone());
                    }
                }
            }
        }
    }
    merged
}
