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
use tokio_util::sync::CancellationToken;

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
pub const DEFAULT_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
pub const MAX_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS: u64 = 6 * 60 * 60 * 1_000;
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
/// Saving a newly supplied sudo/su password requires a completed synchronous
/// command so FileTerm can verify that authentication succeeded first.
pub const BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED: &str =
    "BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED";
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
/// A Copilot terminal handoff must consume a still-pending approval exactly
/// once. This is returned when the renderer races an expiry, cancellation, or
/// another approval resolution.
pub const AI_TERMINAL_HANDOFF_NOT_PENDING: &str = "AI_TERMINAL_HANDOFF_NOT_PENDING";
/// Stable cancellation code shared by the Copilot prompt and exec boundary.
pub const AI_REQUEST_CANCELLED: &str = "AI_REQUEST_CANCELLED";

/// Optional progress callback fired after FileTerm has restored the main
/// window and before the local sudo/su prompt starts waiting. AI Copilot and
/// the MCP/CLI bridge use it to tell their caller that the foreground prompt
/// is ready for the user.
pub type PrivilegedPromptNotice = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionApprovalSource {
    Cli,
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

#[derive(Clone, Debug)]
pub struct ActionApprovalTargetBinding {
    pub tab_id: String,
    pub session_revision: String,
    pub command: String,
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

include!("approval.rs");
include!("remote_exec.rs");
include!("privileged_exec.rs");
include!("validation.rs");
include!("tests.rs");
