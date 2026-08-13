//! Minimal, local-only audit trail for interactive remote exec tasks.
//!
//! This is deliberately not a terminal transcript or a command history. It
//! records only non-sensitive task metadata so a user can establish what was
//! requested and how it ended without retaining prompts, answers, or output.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::AppError;

const AUDIT_FILE: &str = "interactive-remote-exec-audit.jsonl";
const AUDIT_BACKUP_FILE: &str = "interactive-remote-exec-audit.jsonl.1";
const MAX_AUDIT_BYTES: u64 = 2 * 1024 * 1024;
static AUDIT_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractiveRemoteExecAuditSource {
    Mcp,
    Cli,
    /// Reserved for the desktop bridge itself. The MCP and CLI paths set the
    /// more specific source above; keeping this value makes direct command
    /// invocation auditable instead of silently bypassing the trail.
    Desktop,
}

#[derive(Clone, Debug)]
pub struct InteractiveRemoteExecAuditContext {
    pub id: String,
    pub source: InteractiveRemoteExecAuditSource,
    pub target: InteractiveRemoteExecAuditTarget,
    pub command_summary: String,
    pub command_sha256: String,
    interaction_count: Arc<AtomicU8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractiveRemoteExecAuditTarget {
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InteractiveRemoteExecAuditEvent {
    Started,
    InputRequested,
    Completed,
    Cancelled,
    TimedOut,
    TargetChanged,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InteractiveRemoteExecAuditResult {
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InteractiveRemoteExecAuditEntry<'a> {
    id: &'a str,
    at: u128,
    source: InteractiveRemoteExecAuditSource,
    event: InteractiveRemoteExecAuditEvent,
    target: &'a InteractiveRemoteExecAuditTarget,
    command_summary: &'a str,
    command_sha256: &'a str,
    interaction_count: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timed_out: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_truncated: Option<bool>,
}

impl InteractiveRemoteExecAuditContext {
    pub fn new(
        source: InteractiveRemoteExecAuditSource,
        target: InteractiveRemoteExecAuditTarget,
        command: &str,
    ) -> Self {
        Self {
            id: format!("interactive-remote-exec-{}", uuid::Uuid::new_v4()),
            source,
            target,
            command_summary: command_summary(command),
            command_sha256: format!("{:x}", Sha256::digest(command.as_bytes())),
            interaction_count: Arc::new(AtomicU8::new(0)),
        }
    }

    /// Keep only the number of requested local input rounds. The answer and
    /// remote prompt intentionally never enter the audit data structure.
    pub fn note_interaction(&self, attempt: u8) {
        self.interaction_count.store(attempt, Ordering::Relaxed);
    }

    pub fn interaction_count(&self) -> u8 {
        self.interaction_count.load(Ordering::Relaxed)
    }
}

/// Append one bounded JSON line. The caller never receives the command,
/// prompt, answer, error text, or remote output back from this module.
pub async fn record(
    app: &AppHandle,
    context: &InteractiveRemoteExecAuditContext,
    event: InteractiveRemoteExecAuditEvent,
    interaction_count: u8,
    result: Option<InteractiveRemoteExecAuditResult>,
) -> Result<(), AppError> {
    let entry = InteractiveRemoteExecAuditEntry {
        id: &context.id,
        at: now_millis(),
        source: context.source,
        event,
        target: &context.target,
        command_summary: &context.command_summary,
        command_sha256: &context.command_sha256,
        interaction_count,
        exit_code: result.and_then(|result| result.exit_code),
        timed_out: result.map(|result| result.timed_out),
        output_truncated: result.map(|result| result.output_truncated),
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| AppError::Serialization(error.to_string()))?
        + "\n";
    let path = crate::storage::workspace_file(app, AUDIT_FILE)?;
    tokio::task::spawn_blocking(move || append_audit_entry(&path, &line))
        .await
        .map_err(|error| {
            AppError::Storage(format!("Interactive exec audit task failed: {error}"))
        })?
}

fn append_audit_entry(path: &Path, line: &str) -> Result<(), AppError> {
    let _guard = AUDIT_LOCK
        .lock()
        .map_err(|_| AppError::Storage("Interactive exec audit lock poisoned".to_string()))?;
    let directory = path.parent().ok_or_else(|| {
        AppError::Storage("Interactive exec audit path has no parent directory".to_string())
    })?;
    fs::create_dir_all(directory).map_err(|error| AppError::Storage(error.to_string()))?;

    if fs::metadata(path)
        .map(|metadata| metadata.len() >= MAX_AUDIT_BYTES)
        .unwrap_or(false)
    {
        let backup = directory.join(AUDIT_BACKUP_FILE);
        let _ = fs::remove_file(&backup);
        fs::rename(path, backup).map_err(|error| AppError::Storage(error.to_string()))?;
    }

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // `OpenOptionsExt::mode` only applies at file creation. Reassert the
        // owner-only mode for an existing audit file as well, including one
        // restored from a backup or created by an earlier app version.
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    file.write_all(line.as_bytes())
        .and_then(|_| file.sync_data())
        .map_err(|error| AppError::Storage(error.to_string()))
}

fn command_summary(command: &str) -> String {
    let normalized = command.trim();
    let program = normalized
        .split_whitespace()
        .find(|token| !(token.contains('=') && !token.starts_with('=')))
        .filter(|token| {
            !token.is_empty()
                && token.len() <= 128
                && token.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "/._+-".contains(character)
                })
        });
    match program {
        Some(program) => format!("interactive SSH exec: {program}"),
        None => "interactive SSH exec".to_string(),
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::{
        command_summary, InteractiveRemoteExecAuditContext, InteractiveRemoteExecAuditEntry,
        InteractiveRemoteExecAuditEvent, InteractiveRemoteExecAuditSource,
        InteractiveRemoteExecAuditTarget,
    };

    #[test]
    fn command_summary_keeps_only_the_program_name() {
        assert_eq!(
            command_summary("TOKEN=do-not-store sudo docker info --password=still-not-stored"),
            "interactive SSH exec: sudo"
        );
        assert_eq!(command_summary("'secret command'"), "interactive SSH exec");
    }

    #[test]
    fn audit_entry_never_serializes_the_original_command_or_secret() {
        let command = "sudo --password=hunter2 docker info";
        let context = InteractiveRemoteExecAuditContext::new(
            InteractiveRemoteExecAuditSource::Mcp,
            InteractiveRemoteExecAuditTarget {
                host: "example.test".to_string(),
                shell_user: Some("admin".to_string()),
                cwd: Some("/srv/app".to_string()),
            },
            command,
        );
        let entry = InteractiveRemoteExecAuditEntry {
            id: &context.id,
            at: 1,
            source: context.source,
            event: InteractiveRemoteExecAuditEvent::Started,
            target: &context.target,
            command_summary: &context.command_summary,
            command_sha256: &context.command_sha256,
            interaction_count: 0,
            exit_code: None,
            timed_out: None,
            output_truncated: None,
        };
        let encoded = serde_json::to_string(&entry).expect("audit entry serializes");
        assert!(!encoded.contains(command));
        assert!(!encoded.contains("hunter2"));
        assert!(encoded.contains("interactive SSH exec: sudo"));
        assert!(encoded.contains("commandSha256"));
    }

    #[test]
    fn interaction_count_is_shared_without_retaining_input_values() {
        let context = InteractiveRemoteExecAuditContext::new(
            InteractiveRemoteExecAuditSource::Cli,
            InteractiveRemoteExecAuditTarget {
                host: "example.test".to_string(),
                shell_user: None,
                cwd: None,
            },
            "sudo id",
        );
        let task_context = context.clone();
        task_context.note_interaction(2);

        assert_eq!(context.interaction_count(), 2);
        assert!(!format!("{task_context:?}").contains("password"));
    }
}
