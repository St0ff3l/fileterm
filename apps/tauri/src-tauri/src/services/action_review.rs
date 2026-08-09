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

use crate::sessions::WorkerCmd;
use crate::AppError;

pub const ACTION_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_REMOTE_EXEC_COMMAND_BYTES: usize = 64 * 1024;
const MAX_REMOTE_EXEC_CWD_BYTES: usize = 4 * 1024;
const MAX_REMOTE_EXEC_TAB_ID_BYTES: usize = 256;
pub const DEFAULT_REMOTE_EXEC_TIMEOUT_MS: u64 = 60_000;
pub const MIN_REMOTE_EXEC_TIMEOUT_MS: u64 = 1_000;
pub const MAX_REMOTE_EXEC_TIMEOUT_MS: u64 = 120_000;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionApprovalSource {
    Mcp,
    AiReview,
}

#[derive(Clone, Debug)]
pub struct ActionApprovalDetails {
    pub title: String,
    pub summary: String,
    pub target: Option<String>,
    pub details: Option<String>,
    pub destructive: bool,
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
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
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
    let cwd = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.connected {
            return Err(AppError::Command(
                "FileTerm SSH session is not connected".to_string(),
            ));
        }
        cwd.or_else(|| session.shell_cwd.clone())
    };

    let result = crate::commands::send_worker_cmd_with_response_timeout(
        app,
        &tab_id,
        Duration::from_millis(timeout_ms.saturating_add(5_000)),
        |respond_to| WorkerCmd::ExecuteRemoteCommand {
            command,
            cwd,
            timeout_ms,
            respond_to,
        },
    )
    .await?;
    parse_remote_exec_result(result)
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
    Ok(RemoteExecResult {
        output,
        exit_code,
        timed_out,
        output_truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_remote_exec_result, ActionApprovalDecision, ActionApprovalSource};
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
    }
}
