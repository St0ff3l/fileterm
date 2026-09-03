/// Result of the short platform probe that runs before the terminal worker
/// enters its main event loop.
///
/// Interactive SSH gateways (for example JumpServer/KoKo) expose a menu on
/// every ordinary client session.  Their banner is not the target asset's OS
/// identity, so startup phases must carry this distinction instead of treating
/// a PTY handshake as proof that an asset was reached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlatformProbeResult {
    pub platform: String,
    pub request_pty: bool,
    pub interactive_gateway: bool,
}

impl PlatformProbeResult {
    fn new(platform: impl Into<String>, request_pty: bool) -> Self {
        Self {
            platform: platform.into(),
            request_pty,
            interactive_gateway: false,
        }
    }

    fn interactive_gateway(request_pty: bool) -> Self {
        Self {
            platform: "unknown".to_string(),
            request_pty,
            interactive_gateway: true,
        }
    }
}

/// Return the known interactive gateway kind when probe output is a menu
/// rather than a target shell response.
///
/// The detector deliberately requires both a menu prompt and a JumpServer
/// marker (or its stable Chinese menu phrases).  A normal shell may contain
/// the word `Opt>` in application output, but it should not disable metrics or
/// SFTP unless the surrounding output looks like an asset-selection gateway.
pub(crate) fn detect_interactive_gateway(output: &str) -> Option<&'static str> {
    let visible = strip_terminal_control_sequences(output).to_lowercase();
    let has_menu_prompt = visible.contains("opt>") || visible.contains("opt >");
    let has_jumpserver_marker =
        visible.contains("jumpserver") || visible.contains("jump server");
    let has_asset_menu = visible.contains("进行搜索")
        && (visible.contains("进行退出") || visible.contains("显示帮助"));
    // KoKo's English locale uses translated labels such as "Show assets you
    // have access to", "Refresh the latest machine and node information",
    // and "Show Help".  A custom terminal title can hide the JumpServer
    // banner, so keep a structural English-menu fallback as well.  Requiring
    // the prompt plus two menu labels avoids classifying an application's
    // ordinary `Opt>` output as a gateway.
    let has_english_asset_menu = visible.contains("show assets you have access")
        && (visible.contains("refresh the latest")
            || visible.contains("show help")
            || visible.contains("switch language"));

    if has_menu_prompt && (has_jumpserver_marker || has_asset_menu || has_english_asset_menu) {
        Some("jumpserver")
    } else {
        None
    }
}

/// Recognize the direct-login username shapes documented by JumpServer/KoKo.
///
/// KoKo accepts both `user@account@asset` and the equivalent `#` separator;
/// an optional protocol segment produces `user@ssh@account@asset`.  A
/// `JMS-...` username is the connection-token form.  This helper is only a
/// diagnostic hint: it never changes authentication or rewrites a profile.
/// Keeping it pure lets startup logs explain why a session is expected to be
/// an asset route or why it is still an interactive menu route.
pub(crate) fn jumpserver_direct_login_hint(username: &str) -> Option<&'static str> {
    let username = username.trim();
    // KoKo's parser uses a case-sensitive `JMS-` token prefix. Keep the
    // diagnostic hint aligned with that parser; treating `jms-...` as a token
    // would claim that a normal username is an asset route when KoKo will
    // actually show the interactive menu (or reject the login).
    if username.starts_with("JMS-") && username.len() > 4 {
        return Some("connection-token");
    }

    for separator in ['@', '#'] {
        let parts = username.split(separator).collect::<Vec<_>>();
        if !parts.iter().all(|part| !part.trim().is_empty()) {
            continue;
        }
        match parts.len() {
            3 => {
                return Some(match separator {
                    '@' => "direct-asset-at",
                    '#' => "direct-asset-hash",
                    _ => unreachable!(),
                });
            }
            4 => {
                // KoKo accepts four fields syntactically, but the SSH session
                // handler only dispatches the direct route when the protocol
                // field is exactly `ssh`. Do not label an RDP/database shape
                // as an SSH asset route in diagnostics.
                if parts[1] != "ssh" {
                    continue;
                }
                return Some(match separator {
                    '@' => "direct-asset-at-with-protocol",
                    '#' => "direct-asset-hash-with-protocol",
                    _ => unreachable!(),
                });
            }
            _ => {}
        }
    }
    None
}

/// Keep probe diagnostics useful without copying a JumpServer asset menu into
/// `app.log`. The full menu can contain tenant-specific asset names and is not
/// needed to identify the route; the detector already records its bounded
/// structural classification separately.
pub(crate) fn probe_output_preview(output: &str) -> String {
    if detect_interactive_gateway(output).is_some() {
        "<interactive-gateway-menu-suppressed>".to_string()
    } else {
        output.chars().take(300).collect()
    }
}

/// Convert a probe's menu response into an explicit startup result and emit a
/// bounded diagnostic.  The menu itself is intentionally not logged: it can
/// contain asset names, hostnames, or other tenant-visible information.
pub(crate) fn interactive_gateway_probe_result(
    probe_result: &Result<ExecCommandResult, String>,
    tab_id: Option<&str>,
    label: &str,
    request_pty: bool,
) -> Option<PlatformProbeResult> {
    let result = probe_result.as_ref().ok()?;
    let kind = detect_interactive_gateway(&result.output)?;
    let transport = probe_transport_label(request_pty);
    log_probe_message(
        tab_id,
        format!(
            "probe={label} flow_role=target interactive gateway detected kind={kind} transport={transport} exit_code={} timed_out={} output_truncated={} output_bytes={}",
            result
                .exit_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "none".to_string()),
            result.timed_out,
            result.output_truncated,
            result.output.len(),
        ),
    );
    Some(PlatformProbeResult::interactive_gateway(request_pty))
}

macro_rules! return_if_interactive_gateway {
    ($probe_result:expr, $tab_id:expr, $label:expr, $request_pty:expr) => {
        if let Some(gateway) = interactive_gateway_probe_result(
            $probe_result,
            $tab_id,
            $label,
            $request_pty,
        ) {
            return gateway;
        }
    };
}

fn strip_terminal_control_sequences(input: &str) -> String {
    let mut visible = String::with_capacity(input.len());
    let mut in_escape = false;
    for character in input.chars() {
        if in_escape {
            if ('@'..='~').contains(&character) {
                in_escape = false;
            }
            continue;
        }
        if character == '\u{1b}' {
            in_escape = true;
            continue;
        }
        if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
            continue;
        }
        visible.push(character);
    }
    visible
}
