//! Shared one-time action approval and bounded remote exec support.
//!
//! MCP requests originate outside the renderer, while AI Review Mode starts
//! from a visible command card. Both flows still need the same fail-closed
//! approval queue and the same dedicated SSH exec boundary. Keeping those
//! primitives here prevents either surface from silently gaining a shortcut
//! around user confirmation or the interactive terminal.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio::time::timeout;

use crate::services::interactive_exec_audit::{
    self, InteractiveRemoteExecAuditContext, InteractiveRemoteExecAuditEvent,
    InteractiveRemoteExecAuditResult, InteractiveRemoteExecAuditSource,
    InteractiveRemoteExecAuditTarget,
};
use crate::sessions::WorkerCmd;
use crate::AppError;

pub const ACTION_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_REMOTE_EXEC_COMMAND_BYTES: usize = 64 * 1024;
const MAX_REMOTE_EXEC_CWD_BYTES: usize = 4 * 1024;
const MAX_REMOTE_EXEC_TAB_ID_BYTES: usize = 256;
pub const DEFAULT_REMOTE_EXEC_TIMEOUT_MS: u64 = 60_000;
pub const MIN_REMOTE_EXEC_TIMEOUT_MS: u64 = 1_000;
pub const MAX_REMOTE_EXEC_TIMEOUT_MS: u64 = 120_000;
const MAX_REMOTE_EXEC_SECRET_BYTES: usize = 4 * 1024;
pub const SUDO_PASSWORD_NEEDED: &str = "SUDO_PASSWORD_NEEDED";
pub const SU_PASSWORD_NEEDED: &str = "SU_PASSWORD_NEEDED";
pub const SUDO_PASSWORD_CANCELLED: &str = "SUDO_PASSWORD_CANCELLED";
pub const SU_PASSWORD_CANCELLED: &str = "SU_PASSWORD_CANCELLED";
pub const SUDO_AUTH_FAILURE: &str = "SUDO_AUTH_FAILURE";
pub const SU_AUTH_FAILURE: &str = "SU_AUTH_FAILURE";
/// Interactive exec keeps its own SSH PTY, so it can wait for a FileTerm
/// dialog without hijacking the visible terminal. Its execution budget is
/// deliberately longer than the normal fire-and-forget exec budget.
pub const DEFAULT_INTERACTIVE_REMOTE_EXEC_TIMEOUT_MS: u64 = 300_000;
pub const MAX_INTERACTIVE_REMOTE_EXEC_TIMEOUT_MS: u64 = 600_000;
pub const INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT: Duration = Duration::from_secs(300);
pub const INTERACTIVE_REMOTE_EXEC_UNAVAILABLE: &str = "INTERACTIVE_REMOTE_EXEC_UNAVAILABLE";
pub const INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE: &str =
    "INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE";
pub const INTERACTIVE_REMOTE_EXEC_TARGET_CHANGED: &str = "INTERACTIVE_REMOTE_EXEC_TARGET_CHANGED";
pub const INTERACTIVE_REMOTE_EXEC_USER_CANCELLED: &str = "INTERACTIVE_REMOTE_EXEC_USER_CANCELLED";
pub const INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT_CODE: &str =
    "INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT";
pub const INTERACTIVE_REMOTE_EXEC_TOO_MANY_PROMPTS: &str =
    "INTERACTIVE_REMOTE_EXEC_TOO_MANY_PROMPTS";
pub const INTERACTIVE_REMOTE_EXEC_BUSY: &str = "INTERACTIVE_REMOTE_EXEC_BUSY";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionApprovalSource {
    Mcp,
    AiReview,
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
}

impl ActionApprovalDecision {
    pub fn rejection_message(self, source: ActionApprovalSource) -> &'static str {
        match (source, self) {
            (_, Self::Approved) => "",
            (ActionApprovalSource::Mcp, Self::Rejected) => "MCP operation was rejected by the user",
            (ActionApprovalSource::Mcp, Self::Dismissed) => "MCP approval dialog was closed",
            (ActionApprovalSource::Mcp, Self::TimedOut) => {
                "MCP approval timed out; the operation was not started"
            }
            (ActionApprovalSource::AiReview, Self::Rejected) => {
                "AI review was rejected by the user"
            }
            (ActionApprovalSource::AiReview, Self::Dismissed) => {
                "AI review approval dialog was closed"
            }
            (ActionApprovalSource::AiReview, Self::TimedOut) => {
                "AI review approval timed out; the command was not started"
            }
            (ActionApprovalSource::AiCopilot, Self::Rejected) => {
                "Copilot tool call was rejected by the user"
            }
            (ActionApprovalSource::AiCopilot, Self::Dismissed) => {
                "Copilot approval dialog was closed"
            }
            (ActionApprovalSource::AiCopilot, Self::TimedOut) => {
                "Copilot approval timed out; the command was not started"
            }
        }
    }
}

/// Queue a one-time visible approval. The caller decides how a denied or
/// timed-out decision should be represented to its own user (MCP returns an
/// error; AI persists it as a review record), but neither caller can execute
/// before this method returns `Approved`.
pub async fn request_action_approval(
    app: &AppHandle,
    source: ActionApprovalSource,
    operation: impl Into<String>,
    details: ActionApprovalDetails,
) -> Result<ActionApprovalDecision, AppError> {
    let operation = operation.into();
    let request_id = format!("action-approval-{}", uuid::Uuid::new_v4());
    let (sender, receiver) = oneshot::channel();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state
        .pending_action_approvals
        .write()
        .await
        .insert(request_id.clone(), sender);

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
            "Unable to show action approval dialog: {error}"
        )));
    }

    let decision = match timeout(ACTION_APPROVAL_TIMEOUT, receiver).await {
        Ok(Ok(true)) => ActionApprovalDecision::Approved,
        Ok(Ok(false)) => ActionApprovalDecision::Rejected,
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
        let _ = sender.send(approved);
    }
    Ok(())
}

#[derive(Clone, Debug)]
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
    /// local FileTerm password prompt. Copilot full-auto keeps this false so
    /// an unattended tool call fails closed with `*_PASSWORD_NEEDED`.
    pub allow_local_privileged_prompt: bool,
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
    save_password: Option<(String, PrivilegedCommandKind, String)>,
}

#[derive(Clone, Debug)]
pub struct InteractiveRemoteExecRequest {
    pub tab_id: String,
    /// Monotonic target identity returned by get_session_context. This rejects
    /// a request that was planned for an earlier login/user/CWD target.
    pub expected_session_revision: String,
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
    /// The isolated non-interactive channel detected a supported input prompt
    /// in its bounded output. This is only a routing hint for the Agent; no
    /// input value is ever collected or returned on this path.
    pub input_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_kind: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub interaction_count: u8,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

/// Run one explicit command through the SSH worker's independent exec
/// channel. It never writes to the interactive PTY and the worker retains a
/// bounded output buffer, reported as `outputTruncated` to callers.
pub async fn execute_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    let tab_id = request.tab_id.trim().to_string();
    if tab_id.is_empty()
        || tab_id.len() > MAX_REMOTE_EXEC_TAB_ID_BYTES
        || tab_id.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "FileTerm session was not found".to_string(),
        ));
    }
    let command = request.command.trim().to_string();
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
    let cwd = request
        .cwd
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
    ensure_expected_session_revision(
        &state,
        &tab_id,
        request.expected_session_revision.as_deref(),
    )
    .await?;
    let (cwd, profile_id, host, shell_user) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.connected {
            return Err(AppError::Command(
                "FileTerm SSH session is not connected".to_string(),
            ));
        }
        (
            cwd.or_else(|| session.shell_cwd.clone()),
            session.profile_id.clone(),
            session.access_host.clone(),
            session.shell_user.clone(),
        )
    };

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
                request.expected_session_revision.as_deref(),
                kind,
                &host,
                shell_user.as_deref(),
                cwd.as_deref(),
                &command,
            )
            .await?;
            ensure_expected_session_revision(
                &state,
                &tab_id,
                request.expected_session_revision.as_deref(),
            )
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

    ensure_expected_session_revision(
        &state,
        &tab_id,
        request.expected_session_revision.as_deref(),
    )
    .await?;

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
) -> Result<(String, bool), AppError> {
    let needed_code = match kind {
        PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
        PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
    };
    if !state.has_sudo_password_renderer().await {
        return Err(AppError::Command(needed_code.to_string()));
    }
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
        return Err(AppError::Command(format!(
            "Unable to show privileged password prompt: {error}"
        )));
    }

    let response = match timeout(Duration::from_secs(120), receiver).await {
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

/// Run a command through an isolated SSH PTY only when the caller explicitly
/// opted into an interaction-capable operation. Passwords, MFA tokens and
/// confirmations are requested by the FileTerm renderer and are redacted from
/// the eventual result before it returns to MCP / CLI.
pub async fn execute_interactive_remote_command(
    app: &AppHandle,
    request: InteractiveRemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    execute_interactive_remote_command_from_source(
        app,
        request,
        InteractiveRemoteExecAuditSource::Desktop,
    )
    .await
}

/// Run an interactive remote task from a named local integration surface.
/// This is intentionally separate from the normal exec path: only this
/// function creates the task-local secure-input flow and its minimal audit.
pub async fn execute_interactive_remote_command_from_source(
    app: &AppHandle,
    request: InteractiveRemoteExecRequest,
    audit_source: InteractiveRemoteExecAuditSource,
) -> Result<RemoteExecResult, AppError> {
    let tab_id = validate_remote_exec_tab_id(&request.tab_id)?;
    let expected_session_revision = request.expected_session_revision.trim();
    if expected_session_revision.is_empty()
        || expected_session_revision.len() > 64
        || expected_session_revision.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "Interactive remote exec requires a valid session revision".to_string(),
        ));
    }
    let command = validate_remote_exec_command(&request.command)?;
    let cwd = validate_remote_exec_cwd(request.cwd)?;
    let timeout_ms = request
        .timeout_ms
        .unwrap_or(DEFAULT_INTERACTIVE_REMOTE_EXEC_TIMEOUT_MS)
        .clamp(
            MIN_REMOTE_EXEC_TIMEOUT_MS,
            MAX_INTERACTIVE_REMOTE_EXEC_TIMEOUT_MS,
        );

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
            .ok_or_else(|| {
                AppError::Command(format!(
                    "{INTERACTIVE_REMOTE_EXEC_UNAVAILABLE}: FileTerm session was not found"
                ))
            })?
    };
    if session_type != "ssh" {
        return Err(AppError::Command(
            format!(
                "{INTERACTIVE_REMOTE_EXEC_UNAVAILABLE}: interactive remote command execution is only supported for SSH sessions"
            ),
        ));
    }
    let current_session_revision = state.ai_session_revision(&tab_id).await.to_string();
    let (cwd, audit_target) = {
        let sessions = state.sessions.read().await;
        let session = sessions.get(&tab_id).ok_or_else(|| {
            AppError::Command(format!(
                "{INTERACTIVE_REMOTE_EXEC_UNAVAILABLE}: FileTerm session was not found"
            ))
        })?;
        if !session.connected {
            return Err(AppError::Command(format!(
                "{INTERACTIVE_REMOTE_EXEC_UNAVAILABLE}: FileTerm SSH session is not connected"
            )));
        }
        if current_session_revision != expected_session_revision {
            return Err(AppError::Command(
                format!(
                    "{INTERACTIVE_REMOTE_EXEC_TARGET_CHANGED}: refresh session context before interactive exec"
                ),
            ));
        }
        let effective_cwd = cwd.or_else(|| session.shell_cwd.clone());
        (
            effective_cwd.clone(),
            InteractiveRemoteExecAuditTarget {
                host: session.access_host.clone(),
                shell_user: session.shell_user.clone(),
                cwd: effective_cwd,
            },
        )
    };
    // Interactive exec is explicitly the secure local-input route. Refuse
    // before opening the isolated PTY when the main renderer cannot show a
    // task-local prompt, rather than creating a background SSH command that
    // might later ask the Agent to use the visible terminal.
    if !state.has_remote_exec_interaction_renderer().await {
        return Err(AppError::Command(format!(
            "{INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE}: FileTerm's main window is not ready to collect secure local input"
        )));
    }
    let audit_context =
        InteractiveRemoteExecAuditContext::new(audit_source, audit_target, &command);
    // Do not start an interaction-capable task if we cannot establish the
    // local, owner-only audit trail. This is the only fail-closed I/O before
    // the remote command can receive user-provided secrets.
    interactive_exec_audit::record(
        app,
        &audit_context,
        InteractiveRemoteExecAuditEvent::Started,
        0,
        None,
    )
    .await?;

    let response_timeout = Duration::from_millis(timeout_ms.saturating_add(10_000))
        .saturating_add(INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT.saturating_mul(3));
    let worker_result = crate::commands::send_worker_cmd_with_response_timeout(
        app,
        &tab_id,
        response_timeout,
        |respond_to| WorkerCmd::ExecuteInteractiveRemoteCommand {
            expected_session_revision: expected_session_revision.to_string(),
            command,
            cwd,
            timeout_ms,
            audit_context: audit_context.clone(),
            respond_to,
        },
    )
    .await;
    let result = worker_result.and_then(parse_remote_exec_result);

    let (event, details) = match &result {
        Ok(result) if result.timed_out => (
            InteractiveRemoteExecAuditEvent::TimedOut,
            Some(InteractiveRemoteExecAuditResult {
                exit_code: result.exit_code,
                timed_out: true,
                output_truncated: result.output_truncated,
            }),
        ),
        Ok(result) => (
            InteractiveRemoteExecAuditEvent::Completed,
            Some(InteractiveRemoteExecAuditResult {
                exit_code: result.exit_code,
                timed_out: false,
                output_truncated: result.output_truncated,
            }),
        ),
        Err(error) => (interactive_remote_exec_audit_error_event(error), None),
    };
    if interactive_exec_audit::record(
        app,
        &audit_context,
        event,
        audit_context.interaction_count(),
        details,
    )
    .await
    .is_err()
    {
        // The task already started or ended. Do not surface another error
        // that would cause callers to retry a remote command; the warning has
        // no command, prompt, answer, or output data.
        crate::services::logging::warn(
            app,
            "interactive-remote-exec",
            "unable to persist terminal interactive-exec audit completion",
        );
    }
    result
}

fn interactive_remote_exec_audit_error_event(error: &AppError) -> InteractiveRemoteExecAuditEvent {
    let text = match error {
        AppError::Clipboard(message)
        | AppError::Storage(message)
        | AppError::Serialization(message)
        | AppError::Window(message)
        | AppError::Command(message) => message.to_ascii_lowercase(),
    };
    if text.contains(&INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE.to_ascii_lowercase())
        || text.contains(&INTERACTIVE_REMOTE_EXEC_UNAVAILABLE.to_ascii_lowercase())
        || text.contains("not connected")
        || text.contains("session not found")
        || text.contains("exec 通道")
    {
        InteractiveRemoteExecAuditEvent::Unavailable
    } else if text.contains(&INTERACTIVE_REMOTE_EXEC_USER_CANCELLED.to_ascii_lowercase())
        || text.contains("cancelled")
        || text.contains("dismissed")
        || text.contains("empty")
    {
        InteractiveRemoteExecAuditEvent::Cancelled
    } else if text.contains(&INTERACTIVE_REMOTE_EXEC_INPUT_TIMEOUT_CODE.to_ascii_lowercase())
        || text.contains("timed out")
        || text.contains("超时")
    {
        InteractiveRemoteExecAuditEvent::TimedOut
    } else if text.contains(&INTERACTIVE_REMOTE_EXEC_TARGET_CHANGED.to_ascii_lowercase())
        || text.contains("target changed")
        || text.contains("target is no longer")
    {
        InteractiveRemoteExecAuditEvent::TargetChanged
    } else {
        InteractiveRemoteExecAuditEvent::Failed
    }
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

fn detect_privileged_auth_failure(output: &str, kind: PrivilegedCommandKind) -> bool {
    let output = output.to_ascii_lowercase();
    let patterns: &[&str] = match kind {
        PrivilegedCommandKind::Sudo => &[
            "sorry, try again",
            "sudo: incorrect password",
            "sudo: authentication failure",
            "sudo: a password is required",
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
    if let Some(password) = explicit_password.as_deref() {
        validate_privileged_password(password)?;
    }

    let password = if let Some(password) = explicit_password {
        Some(password)
    } else {
        match kind {
            PrivilegedCommandKind::Sudo => {
                if let Some(password) =
                    crate::services::profile_ops::read_sudo_password(app, profile_id)?
                {
                    Some(password)
                } else if crate::services::profile_ops::sudo_same_as_login(app, profile_id)? {
                    crate::services::profile_ops::read_login_password(app, profile_id)?
                } else {
                    None
                }
            }
            PrivilegedCommandKind::Su => {
                crate::services::profile_ops::read_su_password(app, profile_id)?
            }
        }
    };
    let Some(password) = password else {
        return Err(AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
                PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
            }
            .to_string(),
        ));
    };
    validate_privileged_password(&password)?;

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
    let interaction_count = value
        .get("interactionCount")
        .and_then(Value::as_u64)
        .map(u8::try_from)
        .transpose()
        .map_err(|_| {
            AppError::Serialization("Remote command interaction count was invalid".to_string())
        })?
        .unwrap_or(0);
    Ok(RemoteExecResult {
        output,
        exit_code,
        timed_out,
        output_truncated,
        input_required,
        input_kind,
        interaction_count,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        detect_privileged_auth_failure, interactive_remote_exec_audit_error_event,
        parse_remote_exec_result, privileged_command_kind, validate_privileged_password,
        validate_remote_exec_command, validate_remote_exec_cwd, validate_remote_exec_tab_id,
        wrap_sudo_command, ActionApprovalDecision, ActionApprovalSource,
        InteractiveRemoteExecAuditEvent, PrivilegedCommandKind,
        INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE, INTERACTIVE_REMOTE_EXEC_USER_CANCELLED,
        SUDO_AUTH_FAILURE,
    };
    use crate::AppError;
    use serde_json::json;

    #[test]
    fn approval_rejections_remain_specific_to_the_initiating_surface() {
        assert_eq!(
            ActionApprovalDecision::Rejected.rejection_message(ActionApprovalSource::Mcp),
            "MCP operation was rejected by the user"
        );
        assert_eq!(
            ActionApprovalDecision::TimedOut.rejection_message(ActionApprovalSource::AiReview),
            "AI review approval timed out; the command was not started"
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
        assert!(!result.input_required);
        assert_eq!(result.input_kind, None);
        assert_eq!(result.interaction_count, 0);
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
    fn interactive_exec_keeps_renderer_loss_distinct_from_user_cancellation() {
        let unavailable = AppError::Storage(format!(
            "{INTERACTIVE_REMOTE_EXEC_RENDERER_UNAVAILABLE}: main workspace is unavailable"
        ));
        assert!(matches!(
            interactive_remote_exec_audit_error_event(&unavailable),
            InteractiveRemoteExecAuditEvent::Unavailable
        ));

        let cancelled = AppError::Storage(format!(
            "{INTERACTIVE_REMOTE_EXEC_USER_CANCELLED}: user cancelled secure local input"
        ));
        assert!(matches!(
            interactive_remote_exec_audit_error_event(&cancelled),
            InteractiveRemoteExecAuditEvent::Cancelled
        ));
    }

    #[test]
    fn interactive_exec_parser_returns_the_local_prompt_count() {
        let result = parse_remote_exec_result(json!({
            "output": "[REDACTED]",
            "exitCode": 0,
            "timedOut": false,
            "outputTruncated": false,
            "interactionCount": 2,
        }))
        .expect("interactive remote exec result should parse");

        assert_eq!(result.interaction_count, 2);
    }
}
