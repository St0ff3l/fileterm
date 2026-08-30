//! Shared one-time action approval and bounded remote command support.
//!
//! MCP requests originate outside the renderer, while Copilot starts from a
//! visible tool activity. Both flows still need the same fail-closed
//! approval queue. Server commands use the dedicated SSH exec boundary, while
//! network-device commands use the visible raw PTY; keeping both routes here
//! prevents either surface from silently gaining a shortcut around user
//! confirmation or the visible terminal.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};

use crate::sessions::WorkerCmd;
use crate::AppError;

pub const ACTION_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
/// Keep the foreground credential dialog alive long enough for a user to
/// switch back to FileTerm, enter the password, and submit it. This is still
/// bounded so an abandoned request cannot wait forever.
pub const PRIVILEGED_PASSWORD_PROMPT_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_REMOTE_EXEC_COMMAND_BYTES: usize = 64 * 1024;
const MAX_REMOTE_EXEC_CWD_BYTES: usize = 4 * 1024;
const MAX_REMOTE_EXEC_TAB_ID_BYTES: usize = 256;
pub const DEFAULT_REMOTE_EXEC_TIMEOUT_MS: u64 = 60_000;
pub const MIN_REMOTE_EXEC_TIMEOUT_MS: u64 = 1_000;
pub const MAX_REMOTE_EXEC_TIMEOUT_MS: u64 = 120_000;
const MAX_REMOTE_EXEC_SECRET_BYTES: usize = 4 * 1024;
const NETWORK_DEVICE_RAW_OUTPUT_BYTES: usize = 64 * 1024;
const NETWORK_DEVICE_RAW_IDLE_SETTLE: Duration = Duration::from_millis(200);
const NETWORK_DEVICE_RAW_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub const SUDO_PASSWORD_NEEDED: &str = "SUDO_PASSWORD_NEEDED";
pub const SU_PASSWORD_NEEDED: &str = "SU_PASSWORD_NEEDED";
pub const SUDO_PASSWORD_CANCELLED: &str = "SUDO_PASSWORD_CANCELLED";
pub const SU_PASSWORD_CANCELLED: &str = "SU_PASSWORD_CANCELLED";
pub const SUDO_AUTH_FAILURE: &str = "SUDO_AUTH_FAILURE";
pub const SU_AUTH_FAILURE: &str = "SU_AUTH_FAILURE";
/// Stable result code returned when a background exec command needs input
/// that cannot be collected on the independent channel. The user must finish
/// that operation in the visible SSH terminal and retry the non-interactive
/// command afterwards.
pub const REMOTE_INTERACTIVE_INPUT_REQUIRED: &str = "REMOTE_INTERACTIVE_INPUT_REQUIRED";
/// Network-device sessions do not have a POSIX working directory. Callers
/// must send the native CLI command without a `cd` context.
pub const NETWORK_DEVICE_CWD_UNSUPPORTED: &str = "NETWORK_DEVICE_CWD_UNSUPPORTED";
/// sudo/su credentials are meaningful only for a server shell and must never
/// be routed into a network-device terminal command.
pub const NETWORK_DEVICE_PRIVILEGE_UNSUPPORTED: &str = "NETWORK_DEVICE_PRIVILEGE_UNSUPPORTED";
/// Network-device sessions have no isolated SSH exec channel. Callers must
/// choose the explicit visible-terminal route for those commands.
pub const NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED: &str = "NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED";
/// Raw network-device commands must stay single-line so one tool call cannot
/// smuggle a second command through the visible terminal.
pub const NETWORK_DEVICE_COMMAND_INVALID: &str = "NETWORK_DEVICE_COMMAND_INVALID";
/// Visible Agent commands must stay single-line so one tool call cannot
/// smuggle a second command through the interactive terminal.
pub const VISIBLE_TERMINAL_COMMAND_INVALID: &str = "VISIBLE_TERMINAL_COMMAND_INVALID";
/// Writing to a terminal is allowed only after the caller explicitly makes it
/// the active workspace session.
pub const VISIBLE_TERMINAL_SESSION_NOT_ACTIVE: &str = "VISIBLE_TERMINAL_SESSION_NOT_ACTIVE";

/// Optional progress callback fired after FileTerm has restored the main
/// window and before the local sudo/su prompt starts waiting. AI Copilot and
/// the MCP/CLI bridge use it to tell their caller that the foreground prompt
/// is ready for the user.
pub type PrivilegedPromptNotice = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionApprovalSource {
    Mcp,
    AiCopilot,
}

#[derive(Clone, Debug)]
pub struct ActionApprovalDetails {
    pub title: String,
    pub summary: String,
    pub target: Option<String>,
    pub details: Option<String>,
    pub destructive: bool,
    pub requires_risk_acknowledgement: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionApprovalRequest {
    pub request_id: String,
    pub source: ActionApprovalSource,
    pub operation: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub destructive: bool,
    pub requires_risk_acknowledgement: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionApprovalDecision {
    Approved,
    Rejected,
    Dismissed,
    TimedOut,
    /// The user chose to run a Copilot proposal through the visible terminal.
    /// This is intentionally distinct from `Approved`: the background exec
    /// channel must not start a second copy of the command.
    DelegatedToTerminal,
}

impl ActionApprovalDecision {
    pub fn rejection_message(self, source: ActionApprovalSource) -> &'static str {
        match (source, self) {
            (_, Self::Approved) => "",
            (_, Self::DelegatedToTerminal) => {
                "Copilot command was delegated to the visible terminal"
            }
            (ActionApprovalSource::Mcp, Self::Rejected) => {
                "FileTerm external operation was rejected by the user"
            }
            (ActionApprovalSource::Mcp, Self::Dismissed) => {
                "FileTerm external approval dialog was closed"
            }
            (ActionApprovalSource::Mcp, Self::TimedOut) => {
                "FileTerm external approval timed out; the operation was not started"
            }
            (ActionApprovalSource::AiCopilot, Self::Rejected) => {
                "Copilot tool call was rejected by the user"
            }
            (ActionApprovalSource::AiCopilot, Self::Dismissed) => "Copilot approval was dismissed",
            (ActionApprovalSource::AiCopilot, Self::TimedOut) => {
                "Copilot approval timed out; the command was not started"
            }
        }
    }
}

/// Queue a one-time visible approval. The caller decides how a denied or
/// timed-out decision should be represented to its own user (MCP returns an
/// error; Copilot persists a tool result). A Copilot call may also return
/// `DelegatedToTerminal`, which explicitly skips the background exec path.
pub async fn request_action_approval(
    app: &AppHandle,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
) -> Result<ActionApprovalDecision, AppError> {
    let request_id = format!("action-approval-{}", uuid::Uuid::new_v4());
    request_action_approval_with_id(app, request_id, source, operation, details).await
}

/// Queue a one-time visible approval using a caller-supplied ID. Copilot uses
/// this to correlate the backend approval gate with the inline command card
/// that represents the same tool call in its conversation.
pub async fn request_action_approval_with_id(
    app: &AppHandle,
    request_id: String,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
) -> Result<ActionApprovalDecision, AppError> {
    let operation = operation.into();
    let (sender, receiver) = oneshot::channel();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state
        .pending_action_approvals
        .write()
        .await
        .insert(request_id.clone(), sender);

    if matches!(source, ActionApprovalSource::Mcp) {
        // MCP, one-shot CLI, and the persistent Agent all use the legacy
        // `Mcp` approval source on the wire. Bring the shared FileTerm
        // approval back to the main window so a hidden or unfocused desktop
        // window cannot leave the external caller waiting invisibly.
        crate::show_main_window(app);
    }

    let payload = ActionApprovalRequest {
        request_id: request_id.clone(),
        source,
        operation: operation.clone(),
        title: details.title,
        summary: details.summary,
        target: details.target,
        details: details.details,
        destructive: details.destructive,
        requires_risk_acknowledgement: details.requires_risk_acknowledgement,
    };
    if let Err(error) = app.emit("action:approval-request", payload) {
        state
            .pending_action_approvals
            .write()
            .await
            .remove(&request_id);
        return Err(AppError::Command(format!(
            "Unable to publish action approval request: {error}"
        )));
    }

    let decision = match timeout(ACTION_APPROVAL_TIMEOUT, receiver).await {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => ActionApprovalDecision::Dismissed,
        Err(_) => ActionApprovalDecision::TimedOut,
    };
    state
        .pending_action_approvals
        .write()
        .await
        .remove(&request_id);

    let outcome = match decision {
        ActionApprovalDecision::Approved => "granted",
        ActionApprovalDecision::Rejected => "denied",
        ActionApprovalDecision::Dismissed => "dismissed",
        ActionApprovalDecision::TimedOut => "timed-out",
        ActionApprovalDecision::DelegatedToTerminal => "delegated-to-terminal",
    };
    crate::services::logging::info(
        app,
        "action-review",
        format!("approval {outcome} source={source:?} operation={operation}"),
    );
    Ok(decision)
}

/// Resolve an in-app approval exactly once. An unknown ID is intentionally a
/// no-op: it may have already timed out or been dismissed while the renderer
/// was transitioning windows.
pub async fn resolve_action_approval(
    app: &AppHandle,
    request_id: &str,
    approved: bool,
) -> Result<(), AppError> {
    resolve_action_approval_decision(
        app,
        request_id,
        if approved {
            ActionApprovalDecision::Approved
        } else {
            ActionApprovalDecision::Rejected
        },
    )
    .await
}

/// Resolve an in-app approval with a specific outcome. This is kept separate
/// from the boolean approval API so the visible-terminal handoff cannot be
/// persisted or reported as a user rejection.
pub async fn resolve_action_approval_decision(
    app: &AppHandle,
    request_id: &str,
    decision: ActionApprovalDecision,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid action approval request".to_string(),
        ));
    }
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = {
        let mut pending = state.pending_action_approvals.write().await;
        pending.remove(request_id)
    };
    if let Some(sender) = sender {
        let _ = sender.send(decision);
    }
    Ok(())
}

/// Resolve a Copilot approval by handing the command to the already-visible
/// terminal. The Copilot tool loop receives a distinct result and must not
/// open its independent SSH exec channel afterwards.
pub async fn resolve_action_approval_as_terminal(
    app: &AppHandle,
    request_id: &str,
) -> Result<(), AppError> {
    resolve_action_approval_decision(app, request_id, ActionApprovalDecision::DelegatedToTerminal)
        .await
}

#[derive(Clone)]
pub struct RemoteExecRequest {
    pub tab_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    /// Optional identity binding used by Copilot. External callers leave it
    /// unset; a bound request is rejected if the SSH target changes before
    /// the isolated exec channel starts.
    pub expected_session_revision: Option<String>,
    /// One-shot values supplied by a trusted local caller. They are never
    /// logged, persisted, or returned to the caller.
    pub sudo_password: Option<String>,
    pub su_password: Option<String>,
    pub save_sudo_password: bool,
    pub save_su_password: bool,
    /// Whether a missing privileged credential may be resolved through the
    /// local FileTerm password prompt.
    pub allow_local_privileged_prompt: bool,
    /// Optional progress hook used by AI Copilot and the MCP/CLI bridge to show
    /// that the tool call is waiting for the user in the foreground FileTerm
    /// window.
    pub privileged_prompt_notice: Option<PrivilegedPromptNotice>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivilegedCommandKind {
    Sudo,
    Su,
}

#[derive(Debug)]
struct PreparedRemoteExec {
    command: String,
    stdin: Option<String>,
    request_pty: bool,
    kind: Option<PrivilegedCommandKind>,
    used_saved_password: bool,
    save_password: Option<(String, PrivilegedCommandKind, String)>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
    /// True when the command was sent through the visible network-device PTY.
    /// Network CLIs do not provide a reliable process exit code on this path.
    pub raw_terminal: bool,
    /// The isolated non-interactive channel detected a supported input prompt
    /// in its bounded output. This is only a routing hint for the Agent; no
    /// input value is ever collected or returned on this path.
    pub input_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_kind: Option<String>,
}

/// Run one explicit command through the normal FileTerm command boundary.
/// Server sessions use the independent exec channel; network-device sessions
/// retain the established raw-PTY fallback used by the renderer and Copilot.
pub async fn execute_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    execute_remote_command_inner(app, request, true).await
}

/// Run one command through the MCP background route. Unlike the normal
/// FileTerm/Copilot route, this function never falls back to the visible PTY
/// for a network-device session; the caller must choose the explicit visible
/// terminal tool instead.
pub async fn execute_background_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    execute_remote_command_inner(app, request, false).await
}

async fn execute_remote_command_inner(
    app: &AppHandle,
    request: RemoteExecRequest,
    allow_network_device_fallback: bool,
) -> Result<RemoteExecResult, AppError> {
    let tab_id = validate_remote_exec_tab_id(&request.tab_id)?;
    let command = validate_remote_exec_command(&request.command)?;
    let requested_cwd = validate_remote_exec_cwd(request.cwd)?;

    let timeout_ms = request
        .timeout_ms
        .unwrap_or(DEFAULT_REMOTE_EXEC_TIMEOUT_MS)
        .clamp(MIN_REMOTE_EXEC_TIMEOUT_MS, MAX_REMOTE_EXEC_TIMEOUT_MS);
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?
    };
    if session_type != "ssh" {
        return Err(AppError::Command(
            "Remote command execution is only supported for SSH sessions".to_string(),
        ));
    }
    // External CLI/MCP callers do not carry the AI context revision, but they
    // still need the same target binding: a disconnect/reconnect while the
    // command is waiting must not make it run against a replacement session
    // that happens to reuse the same tab ID.
    let bound_session_revision = match request.expected_session_revision.clone() {
        Some(expected) => Some(expected),
        None => Some(state.ai_session_revision(&tab_id).await.to_string()),
    };
    ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref()).await?;
    let (cwd, profile_id, host, shell_user, network_device) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.connected {
            return Err(AppError::Command(
                "FileTerm SSH session is not connected".to_string(),
            ));
        }
        let network_device = session.device_mode.as_deref() == Some("network-device");
        if !network_device && !session.capabilities.shell_integration {
            return Err(AppError::Command(
                "Remote command execution is disabled for this SSH session".to_string(),
            ));
        }
        (
            requested_cwd.clone().or_else(|| {
                (!network_device)
                    .then(|| session.shell_cwd.clone())
                    .flatten()
            }),
            session.profile_id.clone(),
            session.access_host.clone(),
            session.shell_user.clone(),
            network_device,
        )
    };

    if network_device {
        if !allow_network_device_fallback {
            return Err(AppError::Command(
                NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED.to_string(),
            ));
        }
        let command = validate_network_device_command(&request.command)?;
        if requested_cwd.is_some() {
            return Err(AppError::Command(
                NETWORK_DEVICE_CWD_UNSUPPORTED.to_string(),
            ));
        }
        if request.sudo_password.is_some()
            || request.su_password.is_some()
            || request.save_sudo_password
            || request.save_su_password
        {
            return Err(AppError::Command(
                NETWORK_DEVICE_PRIVILEGE_UNSUPPORTED.to_string(),
            ));
        }
        return execute_network_device_command(
            &state,
            &tab_id,
            &command,
            timeout_ms,
            bound_session_revision.as_deref(),
        )
        .await;
    }

    let initial_prepared = prepare_remote_exec(
        app,
        &profile_id,
        &command,
        request.sudo_password.clone(),
        request.su_password.clone(),
        request.save_sudo_password,
        request.save_su_password,
    );
    let prepared = match initial_prepared {
        Ok(prepared) => prepared,
        Err(error)
            if matches!(
                &error,
                AppError::Command(message)
                    if message == SUDO_PASSWORD_NEEDED || message == SU_PASSWORD_NEEDED
            ) =>
        {
            if !request.allow_local_privileged_prompt {
                return Err(error);
            }
            let kind = privileged_command_kind(&command).ok_or(error)?;
            let (password, save) = request_sudo_password_prompt(
                app,
                &state,
                &tab_id,
                bound_session_revision.as_deref(),
                kind,
                &host,
                shell_user.as_deref(),
                cwd.as_deref(),
                &command,
                request.privileged_prompt_notice.clone(),
            )
            .await?;
            ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref())
                .await?;
            prepare_remote_exec(
                app,
                &profile_id,
                &command,
                matches!(kind, PrivilegedCommandKind::Sudo).then_some(password.clone()),
                matches!(kind, PrivilegedCommandKind::Su).then_some(password),
                matches!(kind, PrivilegedCommandKind::Sudo) && save,
                matches!(kind, PrivilegedCommandKind::Su) && save,
            )?
        }
        Err(error) => return Err(error),
    };

    ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref()).await?;

    let result = crate::commands::send_worker_cmd_with_response_timeout(
        app,
        &tab_id,
        Duration::from_millis(timeout_ms.saturating_add(5_000)),
        |respond_to| WorkerCmd::ExecuteRemoteCommand {
            command: prepared.command.clone(),
            cwd,
            timeout_ms,
            stdin: prepared.stdin.clone(),
            request_pty: prepared.request_pty,
            respond_to,
        },
    )
    .await?;
    let parsed = parse_remote_exec_result(result)?;
    if let Some(kind) = prepared.kind {
        if detect_privileged_auth_failure(&parsed.output, kind) {
            if prepared.used_saved_password {
                let clear_result = match kind {
                    PrivilegedCommandKind::Sudo => {
                        crate::services::profile_ops::set_sudo_password(app, &profile_id, None)
                    }
                    PrivilegedCommandKind::Su => {
                        crate::services::profile_ops::set_su_password(app, &profile_id, None)
                    }
                };
                if let Err(error) = clear_result {
                    crate::services::logging::warn(
                        app,
                        "security",
                        format!("failed to clear invalid saved privileged password: {error}"),
                    );
                }
            }
            return Err(AppError::Command(
                match kind {
                    PrivilegedCommandKind::Sudo => SUDO_AUTH_FAILURE,
                    PrivilegedCommandKind::Su => SU_AUTH_FAILURE,
                }
                .to_string(),
            ));
        }
    }
    if let Some((profile_id, kind, password)) = prepared.save_password.as_ref() {
        match kind {
            PrivilegedCommandKind::Sudo => {
                crate::services::profile_ops::set_sudo_password(app, profile_id, Some(password))?
            }
            PrivilegedCommandKind::Su => {
                crate::services::profile_ops::set_su_password(app, profile_id, Some(password))?
            }
        }
    }
    Ok(parsed)
}

/// Resolve a visible-terminal target and require that it is the session the
/// workspace currently exposes as active. This is shared by the direct MCP
/// visible-command route and legacy visible command-template calls so an
/// external caller cannot write to a background tab by accident.
pub async fn ensure_visible_terminal_session_active(
    app: &AppHandle,
    raw_tab_id: &str,
) -> Result<String, AppError> {
    let (tab_id, _) = visible_terminal_session_context(app, raw_tab_id).await?;
    Ok(tab_id)
}

async fn visible_terminal_session_context(
    app: &AppHandle,
    raw_tab_id: &str,
) -> Result<(String, bool), AppError> {
    let tab_id = validate_remote_exec_tab_id(raw_tab_id)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let (root_tab_id, session_type) = {
        let tabs = state.tabs.read().await;
        let tab = tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        (
            tab.pane_root_tab_id
                .clone()
                .unwrap_or_else(|| tab.id.clone()),
            tab.session_type.clone(),
        )
    };
    let (connected, network_device) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        (
            session.connected,
            session.device_mode.as_deref() == Some("network-device"),
        )
    };
    if session_type != "ssh" {
        return Err(AppError::Command(
            "Visible command execution is only supported for SSH sessions".to_string(),
        ));
    }
    if !connected {
        return Err(AppError::Command(
            "FileTerm SSH session is not connected".to_string(),
        ));
    }

    let active_tab_id = state.active_tab_id.read().await.clone();
    let active_pane_tab_id = state
        .active_pane_tab_id_by_root
        .read()
        .await
        .get(&root_tab_id)
        .cloned();
    let active_pane_matches = active_pane_tab_id
        .as_deref()
        .map_or(root_tab_id == tab_id, |active_id| active_id == tab_id);
    if active_tab_id.as_deref() != Some(root_tab_id.as_str()) || !active_pane_matches {
        return Err(AppError::Command(
            VISIBLE_TERMINAL_SESSION_NOT_ACTIVE.to_string(),
        ));
    }
    Ok((tab_id, network_device))
}

/// Send one explicit command to the already-active visible SSH terminal.
/// Unlike `execute_remote_command`, this route deliberately does not create a
/// separate exec channel or collect a command result for server sessions: the
/// terminal owns the prompt, echo, output and any follow-up interaction.
pub async fn execute_visible_terminal_command(
    app: &AppHandle,
    raw_tab_id: &str,
    raw_command: &str,
    timeout_ms: Option<u64>,
) -> Result<RemoteExecResult, AppError> {
    let command = validate_visible_terminal_command(raw_command)?;
    let timeout_ms = timeout_ms
        .unwrap_or(DEFAULT_REMOTE_EXEC_TIMEOUT_MS)
        .clamp(MIN_REMOTE_EXEC_TIMEOUT_MS, MAX_REMOTE_EXEC_TIMEOUT_MS);
    let (tab_id, network_device) = visible_terminal_session_context(app, raw_tab_id).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    let expected_session_revision = state.ai_session_revision(&tab_id).await.to_string();
    ensure_expected_session_revision(&state, &tab_id, Some(&expected_session_revision)).await?;
    if network_device {
        return execute_network_device_command(
            &state,
            &tab_id,
            &command,
            timeout_ms,
            Some(&expected_session_revision),
        )
        .await;
    }

    crate::commands::send_exact_terminal_input(&state, &tab_id, format!("{command}\r")).await?;
    Ok(RemoteExecResult {
        output: String::new(),
        exit_code: None,
        timed_out: false,
        output_truncated: false,
        raw_terminal: true,
        input_required: false,
        input_kind: None,
    })
}

/// Send one single-line native network-device command to the existing shell
/// PTY. There is no portable command delimiter or exit-code protocol for
/// Cisco/Huawei/Comware CLIs, so completion is inferred from a quiet period
/// after the PTY starts producing new transcript data, matching the raw-PTY
/// behavior used by Netcatty for network-device agent commands.
async fn execute_network_device_command(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    command: &str,
    timeout_ms: u64,
    expected_session_revision: Option<&str>,
) -> Result<RemoteExecResult, AppError> {
    ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
    let transcript_before = terminal_transcript_snapshot(state, tab_id)
        .await
        .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
    crate::commands::send_exact_terminal_input(state, tab_id, format!("{command}\r")).await?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut latest_transcript = transcript_before.clone();
    let mut last_change_at = None;
    let mut timed_out = false;

    loop {
        // A reconnect can replace the terminal sender while this bounded
        // quiet-period wait is in progress. Abort as soon as the identity
        // changes so a stale request cannot wait out the whole timeout or
        // report output from the replacement session.
        ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
        if let Some(transcript) = terminal_transcript_snapshot(state, tab_id).await {
            if transcript != latest_transcript {
                latest_transcript = transcript;
                last_change_at = Some(Instant::now());
            }
            if last_change_at
                .is_some_and(|changed_at| changed_at.elapsed() >= NETWORK_DEVICE_RAW_IDLE_SETTLE)
            {
                break;
            }
        } else {
            return Err(AppError::Command(
                "FileTerm session was not found".to_string(),
            ));
        }

        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        sleep(NETWORK_DEVICE_RAW_POLL_INTERVAL).await;
    }

    let transcript_after = terminal_transcript_snapshot(state, tab_id)
        .await
        .unwrap_or(latest_transcript);
    ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
    let output = terminal_transcript_delta(&transcript_before, &transcript_after);
    let (output, output_truncated) = truncate_network_device_output(output);

    Ok(RemoteExecResult {
        output,
        exit_code: None,
        timed_out,
        output_truncated,
        raw_terminal: true,
        input_required: false,
        input_kind: None,
    })
}

async fn terminal_transcript_snapshot(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
) -> Option<String> {
    state
        .sessions
        .read()
        .await
        .get(tab_id)
        .map(|session| session.terminal_transcript.clone())
}

fn terminal_transcript_delta(before: &str, after: &str) -> String {
    after
        .strip_prefix(before)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| after.to_string())
}

fn truncate_network_device_output(output: String) -> (String, bool) {
    if output.len() <= NETWORK_DEVICE_RAW_OUTPUT_BYTES {
        return (output, false);
    }
    let mut start = output.len() - NETWORK_DEVICE_RAW_OUTPUT_BYTES;
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    (output[start..].to_string(), true)
}

async fn ensure_expected_session_revision(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    expected_session_revision: Option<&str>,
) -> Result<(), AppError> {
    let Some(expected_session_revision) = expected_session_revision else {
        return Ok(());
    };
    let expected_session_revision = expected_session_revision.trim();
    if expected_session_revision.is_empty()
        || expected_session_revision.len() > 64
        || expected_session_revision.chars().any(char::is_control)
    {
        return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
    }
    let current_session_revision = state.ai_session_revision(tab_id).await.to_string();
    if current_session_revision != expected_session_revision {
        return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
    }
    Ok(())
}

// The prompt payload is kept explicit at this boundary so the password source,
// target identity, and local-only display fields cannot be accidentally mixed
// with the visible terminal input route.
#[allow(clippy::too_many_arguments)]
async fn request_sudo_password_prompt(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    expected_session_revision: Option<&str>,
    kind: PrivilegedCommandKind,
    host: &str,
    shell_user: Option<&str>,
    cwd: Option<&str>,
    command: &str,
    privileged_prompt_notice: Option<PrivilegedPromptNotice>,
) -> Result<(String, bool), AppError> {
    let needed_code = match kind {
        PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
        PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
    };
    if !state.has_sudo_password_renderer().await || !main_window_exists(app) {
        return Err(AppError::Command(needed_code.to_string()));
    }
    crate::show_main_window(app);
    let current_session_revision = state.ai_session_revision(tab_id).await.to_string();
    let expected_session_revision = expected_session_revision
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&current_session_revision)
        .to_string();
    let request_id = format!("sudo-password-{}", uuid::Uuid::new_v4());
    let (sender, receiver) = oneshot::channel();
    let pending = crate::services::workspace::PendingSudoPassword {
        tab_id: tab_id.to_string(),
        expected_session_revision,
        sender,
    };
    if !state
        .insert_pending_sudo_password(request_id.clone(), pending)
        .await
    {
        return Err(AppError::Command(needed_code.to_string()));
    }
    let payload = serde_json::json!({
        "requestId": request_id,
        "tabId": tab_id,
        "kind": match kind {
            PrivilegedCommandKind::Sudo => "sudo",
            PrivilegedCommandKind::Su => "su",
        },
        "host": host,
        "shellUser": shell_user,
        "cwd": cwd,
        "command": command,
    });
    if let Err(error) = app.emit("sudo:password-request", payload) {
        state
            .pending_sudo_passwords
            .write()
            .await
            .remove(&request_id);
        crate::services::logging::warn(
            app,
            "security",
            format!("privileged password prompt delivery failed: {error}"),
        );
        return Err(AppError::Command(needed_code.to_string()));
    }
    if let Some(notice) = privileged_prompt_notice {
        notice(needed_code);
    }

    let response = match timeout(PRIVILEGED_PASSWORD_PROMPT_TIMEOUT, receiver).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) | Err(_) => {
            state
                .pending_sudo_passwords
                .write()
                .await
                .remove(&request_id);
            return Err(AppError::Command(
                match kind {
                    PrivilegedCommandKind::Sudo => SUDO_PASSWORD_CANCELLED,
                    PrivilegedCommandKind::Su => SU_PASSWORD_CANCELLED,
                }
                .to_string(),
            ));
        }
    };
    if response.cancelled {
        return Err(AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_CANCELLED,
                PrivilegedCommandKind::Su => SU_PASSWORD_CANCELLED,
            }
            .to_string(),
        ));
    }
    let password = response.value.ok_or_else(|| {
        AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
                PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
            }
            .to_string(),
        )
    })?;
    validate_privileged_password(&password)?;
    Ok((password, response.save))
}

fn main_window_exists(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some()
}

fn privileged_command_kind(command: &str) -> Option<PrivilegedCommandKind> {
    let trimmed = command.trim_start();
    if trimmed == "sudo"
        || trimmed
            .strip_prefix("sudo")
            .is_some_and(starts_with_shell_space)
    {
        return Some(PrivilegedCommandKind::Sudo);
    }
    if trimmed == "su"
        || trimmed
            .strip_prefix("su")
            .is_some_and(starts_with_shell_space)
    {
        return Some(PrivilegedCommandKind::Su);
    }
    None
}

fn starts_with_shell_space(value: &str) -> bool {
    value.chars().next().is_some_and(char::is_whitespace)
}

fn validate_privileged_password(password: &str) -> Result<(), AppError> {
    if password.is_empty()
        || password.len() > MAX_REMOTE_EXEC_SECRET_BYTES
        || password.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "Privileged command password input is invalid".to_string(),
        ));
    }
    Ok(())
}

fn wrap_sudo_command(command: &str) -> String {
    let trimmed = command.trim_start();
    let suffix = trimmed.strip_prefix("sudo").unwrap_or_default();
    format!("sudo -S -p ''{suffix}")
}

fn resolve_privileged_password(
    kind: PrivilegedCommandKind,
    explicit_password: Option<String>,
    saved_password: Option<String>,
) -> Result<String, AppError> {
    let password = explicit_password.or(saved_password).ok_or_else(|| {
        AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
                PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
            }
            .to_string(),
        )
    })?;
    validate_privileged_password(&password)?;
    Ok(password)
}

fn detect_privileged_auth_failure(output: &str, kind: PrivilegedCommandKind) -> bool {
    let output = output.to_ascii_lowercase();
    let patterns: &[&str] = match kind {
        PrivilegedCommandKind::Sudo => &[
            "sorry, try again",
            "incorrect password",
            "authentication failure",
            "a password is required",
        ],
        PrivilegedCommandKind::Su => &[
            "su: authentication failure",
            "su: incorrect password",
            "su: sorry",
            "authentication failure",
        ],
    };
    patterns.iter().any(|pattern| output.contains(pattern))
}

fn prepare_remote_exec(
    app: &AppHandle,
    profile_id: &str,
    command: &str,
    sudo_password: Option<String>,
    su_password: Option<String>,
    save_sudo_password: bool,
    save_su_password: bool,
) -> Result<PreparedRemoteExec, AppError> {
    let kind = privileged_command_kind(command);
    let has_any_credential_input =
        sudo_password.is_some() || su_password.is_some() || save_sudo_password || save_su_password;
    let Some(kind) = kind else {
        if has_any_credential_input {
            return Err(AppError::Command(
                "Privileged password parameters require a sudo or su command".to_string(),
            ));
        }
        return Ok(PreparedRemoteExec {
            command: command.to_string(),
            stdin: None,
            request_pty: false,
            kind: None,
            used_saved_password: false,
            save_password: None,
        });
    };

    let (explicit_password, save_password) = match kind {
        PrivilegedCommandKind::Sudo => {
            if su_password.is_some() || save_su_password {
                return Err(AppError::Command(
                    "su password parameters cannot be used with a sudo command".to_string(),
                ));
            }
            (sudo_password, save_sudo_password)
        }
        PrivilegedCommandKind::Su => {
            if sudo_password.is_some() || save_sudo_password {
                return Err(AppError::Command(
                    "sudo password parameters cannot be used with a su command".to_string(),
                ));
            }
            (su_password, save_su_password)
        }
    };

    if save_password && explicit_password.is_none() {
        return Err(AppError::Command(
            "Saving a privileged password requires a one-shot password value".to_string(),
        ));
    }
    let saved_password = match kind {
        PrivilegedCommandKind::Sudo => {
            crate::services::profile_ops::read_sudo_password(app, profile_id)?
        }
        PrivilegedCommandKind::Su => {
            crate::services::profile_ops::read_su_password(app, profile_id)?
        }
    };
    let used_saved_password = explicit_password.is_none() && saved_password.is_some();
    let password = resolve_privileged_password(kind, explicit_password, saved_password)?;

    let save_password = if save_password {
        Some((profile_id.to_string(), kind, password.clone()))
    } else {
        None
    };
    Ok(PreparedRemoteExec {
        command: match kind {
            PrivilegedCommandKind::Sudo => wrap_sudo_command(command),
            PrivilegedCommandKind::Su => command.to_string(),
        },
        stdin: Some(format!("{password}\n")),
        request_pty: matches!(kind, PrivilegedCommandKind::Su),
        kind: Some(kind),
        used_saved_password,
        save_password,
    })
}

fn validate_remote_exec_tab_id(raw_tab_id: &str) -> Result<String, AppError> {
    let tab_id = raw_tab_id.trim().to_string();
    if tab_id.is_empty()
        || tab_id.len() > MAX_REMOTE_EXEC_TAB_ID_BYTES
        || tab_id.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "FileTerm session was not found".to_string(),
        ));
    }
    Ok(tab_id)
}

fn validate_remote_exec_command(raw_command: &str) -> Result<String, AppError> {
    let command = raw_command.trim().to_string();
    if command.is_empty() {
        return Err(AppError::Command(
            "Remote command must not be empty".to_string(),
        ));
    }
    if command.len() > MAX_REMOTE_EXEC_COMMAND_BYTES {
        return Err(AppError::Command(format!(
            "Remote command exceeds the {} KiB limit",
            MAX_REMOTE_EXEC_COMMAND_BYTES / 1024
        )));
    }
    Ok(command)
}

fn validate_network_device_command(raw_command: &str) -> Result<String, AppError> {
    if raw_command.chars().any(char::is_control) {
        return Err(AppError::Command(
            NETWORK_DEVICE_COMMAND_INVALID.to_string(),
        ));
    }
    validate_remote_exec_command(raw_command)
}

fn validate_visible_terminal_command(raw_command: &str) -> Result<String, AppError> {
    if raw_command.chars().any(char::is_control) {
        return Err(AppError::Command(
            VISIBLE_TERMINAL_COMMAND_INVALID.to_string(),
        ));
    }
    validate_remote_exec_command(raw_command)
}

fn validate_remote_exec_cwd(cwd: Option<String>) -> Result<Option<String>, AppError> {
    let cwd = cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if cwd
        .as_ref()
        .is_some_and(|value| value.len() > MAX_REMOTE_EXEC_CWD_BYTES)
    {
        return Err(AppError::Command(
            "Remote command working directory is too long".to_string(),
        ));
    }
    Ok(cwd)
}

fn parse_remote_exec_result(value: Value) -> Result<RemoteExecResult, AppError> {
    let output = value
        .get("output")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Serialization("Remote command output was invalid".to_string()))?
        .to_string();
    let exit_code = value
        .get("exitCode")
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| AppError::Serialization("Remote command exit code was invalid".to_string()))?;
    let timed_out = value
        .get("timedOut")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            AppError::Serialization("Remote command timeout state was invalid".to_string())
        })?;
    let output_truncated = value
        .get("outputTruncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_terminal = value
        .get("rawTerminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let input_kind = value
        .get("inputKind")
        .and_then(Value::as_str)
        .filter(|kind| matches!(*kind, "secret" | "text"))
        .map(ToOwned::to_owned);
    let input_required = value
        .get("inputRequired")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| input_kind.is_some());
    let input_required = input_required && input_kind.is_some();
    Ok(RemoteExecResult {
        output,
        exit_code,
        timed_out,
        output_truncated,
        raw_terminal,
        input_required,
        input_kind,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        detect_privileged_auth_failure, parse_remote_exec_result, privileged_command_kind,
        resolve_privileged_password, validate_network_device_command, validate_privileged_password,
        validate_remote_exec_command, validate_remote_exec_cwd, validate_remote_exec_tab_id,
        validate_visible_terminal_command, wrap_sudo_command, ActionApprovalDecision,
        ActionApprovalSource, PrivilegedCommandKind, SUDO_AUTH_FAILURE,
        VISIBLE_TERMINAL_COMMAND_INVALID,
    };
    use crate::AppError;
    use serde_json::json;

    #[test]
    fn approval_rejections_remain_specific_to_the_initiating_surface() {
        assert_eq!(
            ActionApprovalDecision::Rejected.rejection_message(ActionApprovalSource::Mcp),
            "FileTerm external operation was rejected by the user"
        );
        assert_eq!(
            ActionApprovalDecision::TimedOut.rejection_message(ActionApprovalSource::AiCopilot),
            "Copilot approval timed out; the command was not started"
        );
    }

    #[test]
    fn remote_exec_parser_preserves_the_bounded_output_signal() {
        let result = parse_remote_exec_result(json!({
            "output": "partial output",
            "exitCode": 0,
            "timedOut": false,
            "outputTruncated": true,
        }))
        .expect("remote exec result should parse");

        assert_eq!(result.output, "partial output");
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert!(result.output_truncated);
        assert!(!result.raw_terminal);
        assert!(!result.input_required);
        assert_eq!(result.input_kind, None);
    }

    #[test]
    fn remote_exec_parser_exposes_only_a_bounded_input_hint() {
        let result = parse_remote_exec_result(json!({
            "output": "Password: ",
            "exitCode": null,
            "timedOut": true,
            "outputTruncated": false,
            "inputRequired": true,
            "inputKind": "secret",
        }))
        .expect("remote exec input hint should parse");

        assert!(result.input_required);
        assert_eq!(result.input_kind.as_deref(), Some("secret"));

        let invalid_kind = parse_remote_exec_result(json!({
            "output": "Continue? [y/N]",
            "exitCode": null,
            "timedOut": true,
            "outputTruncated": false,
            "inputRequired": true,
            "inputKind": "password",
        }))
        .expect("invalid input kind should not break result parsing");
        assert!(!invalid_kind.input_required);
        assert_eq!(invalid_kind.input_kind, None);
    }

    #[test]
    fn network_device_commands_are_single_line_and_marked_raw() {
        assert_eq!(
            validate_network_device_command("display version").unwrap(),
            "display version"
        );
        assert!(validate_network_device_command("display version\r").is_err());
        assert!(validate_network_device_command("display\nversion").is_err());
    }

    #[test]
    fn visible_terminal_commands_are_single_line() {
        assert_eq!(
            validate_visible_terminal_command("uname -a").unwrap(),
            "uname -a"
        );
        let error = validate_visible_terminal_command("printf 'first\nsecond'").unwrap_err();
        assert!(matches!(
            error,
            AppError::Command(message) if message == VISIBLE_TERMINAL_COMMAND_INVALID
        ));
    }

    #[test]
    fn remote_exec_validators_reject_empty_and_unsafe_routing_values() {
        assert!(validate_remote_exec_tab_id("\n").is_err());
        assert!(validate_remote_exec_command("  ").is_err());
        assert!(validate_remote_exec_cwd(Some("x".repeat(4_097))).is_err());
        assert_eq!(
            validate_remote_exec_cwd(Some(" /srv/app ".to_string())).unwrap(),
            Some("/srv/app".to_string())
        );
    }

    #[test]
    fn privileged_detection_only_accepts_a_leading_shell_token() {
        assert_eq!(
            privileged_command_kind("  sudo -S id"),
            Some(PrivilegedCommandKind::Sudo)
        );
        assert_eq!(
            privileged_command_kind("su -c 'id'"),
            Some(PrivilegedCommandKind::Su)
        );
        assert_eq!(privileged_command_kind("sudoers --check"), None);
        assert_eq!(privileged_command_kind("echo sudo id"), None);
    }

    #[test]
    fn sudo_wrapper_keeps_password_out_of_the_command_text() {
        let command = wrap_sudo_command("  sudo -u root id");
        assert_eq!(command, "sudo -S -p '' -u root id");
        assert!(!command.contains("secret"));
    }

    #[test]
    fn privileged_auth_failures_are_classified_without_returning_remote_output() {
        assert!(detect_privileged_auth_failure(
            "sudo: incorrect password",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "sudo: 3 incorrect password attempts",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "sudo: authentication failure",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "su: Authentication failure",
            PrivilegedCommandKind::Su
        ));
        assert!(!detect_privileged_auth_failure(
            "command completed",
            PrivilegedCommandKind::Sudo
        ));
        assert_eq!(SUDO_AUTH_FAILURE, "SUDO_AUTH_FAILURE");
    }

    #[test]
    fn privileged_password_validation_rejects_control_input() {
        assert!(validate_privileged_password("secret").is_ok());
        assert!(validate_privileged_password("").is_err());
        assert!(validate_privileged_password("secret\n").is_err());
    }

    #[test]
    fn privileged_password_priority_is_explicit_then_saved_without_login_fallback() {
        assert_eq!(
            resolve_privileged_password(
                PrivilegedCommandKind::Sudo,
                Some("explicit".to_string()),
                Some("saved".to_string()),
            )
            .unwrap(),
            "explicit"
        );
        assert_eq!(
            resolve_privileged_password(
                PrivilegedCommandKind::Sudo,
                None,
                Some("saved".to_string()),
            )
            .unwrap(),
            "saved"
        );
        let error = resolve_privileged_password(PrivilegedCommandKind::Su, None, None).unwrap_err();
        assert!(matches!(error, AppError::Command(message) if message == "SU_PASSWORD_NEEDED"));
    }
}
