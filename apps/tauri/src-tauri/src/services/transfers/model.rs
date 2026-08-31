use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::sessions::WorkerCmd;
use crate::AppError;

const JOURNAL_VERSION: u8 = 1;
const JOURNAL_MAX_TASKS: usize = 200;
const UPDATE_INTERVAL: Duration = Duration::from_millis(200);
const SPEED_SAMPLE_INTERVAL: Duration = Duration::from_millis(120);
const TRANSFER_STOP_TIMEOUT: Duration = Duration::from_secs(15);
const TRANSFER_WORKER_SEND_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSFER_WORKER_CONTROL_TIMEOUT: Duration = Duration::from_secs(20);
// A data command may legitimately run for hours. Cancellation is the fast
// path; this ceiling still prevents a lost worker reply from hanging the
// transfer task forever when the connection never observes the token.
const TRANSFER_WORKER_DATA_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const MAX_REMOTE_TREE_DEPTH: usize = 64;
const MAX_REMOTE_TREE_ENTRIES: usize = 100_000;
const MAX_REMOTE_TREE_BYTES: u64 = 1 << 40;
const PARTIAL_SUFFIX: &str = ".fileterm-part";
// /tmp is frequently mounted as tmpfs on small Linux hosts. Keep new root
// upload staging on the disk-oriented temporary filesystem instead.
const ROOT_STAGING_PREFIX: &str = "/var/tmp/fileterm-root-upload-";
const LEGACY_ROOT_STAGING_PREFIX: &str = "/tmp/fileterm-root-upload-";

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferFileIdentity {
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferManifestEntry {
    pub relative_path: String,
    pub source_path: String,
    pub destination_path: String,
    pub partial_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staging_path: Option<String>,
    pub source_identity: TransferFileIdentity,
    pub status: String,
    pub transferred_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferManifest {
    pub version: u8,
    pub directories: Vec<String>,
    pub files: Vec<TransferManifestEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferTask {
    pub id: String,
    pub direction: String,
    pub name: String,
    pub progress: f64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transferred_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_access_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staging_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_identity: Option<TransferFileIdentity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<TransferManifest>,
    #[serde(default)]
    pub resumable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_attempt: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cleanup_pending: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
}

#[derive(Deserialize, Serialize)]
struct TransferJournal {
    version: u8,
    transfers: Vec<TransferTask>,
}

impl TransferTask {
    fn active(&self) -> bool {
        matches!(
            self.status.as_str(),
            "queued" | "running" | "verifying" | "finalizing"
        )
    }

    fn terminal(&self) -> bool {
        matches!(self.status.as_str(), "done" | "failed" | "canceled")
    }
}

/// Status a task transitions to when the application interrupts it (quit,
/// session lost, pane closed). Resumable tasks go to `paused` so the user
/// can resume from the partial file; non-resumable tasks are `canceled`.
fn interrupt_status(resumable: bool) -> &'static str {
    if resumable {
        "paused"
    } else {
        "canceled"
    }
}

/// Status a task transitions to when the transfer itself fails. Resumable
/// tasks go back to `paused` so the user can retry from the partial file;
/// non-resumable tasks land in the terminal `failed` state.
fn failure_status(resumable: bool) -> &'static str {
    if resumable {
        "paused"
    } else {
        "failed"
    }
}

/// Whether a task can be resumed from its current status. Only `paused`,
/// `interrupted`, and `failed` retain enough state to resume; terminal
/// statuses (`done`/`canceled`) and active statuses (`queued`/`running`/
/// `verifying`/`finalizing`) cannot be resumed.
fn can_resume_from(status: &str) -> bool {
    matches!(status, "paused" | "interrupted" | "failed")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn transfer_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
}

fn progress_event_due(previous: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    previous
        .map(|previous| now.saturating_duration_since(previous) >= UPDATE_INTERVAL)
        .unwrap_or(true)
}

fn task_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn join_remote_path(directory: &str, name: &str) -> String {
    let directory = directory.trim_end_matches('/');
    if directory.is_empty() || directory == "/" {
        format!("/{name}")
    } else {
        format!("{directory}/{name}")
    }
}

fn partial_path(path: &str) -> String {
    format!("{path}{PARTIAL_SUFFIX}")
}

fn root_staging_path(name: &str) -> String {
    let safe_name = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "{ROOT_STAGING_PREFIX}{}-{}.part",
        uuid::Uuid::new_v4(),
        safe_name
    )
}

pub(crate) fn is_root_upload_staging_path(path: &str) -> bool {
    path.starts_with(ROOT_STAGING_PREFIX) || path.starts_with(LEGACY_ROOT_STAGING_PREFIX)
}

fn normalize_root_upload_staging(task: &mut TransferTask) {
    if task.direction != "upload" || task.file_access_mode.as_deref() != Some("root") {
        return;
    }

    if let (Some(destination), Some(current_partial)) =
        (task.destination_path.as_deref(), task.partial_path.clone())
    {
        if task.staging_path.is_none() {
            task.staging_path = Some(if is_root_upload_staging_path(&current_partial) {
                current_partial
            } else {
                root_staging_path(&task.name)
            });
        }
        task.partial_path = Some(partial_path(destination));
    }

    if let Some(manifest) = task.manifest.as_mut() {
        for entry in &mut manifest.files {
            if entry.staging_path.is_none() {
                entry.staging_path = Some(if is_root_upload_staging_path(&entry.partial_path) {
                    entry.partial_path.clone()
                } else {
                    root_staging_path(&entry.relative_path)
                });
            }
            entry.partial_path = partial_path(&entry.destination_path);
        }
    }
}

fn journal_paths(app: &AppHandle) -> Result<(PathBuf, PathBuf, PathBuf), AppError> {
    let path = crate::storage::workspace_file(app, "transfer-journal.json")?;
    Ok((
        path.clone(),
        path.with_file_name("transfer-journal.json.tmp"),
        path.with_file_name("transfer-journal.json.bak"),
    ))
}

/// Pick the tasks that should survive the next journal write.
///
/// `state.transfers` is append-only, so `take(N)` from the front would keep the
/// oldest entries and silently drop the newly appended active/resumable tasks
/// once the limit is exceeded. Instead, sort by `updated_at` (falling back to
/// `created_at`, then the original append index for stability) and keep the
/// most recent `limit` entries, preserving the on-disk ordering for readability.
fn select_journal_tasks(tasks: &[TransferTask], limit: usize) -> Vec<TransferTask> {
    if tasks.len() <= limit {
        return tasks.to_vec();
    }
    let mut indexed: Vec<(usize, u64)> = tasks
        .iter()
        .enumerate()
        .map(|(idx, task)| (idx, task.updated_at.or(task.created_at).unwrap_or(0)))
        .collect();
    // Most recent first; ties broken by append order so the later-appended
    // task (higher index) is treated as newer and survives the cut.
    indexed.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    let mut kept: Vec<usize> = indexed
        .into_iter()
        .take(limit)
        .map(|(idx, _)| idx)
        .collect();
    kept.sort_unstable();
    kept.into_iter().map(|idx| tasks[idx].clone()).collect()
}

fn write_journal(app: &AppHandle, tasks: &[TransferTask]) -> Result<(), AppError> {
    let (path, temporary, backup) = journal_paths(app)?;
    let journal = TransferJournal {
        version: JOURNAL_VERSION,
        transfers: select_journal_tasks(tasks, JOURNAL_MAX_TASKS),
    };
    let json = serde_json::to_vec_pretty(&journal)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(&temporary, json).map_err(|error| AppError::Storage(error.to_string()))?;
    let _ = std::fs::remove_file(&backup);

    let moved_current = if path.exists() {
        std::fs::rename(&path, &backup).map_err(|error| AppError::Storage(error.to_string()))?;
        true
    } else {
        false
    };
    if let Err(error) = std::fs::rename(&temporary, &path) {
        if moved_current {
            let _ = std::fs::rename(&backup, &path);
        }
        return Err(AppError::Storage(error.to_string()));
    }
    let _ = std::fs::remove_file(&backup);
    Ok(())
}

fn read_journal(app: &AppHandle) -> Result<Vec<TransferTask>, AppError> {
    let (path, _temporary, backup) = journal_paths(app)?;
    for candidate in [path, backup] {
        let Ok(content) = std::fs::read_to_string(candidate) else {
            continue;
        };
        let Ok(mut journal) = serde_json::from_str::<TransferJournal>(&content) else {
            continue;
        };
        if journal.version != JOURNAL_VERSION {
            continue;
        }
        for task in &mut journal.transfers {
            normalize_root_upload_staging(task);
            if task.active() {
                task.status = interrupt_status(task.resumable).to_string();
                task.message = Some(if task.resumable {
                    "应用退出前传输未完成，可手动继续".to_string()
                } else {
                    "应用退出前传输未完成".to_string()
                });
                task.speed = None;
                task.updated_at = Some(now_ms());
            }
        }
        return Ok(journal.transfers);
    }
    Ok(Vec::new())
}
