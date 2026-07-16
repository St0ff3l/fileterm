use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::sessions::WorkerCmd;
use crate::AppError;

const JOURNAL_VERSION: u8 = 1;
const UPDATE_INTERVAL: Duration = Duration::from_millis(200);
const PARTIAL_SUFFIX: &str = ".fileterm-part";

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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn transfer_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
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
        "/tmp/fileterm-root-upload-{}-{}.part",
        uuid::Uuid::new_v4(),
        safe_name
    )
}

fn normalize_root_upload_staging(task: &mut TransferTask) {
    if task.direction != "upload" || task.file_access_mode.as_deref() != Some("root") {
        return;
    }

    if let (Some(destination), Some(current_partial)) =
        (task.destination_path.as_deref(), task.partial_path.clone())
    {
        if task.staging_path.is_none() {
            task.staging_path = Some(if current_partial.starts_with("/tmp/fileterm-root-upload-") {
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
                entry.staging_path = Some(
                    if entry.partial_path.starts_with("/tmp/fileterm-root-upload-") {
                        entry.partial_path.clone()
                    } else {
                        root_staging_path(&entry.relative_path)
                    },
                );
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

fn write_journal(app: &AppHandle, tasks: &[TransferTask]) -> Result<(), AppError> {
    let (path, temporary, backup) = journal_paths(app)?;
    let journal = TransferJournal {
        version: JOURNAL_VERSION,
        transfers: tasks.iter().take(200).cloned().collect(),
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
                task.status = if task.resumable { "paused" } else { "canceled" }.to_string();
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

pub async fn ensure_loaded(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut loaded = state.transfer_journal_loaded.lock().await;
    if *loaded {
        return Ok(());
    }
    let tasks = read_journal(app)?;
    *state.transfers.write().await = tasks.clone();
    *loaded = true;
    drop(loaded);
    write_journal(app, &tasks)
}

pub async fn list(app: &AppHandle) -> Result<Vec<TransferTask>, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let transfers = state.transfers.read().await.clone();
    Ok(transfers)
}

async fn persist(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tasks = state.transfers.read().await.clone();
    write_journal(app, &tasks)
}

async fn emit_task(app: &AppHandle, task: TransferTask, snapshot: bool) {
    let _ = app.emit("transfer:update", task);
    if snapshot {
        if let Ok(workspace) = crate::commands::get_workspace_snapshot(app.clone()).await {
            let _ = app.emit("workspace:snapshot", workspace);
        }
    }
}

async fn patch_task(
    app: &AppHandle,
    transfer_id: &str,
    patch: impl FnOnce(&mut TransferTask),
    immediate: bool,
) -> Result<Option<TransferTask>, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let task = {
        let mut tasks = state.transfers.write().await;
        let Some(task) = tasks.iter_mut().find(|task| task.id == transfer_id) else {
            return Ok(None);
        };
        patch(task);
        task.updated_at = Some(now_ms());
        task.clone()
    };
    if immediate {
        persist(app).await?;
    }
    emit_task(app, task.clone(), immediate).await;
    Ok(Some(task))
}

pub async fn report_progress(app: &AppHandle, transfer_id: &str, transferred: u64, total: u64) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let should_emit = {
        let mut last_events = state.transfer_last_event.lock().await;
        let now = std::time::Instant::now();
        let should_emit = last_events
            .get(transfer_id)
            .map(|last| now.duration_since(*last) >= UPDATE_INTERVAL)
            .unwrap_or(true);
        if should_emit {
            last_events.insert(transfer_id.to_string(), now);
        }
        should_emit
    };
    let _ = patch_task(
        app,
        transfer_id,
        |task| {
            let previous_bytes = task.transferred_bytes.unwrap_or(0);
            let previous_updated = task.updated_at.unwrap_or_else(now_ms);
            let now = now_ms();
            let (aggregate_transferred, aggregate_total) =
                if let Some(manifest) = task.manifest.as_mut() {
                    if let Some(entry) = manifest
                        .files
                        .iter_mut()
                        .find(|entry| entry.status == "running")
                    {
                        entry.transferred_bytes = transferred.min(entry.source_identity.size);
                    }
                    manifest_totals(manifest)
                } else {
                    (transferred, total)
                };
            task.status = "running".to_string();
            task.transferred_bytes = Some(aggregate_transferred);
            task.total_bytes = Some(aggregate_total);
            task.progress = if aggregate_total == 0 {
                99.0
            } else {
                ((aggregate_transferred as f64 / aggregate_total as f64) * 100.0).min(99.0)
            };
            if task.manifest.is_none() {
                task.message = Some(
                    task.partial_path
                        .clone()
                        .unwrap_or_else(|| task.name.clone()),
                );
            }
            if now.saturating_sub(previous_updated) >= 120
                && aggregate_transferred >= previous_bytes
            {
                task.speed = format_transfer_speed(
                    (aggregate_transferred - previous_bytes) as f64
                        / ((now.saturating_sub(previous_updated) as f64) / 1000.0),
                );
            }
            task.resumable = true;
        },
        should_emit,
    )
    .await;
}

async fn worker_call<T>(
    app: &AppHandle,
    tab_id: &str,
    make_command: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = state
        .workers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("传输会话未连接"))?;
    let (respond_to, result) = oneshot::channel();
    sender
        .send(make_command(respond_to))
        .await
        .map_err(|_| transfer_error("传输会话已关闭"))?;
    result
        .await
        .map_err(|_| transfer_error("传输会话未返回结果"))?
        .map_err(transfer_error)
}

#[derive(Debug, PartialEq, Eq)]
struct RemoteUploadPlan {
    upload_path: String,
    resume_offset: u64,
    upload_needed: bool,
    partial_ready: bool,
}

async fn stat_remote_transfer_size(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
) -> Result<Option<u64>, AppError> {
    worker_call(app, tab_id, |respond_to| WorkerCmd::StatRemoteFile {
        path: path.to_string(),
        respond_to,
    })
    .await
    .map(|stat| stat.map(|value| value.size))
}

async fn remove_remote_transfer_file(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
) -> Result<(), AppError> {
    worker_call(app, tab_id, |respond_to| WorkerCmd::RemoveRemoteFile {
        path: path.to_string(),
        respond_to,
    })
    .await
}

async fn stat_remote_upload_progress(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
) -> Option<u64> {
    if let Some(staging_path) = staging_path {
        if let Some(size) = stat_remote_transfer_size(app, tab_id, staging_path)
            .await
            .ok()
            .flatten()
        {
            return Some(size);
        }
    }
    stat_remote_transfer_size(app, tab_id, partial_path)
        .await
        .ok()
        .flatten()
}

async fn remove_remote_upload_artifacts(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
) -> Result<(), AppError> {
    remove_remote_transfer_file(app, tab_id, partial_path).await?;
    if let Some(staging_path) = staging_path {
        remove_remote_transfer_file(app, tab_id, staging_path).await?;
    }
    Ok(())
}

async fn prepare_remote_upload(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
    source_size: u64,
) -> Result<RemoteUploadPlan, AppError> {
    if let Some(staging_path) = staging_path {
        let partial_size = stat_remote_transfer_size(app, tab_id, partial_path).await?;
        if partial_size == Some(source_size) {
            return Ok(RemoteUploadPlan {
                upload_path: staging_path.to_string(),
                resume_offset: source_size,
                upload_needed: false,
                partial_ready: true,
            });
        }
        if partial_size.is_some() {
            remove_remote_transfer_file(app, tab_id, partial_path).await?;
        }

        let staging_size = stat_remote_transfer_size(app, tab_id, staging_path).await?;
        let resume_offset = staging_size.unwrap_or(0);
        if resume_offset > source_size {
            return Err(transfer_error(
                "root staging 大于源文件，请丢弃断点后重新传输",
            ));
        }
        return Ok(RemoteUploadPlan {
            upload_path: staging_path.to_string(),
            resume_offset,
            upload_needed: staging_size != Some(source_size),
            partial_ready: false,
        });
    }

    let partial_size = stat_remote_transfer_size(app, tab_id, partial_path).await?;
    let resume_offset = partial_size.unwrap_or(0);
    if resume_offset > source_size {
        return Err(transfer_error(
            "断点文件大于源文件，请丢弃断点后重新传输",
        ));
    }
    Ok(RemoteUploadPlan {
        upload_path: partial_path.to_string(),
        resume_offset,
        upload_needed: partial_size != Some(source_size),
        partial_ready: partial_size == Some(source_size),
    })
}

async fn finalize_remote_upload(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
    destination_path: &str,
    source_size: u64,
    partial_ready: bool,
) -> Result<(), AppError> {
    if let Some(staging_path) = staging_path {
        if !partial_ready {
            worker_call(app, tab_id, |respond_to| WorkerCmd::CommitRemoteStaging {
                staging_path: staging_path.to_string(),
                partial_path: partial_path.to_string(),
                respond_to,
            })
            .await?;
        }
        let committed_size = stat_remote_transfer_size(app, tab_id, partial_path)
            .await?
            .unwrap_or(0);
        if committed_size != source_size {
            return Err(transfer_error(format!(
                "root 目标目录断点校验失败：{committed_size} bytes，期望 {source_size}"
            )));
        }
    }

    worker_call(app, tab_id, |respond_to| WorkerCmd::ReplaceRemoteFile {
        partial_path: partial_path.to_string(),
        destination_path: destination_path.to_string(),
        respond_to,
    })
    .await
}

async fn find_connected_tab(app: &AppHandle, profile_id: &str) -> Option<String> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tabs = state.tabs.read().await.clone();
    let sessions = state.sessions.read().await;
    tabs.into_iter().find_map(|tab| {
        (tab.profile_id == profile_id
            && matches!(tab.session_type.as_str(), "ssh" | "ftp")
            && sessions
                .get(&tab.id)
                .map(|session| session.connected)
                .unwrap_or(false))
        .then_some(tab.id)
    })
}

async fn task_for(app: &AppHandle, transfer_id: &str) -> Result<TransferTask, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let task = state
        .transfers
        .read()
        .await
        .iter()
        .find(|task| task.id == transfer_id)
        .cloned()
        .ok_or_else(|| transfer_error("传输任务不存在"));
    task
}

async fn ensure_remote_directory(
    app: &AppHandle,
    tab_id: &str,
    directory: &str,
) -> Result<(), AppError> {
    let normalized = directory.trim_end_matches('/');
    if normalized.is_empty() || normalized == "/" {
        return Ok(());
    }
    let parent = parent_remote_path(normalized);
    let name = normalized
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| transfer_error("远端目录无效"))?
        .to_string();
    worker_call(app, tab_id, |respond_to| WorkerCmd::CreateRemoteDirectory {
        parent_path: parent,
        name,
        respond_to,
    })
    .await
}

fn parent_remote_path(path: &str) -> String {
    let normalized = path.trim_end_matches('/');
    match normalized.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => normalized[..index].to_string(),
    }
}

async fn collect_local_tree(
    root: &Path,
) -> Result<(Vec<PathBuf>, Vec<(PathBuf, TransferFileIdentity)>), AppError> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = tokio::fs::read_dir(&directory).await.map_err(|error| {
            transfer_error(format!("无法读取本地目录 {}: {error}", directory.display()))
        })?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| transfer_error(format!("无法读取本地目录项: {error}")))?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|error| transfer_error(format!("无法读取本地文件类型: {error}")))?;
            if file_type.is_dir() {
                directories.push(path.clone());
                pending.push(path);
            } else if file_type.is_file() {
                let metadata = entry
                    .metadata()
                    .await
                    .map_err(|error| transfer_error(format!("无法读取本地文件信息: {error}")))?;
                files.push((
                    path,
                    TransferFileIdentity {
                        size: metadata.len(),
                        modified_at: metadata
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                            .map(|value| value.as_millis() as u64),
                    },
                ));
            }
        }
    }
    directories.sort();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok((directories, files))
}

async fn collect_remote_tree(
    app: &AppHandle,
    tab_id: &str,
    root: &str,
) -> Result<(Vec<String>, Vec<(String, TransferFileIdentity)>), AppError> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut pending = vec![root.to_string()];
    while let Some(directory) = pending.pop() {
        let entries = worker_call(app, tab_id, |respond_to| WorkerCmd::ListRemoteFiles {
            path: directory.clone(),
            respond_to,
        })
        .await?;
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if name == ".." || name.is_empty() {
                continue;
            }
            let path = entry
                .get("path")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| transfer_error("远端目录返回了无效路径"))?
                .to_string();
            if entry.get("type").and_then(|value| value.as_str()) == Some("folder") {
                directories.push(path.clone());
                pending.push(path);
                continue;
            }
            let identity = worker_call(app, tab_id, |respond_to| WorkerCmd::StatRemoteFile {
                path: path.clone(),
                respond_to,
            })
            .await?
            .ok_or_else(|| transfer_error(format!("无法读取远端文件信息: {path}")))?;
            files.push((
                path,
                TransferFileIdentity {
                    size: identity.size,
                    modified_at: identity.modified_at,
                },
            ));
        }
    }
    directories.sort();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok((directories, files))
}

fn relative_remote_path(root: &str, path: &str) -> Result<String, AppError> {
    let root = root.trim_end_matches('/');
    path.strip_prefix(root)
        .and_then(|value| value.strip_prefix('/'))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| transfer_error(format!("路径 {path} 不在根目录 {root} 内")))
}

fn manifest_totals(manifest: &TransferManifest) -> (u64, u64) {
    let total = manifest
        .files
        .iter()
        .map(|entry| entry.source_identity.size)
        .sum();
    let transferred = manifest
        .files
        .iter()
        .map(|entry| {
            if entry.status == "done" {
                entry.source_identity.size
            } else {
                entry.transferred_bytes
            }
        })
        .sum();
    (transferred, total)
}

fn format_transfer_speed(bytes_per_second: f64) -> Option<String> {
    if !bytes_per_second.is_finite() || bytes_per_second <= 0.0 {
        return None;
    }
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes_per_second;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let precision = if value >= 100.0 {
        0
    } else if value >= 10.0 {
        1
    } else {
        2
    };
    Some(format!("{value:.precision$} {}", UNITS[unit]))
}

pub async fn queue_upload(app: &AppHandle, _file_names: Vec<String>) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    // The following `upload_file` invocations create durable tasks with source,
    // destination and resume metadata.  Do not create anonymous placeholders:
    // they cannot be resumed or canceled and would otherwise remain forever.
    Ok(())
}

pub async fn create_upload(
    app: &AppHandle,
    tab_id: String,
    local_path: String,
    remote_directory: String,
    target_name: Option<String>,
) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let metadata = tokio::fs::metadata(&local_path)
        .await
        .map_err(|error| transfer_error(format!("无法读取本地上传文件: {error}")))?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tab = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("目标标签页不存在"))?;
    let name = target_name.unwrap_or_else(|| task_name(&local_path));
    let destination_path = join_remote_path(&remote_directory, &name);
    let file_access_mode = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| session.file_access_mode.clone())
        .unwrap_or_else(|| "user".to_string());
    if metadata.is_dir() {
        let (directories, files) = collect_local_tree(Path::new(&local_path)).await?;
        let task_id = format!("transfer-{}", uuid::Uuid::new_v4());
        let mut manifest_directories = vec![destination_path.clone()];
        manifest_directories.extend(directories.into_iter().map(|directory| {
            let relative = directory
                .strip_prefix(&local_path)
                .unwrap_or(&directory)
                .to_string_lossy()
                .replace('\\', "/");
            join_remote_path(&destination_path, &relative)
        }));
        let manifest_files = files
            .into_iter()
            .map(|(source, source_identity)| {
                let relative_path = source
                    .strip_prefix(&local_path)
                    .unwrap_or(&source)
                    .to_string_lossy()
                    .replace('\\', "/");
                let entry_destination = join_remote_path(&destination_path, &relative_path);
                let entry_partial = partial_path(&entry_destination);
                let entry_staging = (file_access_mode == "root")
                    .then(|| root_staging_path(&relative_path));
                TransferManifestEntry {
                    relative_path,
                    source_path: source.to_string_lossy().into_owned(),
                    destination_path: entry_destination,
                    partial_path: entry_partial,
                    staging_path: entry_staging,
                    source_identity,
                    status: "pending".to_string(),
                    transferred_bytes: 0,
                }
            })
            .collect::<Vec<_>>();
        let manifest = TransferManifest {
            version: 1,
            directories: manifest_directories,
            files: manifest_files,
        };
        let (_, total) = manifest_totals(&manifest);
        let now = now_ms();
        let task = TransferTask {
            id: task_id,
            direction: "upload".to_string(),
            name,
            progress: 0.0,
            status: "queued".to_string(),
            message: Some("等待上传目录".to_string()),
            speed: None,
            transferred_bytes: Some(0),
            total_bytes: Some(total),
            tab_id: Some(tab_id),
            profile_id: Some(tab.profile_id),
            session_type: Some(tab.session_type),
            file_access_mode: Some(file_access_mode),
            target_type: Some("folder".to_string()),
            source_path: Some(local_path),
            destination_path: Some(destination_path),
            partial_path: None,
            staging_path: None,
            source_identity: None,
            manifest: Some(manifest),
            resumable: true,
            created_at: Some(now),
            updated_at: Some(now),
        };
        state.transfers.write().await.push(task.clone());
        persist(app).await?;
        emit_task(app, task.clone(), true).await;
        start(app.clone(), task.id);
        return Ok(());
    }
    if !metadata.is_file() {
        return Err(transfer_error("仅支持上传普通文件或目录"));
    }
    let partial = partial_path(&destination_path);
    let staging = (file_access_mode == "root").then(|| root_staging_path(&name));
    let now = now_ms();
    let task = TransferTask {
        id: format!("transfer-{}", uuid::Uuid::new_v4()),
        direction: "upload".to_string(),
        name,
        progress: 0.0,
        status: "queued".to_string(),
        message: Some("等待上传".to_string()),
        speed: None,
        transferred_bytes: Some(0),
        total_bytes: Some(metadata.len()),
        tab_id: Some(tab_id.clone()),
        profile_id: Some(tab.profile_id),
        session_type: Some(tab.session_type),
        file_access_mode: Some(file_access_mode),
        target_type: Some("file".to_string()),
        source_path: Some(local_path),
        partial_path: Some(partial),
        staging_path: staging,
        destination_path: Some(destination_path),
        source_identity: Some(TransferFileIdentity {
            size: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis() as u64),
        }),
        manifest: None,
        resumable: true,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone(), true).await;
    start(app.clone(), task.id);
    Ok(())
}

pub async fn create_download(
    app: &AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    target_name: Option<String>,
) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tab = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("目标标签页不存在"))?;
    let size = worker_call(app, &tab_id, |respond_to| WorkerCmd::StatRemoteFile {
        path: remote_path.clone(),
        respond_to,
    })
    .await?
    .ok_or_else(|| transfer_error("远端下载文件不存在"))?;
    let name = target_name.unwrap_or_else(|| task_name(&remote_path));
    let destination_path = Path::new(&local_directory)
        .join(&name)
        .to_string_lossy()
        .into_owned();
    let now = now_ms();
    let task = TransferTask {
        id: format!("transfer-{}", uuid::Uuid::new_v4()),
        direction: "download".to_string(),
        name,
        progress: 0.0,
        status: "queued".to_string(),
        message: Some("等待下载".to_string()),
        speed: None,
        transferred_bytes: Some(0),
        total_bytes: Some(size.size),
        tab_id: Some(tab_id.clone()),
        profile_id: Some(tab.profile_id),
        session_type: Some(tab.session_type),
        file_access_mode: state
            .sessions
            .read()
            .await
            .get(&tab_id)
            .map(|session| session.file_access_mode.clone()),
        target_type: Some("file".to_string()),
        source_path: Some(remote_path),
        partial_path: Some(partial_path(&destination_path)),
        staging_path: None,
        destination_path: Some(destination_path),
        source_identity: Some(TransferFileIdentity {
            size: size.size,
            modified_at: size.modified_at,
        }),
        manifest: None,
        resumable: true,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone(), true).await;
    start(app.clone(), task.id);
    Ok(())
}

pub async fn create_download_directory(
    app: &AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    target_name: Option<String>,
) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tab = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("目标标签页不存在"))?;
    let name = target_name.unwrap_or_else(|| task_name(&remote_path));
    let destination_root = Path::new(&local_directory).join(&name);
    let file_access_mode = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| session.file_access_mode.clone());
    let (directories, files) = collect_remote_tree(app, &tab_id, &remote_path).await?;
    let mut manifest_directories = vec![destination_root.to_string_lossy().into_owned()];
    for directory in directories {
        let relative = relative_remote_path(&remote_path, &directory)?;
        manifest_directories.push(
            destination_root
                .join(relative)
                .to_string_lossy()
                .into_owned(),
        );
    }
    let manifest_files = files
        .into_iter()
        .map(|(source_path, source_identity)| {
            let relative_path = relative_remote_path(&remote_path, &source_path)?;
            let destination_path = destination_root
                .join(&relative_path)
                .to_string_lossy()
                .into_owned();
            Ok(TransferManifestEntry {
                relative_path,
                source_path,
                partial_path: partial_path(&destination_path),
                staging_path: None,
                destination_path,
                source_identity,
                status: "pending".to_string(),
                transferred_bytes: 0,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let manifest = TransferManifest {
        version: 1,
        directories: manifest_directories,
        files: manifest_files,
    };
    let (_, total) = manifest_totals(&manifest);
    let now = now_ms();
    let task = TransferTask {
        id: format!("transfer-{}", uuid::Uuid::new_v4()),
        direction: "download".to_string(),
        name,
        progress: 0.0,
        status: "queued".to_string(),
        message: Some("等待下载目录".to_string()),
        speed: None,
        transferred_bytes: Some(0),
        total_bytes: Some(total),
        tab_id: Some(tab_id),
        profile_id: Some(tab.profile_id),
        session_type: Some(tab.session_type),
        file_access_mode,
        target_type: Some("folder".to_string()),
        source_path: Some(remote_path),
        destination_path: Some(destination_root.to_string_lossy().into_owned()),
        partial_path: None,
        staging_path: None,
        source_identity: None,
        manifest: Some(manifest),
        resumable: true,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone(), true).await;
    start(app.clone(), task.id);
    Ok(())
}

fn start(app: AppHandle, transfer_id: String) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run(app.clone(), transfer_id.clone()).await {
            let _ = fail_if_running(&app, &transfer_id, error.to_string()).await;
        }
    });
}

async fn run(app: AppHandle, transfer_id: String) -> Result<(), AppError> {
    let mut task = task_for(&app, &transfer_id).await?;
    if task.terminal() || task.status == "paused" {
        return Ok(());
    }
    crate::services::logging::info(
        &app,
        &format!("transfer:{transfer_id}"),
        format!(
            "starting direction={} target_type={} name={} total_bytes={}",
            task.direction,
            task.target_type.as_deref().unwrap_or("file"),
            task.name,
            task.total_bytes.unwrap_or(0)
        ),
    );
    let resume_requested = task.message.as_deref() == Some("等待继续传输");
    let profile_id = task
        .profile_id
        .clone()
        .ok_or_else(|| transfer_error("传输任务缺少连接信息"))?;
    let tab_id = match task.tab_id.clone() {
        Some(tab_id) => tab_id,
        None => find_connected_tab(&app, &profile_id)
            .await
            .ok_or_else(|| transfer_error("请先连接原传输使用的连接"))?,
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let (connected, current_file_access_mode) = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| (session.connected, session.file_access_mode.clone()))
        .unwrap_or_else(|| (false, "user".to_string()));
    if !connected {
        return Err(transfer_error("连接已断开，可在重连后继续传输"));
    }
    if task
        .file_access_mode
        .as_deref()
        .is_some_and(|expected| expected != current_file_access_mode)
    {
        return Err(transfer_error(
            "文件访问权限模式已变化，请切换回创建任务时的视图后再传输",
        ));
    }
    let cancel = CancellationToken::new();
    state
        .transfer_controls
        .write()
        .await
        .insert(transfer_id.clone(), cancel.clone());
    task = patch_task(
        &app,
        &transfer_id,
        |task| {
            task.tab_id = Some(tab_id.clone());
            task.status = "running".to_string();
            task.message = Some("正在检查断点...".to_string());
            task.speed = None;
        },
        true,
    )
    .await?
    .ok_or_else(|| transfer_error("传输任务不存在"))?;

    let result = if task.target_type.as_deref() == Some("folder") {
        run_directory_transfer(
            &app,
            &transfer_id,
            &tab_id,
            &task,
            cancel.clone(),
            resume_requested,
        )
        .await
    } else {
        async {
            let source_path = task
                .source_path
                .clone()
                .ok_or_else(|| transfer_error("传输任务缺少源路径"))?;
            let destination_path = task
                .destination_path
                .clone()
                .ok_or_else(|| transfer_error("传输任务缺少目标路径"))?;
            let partial = task
                .partial_path
                .clone()
                .ok_or_else(|| transfer_error("传输任务缺少断点路径"))?;
            let staging = task.staging_path.clone();
            let source_size = if task.direction == "upload" {
                let metadata = tokio::fs::metadata(&source_path)
                    .await
                    .map_err(|_| transfer_error("上传源文件不存在或无法读取"))?;
                if !metadata.is_file() {
                    return Err(transfer_error("上传源不是普通文件"));
                }
                let identity = TransferFileIdentity {
                    size: metadata.len(),
                    modified_at: metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                        .map(|value| value.as_millis() as u64),
                };
                if resume_requested
                    && task
                        .source_identity
                        .as_ref()
                        .is_some_and(|expected| !same_transfer_identity(&identity, expected))
                {
                    return Err(transfer_error(
                        "上传源文件已发生变化，不能继续旧断点；请丢弃后重新传输",
                    ));
                }
                identity.size
            } else {
                let source = worker_call(&app, &tab_id, |respond_to| WorkerCmd::StatRemoteFile {
                    path: source_path.clone(),
                    respond_to,
                })
                .await?
                .ok_or_else(|| transfer_error("下载源文件不存在或无法读取"))?;
                let identity = TransferFileIdentity {
                    size: source.size,
                    modified_at: source.modified_at,
                };
                if resume_requested
                    && task
                        .source_identity
                        .as_ref()
                        .is_some_and(|expected| !same_transfer_identity(&identity, expected))
                {
                    return Err(transfer_error(
                        "下载源文件已发生变化，不能继续旧断点；请丢弃后重新传输",
                    ));
                }
                identity.size
            };
            if !resume_requested {
                if task.direction == "upload" {
                    remove_remote_upload_artifacts(
                        &app,
                        &tab_id,
                        &partial,
                        staging.as_deref(),
                    )
                    .await?;
                } else {
                    let _ = tokio::fs::remove_file(&partial).await;
                }
            }
            let upload_plan = if task.direction == "upload" {
                Some(
                    prepare_remote_upload(
                        &app,
                        &tab_id,
                        &partial,
                        staging.as_deref(),
                        source_size,
                    )
                    .await?,
                )
            } else {
                None
            };
            let offset = if let Some(plan) = upload_plan.as_ref() {
                plan.resume_offset
            } else {
                tokio::fs::metadata(&partial)
                    .await
                    .map(|metadata| metadata.len())
                    .unwrap_or(0)
            };
            if offset > source_size {
                return Err(transfer_error("断点文件大于源文件，请丢弃断点后重新传输"));
            }
            patch_task(
                &app,
                &transfer_id,
                |task| {
                    task.transferred_bytes = Some(offset);
                    task.total_bytes = Some(source_size);
                    task.progress = if source_size == 0 {
                        0.0
                    } else {
                        ((offset as f64 / source_size as f64) * 100.0).min(99.0)
                    };
                    task.message = Some(if offset > 0 {
                        format!("从 {offset} bytes 继续")
                    } else {
                        "正在传输".to_string()
                    });
                    task.resumable = true;
                },
                true,
            )
            .await?;
            if task.direction == "upload" {
                let plan = upload_plan.as_ref().expect("upload plan exists");
                if plan.upload_needed {
                    worker_call(&app, &tab_id, |respond_to| WorkerCmd::UploadLocalFile {
                        local_path: source_path,
                        remote_path: plan.upload_path.clone(),
                        resume_offset: offset,
                        transfer_id: transfer_id.clone(),
                        cancel: cancel.clone(),
                        respond_to,
                    })
                    .await?;
                }
            } else {
                if let Some(parent) = Path::new(&partial).parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|error| transfer_error(error.to_string()))?;
                }
                worker_call(&app, &tab_id, |respond_to| WorkerCmd::DownloadRemoteFile {
                    remote_path: source_path,
                    local_path: partial.clone(),
                    resume_offset: offset,
                    transfer_id: transfer_id.clone(),
                    cancel: cancel.clone(),
                    respond_to,
                })
                .await?;
            }
            let stopped = task_for(&app, &transfer_id).await?;
            if matches!(stopped.status.as_str(), "paused" | "canceled") || cancel.is_cancelled() {
                return Ok(());
            }
            patch_task(
                &app,
                &transfer_id,
                |task| {
                    task.status = "verifying".to_string();
                    task.message = Some("正在校验文件大小...".to_string());
                    task.speed = None;
                },
                true,
            )
            .await?;
            let completed_size = if task.direction == "upload" {
                let plan = upload_plan.as_ref().expect("upload plan exists");
                if plan.partial_ready {
                    source_size
                } else {
                    stat_remote_transfer_size(&app, &tab_id, &plan.upload_path)
                        .await?
                        .unwrap_or(0)
                }
            } else {
                tokio::fs::metadata(&partial)
                    .await
                    .map_err(|error| transfer_error(error.to_string()))?
                    .len()
            };
            if completed_size != source_size {
                return Err(transfer_error(format!(
                    "传输校验失败：断点文件大小为 {completed_size}，期望 {source_size}"
                )));
            }
            patch_task(
                &app,
                &transfer_id,
                |task| {
                    task.status = "finalizing".to_string();
                    task.message = Some("正在替换目标文件...".to_string());
                },
                true,
            )
            .await?;
            if task.direction == "upload" {
                finalize_remote_upload(
                    &app,
                    &tab_id,
                    &partial,
                    staging.as_deref(),
                    &destination_path,
                    source_size,
                    upload_plan
                        .as_ref()
                        .expect("upload plan exists")
                        .partial_ready,
                )
                .await?;
            } else {
                replace_local_file(Path::new(&partial), Path::new(&destination_path)).await?;
            }
            patch_task(
                &app,
                &transfer_id,
                |task| {
                    task.status = "done".to_string();
                    task.progress = 100.0;
                    task.message = None;
                    task.speed = None;
                    task.transferred_bytes = Some(source_size);
                    task.total_bytes = Some(source_size);
                    task.resumable = false;
                },
                true,
            )
            .await?;
            if task.direction == "upload" {
                if let Err(error) = refresh_remote_listing(&app, &tab_id).await {
                    crate::services::logging::warn(
                        &app,
                        &format!("transfer:{transfer_id}"),
                        format!("upload completed but remote listing refresh failed: {error}"),
                    );
                }
            }
            Ok(())
        }
        .await
    };
    state.transfer_controls.write().await.remove(&transfer_id);
    if result.is_ok() {
        if let Ok(current) = task_for(&app, &transfer_id).await {
            crate::services::logging::info(
                &app,
                &format!("transfer:{transfer_id}"),
                format!(
                    "stopped status={} transferred_bytes={} total_bytes={}",
                    current.status,
                    current.transferred_bytes.unwrap_or(0),
                    current.total_bytes.unwrap_or(0)
                ),
            );
        }
    }
    result
}

fn same_transfer_identity(current: &TransferFileIdentity, expected: &TransferFileIdentity) -> bool {
    current.size == expected.size
        && match (current.modified_at, expected.modified_at) {
            (Some(current), Some(expected)) => current.abs_diff(expected) < 1,
            _ => true,
        }
}

async fn stat_local_transfer_file(path: &str) -> Option<TransferFileIdentity> {
    let metadata = tokio::fs::metadata(path).await.ok()?;
    metadata.is_file().then(|| TransferFileIdentity {
        size: metadata.len(),
        modified_at: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_millis() as u64),
    })
}

async fn update_directory_manifest(
    app: &AppHandle,
    transfer_id: &str,
    manifest: &TransferManifest,
    status: &str,
    message: Option<String>,
    immediate: bool,
) -> Result<(), AppError> {
    let (transferred, total) = manifest_totals(manifest);
    patch_task(
        app,
        transfer_id,
        |task| {
            task.manifest = Some(manifest.clone());
            task.status = status.to_string();
            task.message = message;
            task.transferred_bytes = Some(transferred);
            task.total_bytes = Some(total);
            task.progress = if total == 0 {
                if status == "done" {
                    100.0
                } else {
                    0.0
                }
            } else if status == "done" {
                100.0
            } else {
                ((transferred as f64 / total as f64) * 100.0).min(99.0)
            };
            task.resumable = status != "done";
            if status != "running" {
                task.speed = None;
            }
        },
        immediate,
    )
    .await?;
    Ok(())
}

async fn refresh_remote_listing(app: &AppHandle, tab_id: &str) -> Result<(), AppError> {
    let path = app
        .state::<crate::services::workspace::WorkspaceState>()
        .sessions
        .read()
        .await
        .get(tab_id)
        .map(|session| session.remote_path.clone())
        .unwrap_or_else(|| "/".to_string());
    let files = worker_call(app, tab_id, |respond_to| WorkerCmd::ListRemoteFiles {
        path,
        respond_to,
    })
    .await?;
    if let Some(session) = app
        .state::<crate::services::workspace::WorkspaceState>()
        .sessions
        .write()
        .await
        .get_mut(tab_id)
    {
        session.remote_files = files;
    }
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
    Ok(())
}

async fn run_directory_transfer(
    app: &AppHandle,
    transfer_id: &str,
    tab_id: &str,
    task: &TransferTask,
    cancel: CancellationToken,
    resume_requested: bool,
) -> Result<(), AppError> {
    let mut manifest = task
        .manifest
        .clone()
        .filter(|manifest| manifest.version == 1)
        .ok_or_else(|| transfer_error("目录传输任务缺少有效 manifest"))?;

    if !resume_requested {
        for entry in &mut manifest.files {
            if task.direction == "upload" {
                remove_remote_upload_artifacts(
                    app,
                    tab_id,
                    &entry.partial_path,
                    entry.staging_path.as_deref(),
                )
                .await?;
            } else {
                let _ = tokio::fs::remove_file(&entry.partial_path).await;
            }
            entry.status = "pending".to_string();
            entry.transferred_bytes = 0;
        }
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            "running",
            Some("正在准备目录传输".to_string()),
            true,
        )
        .await?;
    }

    for directory in &manifest.directories {
        if cancel.is_cancelled() {
            return Ok(());
        }
        if task.direction == "upload" {
            ensure_remote_directory(app, tab_id, directory).await?;
        } else {
            tokio::fs::create_dir_all(directory)
                .await
                .map_err(|error| {
                    transfer_error(format!("无法创建本地目录 {directory}: {error}"))
                })?;
        }
    }

    for index in 0..manifest.files.len() {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let entry = manifest.files[index].clone();
        let current_identity = if task.direction == "upload" {
            stat_local_transfer_file(&entry.source_path)
                .await
                .ok_or_else(|| {
                    transfer_error(format!(
                        "上传源文件不存在或无法读取：{}",
                        entry.relative_path
                    ))
                })?
        } else {
            let stat = worker_call(app, tab_id, |respond_to| WorkerCmd::StatRemoteFile {
                path: entry.source_path.clone(),
                respond_to,
            })
            .await?
            .ok_or_else(|| {
                transfer_error(format!(
                    "下载源文件不存在或无法读取：{}",
                    entry.relative_path
                ))
            })?;
            TransferFileIdentity {
                size: stat.size,
                modified_at: stat.modified_at,
            }
        };
        if !same_transfer_identity(&current_identity, &entry.source_identity) {
            return Err(transfer_error(format!(
                "源文件已发生变化，不能继续目录断点：{}",
                entry.relative_path
            )));
        }

        if entry.status == "done" {
            let destination = if task.direction == "upload" {
                worker_call(app, tab_id, |respond_to| WorkerCmd::StatRemoteFile {
                    path: entry.destination_path.clone(),
                    respond_to,
                })
                .await?
                .map(|value| TransferFileIdentity {
                    size: value.size,
                    modified_at: value.modified_at,
                })
            } else {
                stat_local_transfer_file(&entry.destination_path).await
            };
            if destination
                .as_ref()
                .is_some_and(|value| value.size == entry.source_identity.size)
            {
                continue;
            }
        }

        let upload_plan = if task.direction == "upload" {
            Some(
                prepare_remote_upload(
                    app,
                    tab_id,
                    &entry.partial_path,
                    entry.staging_path.as_deref(),
                    entry.source_identity.size,
                )
                .await?,
            )
        } else {
            None
        };
        let offset = if let Some(plan) = upload_plan.as_ref() {
            plan.resume_offset
        } else {
            stat_local_transfer_file(&entry.partial_path)
                .await
                .map(|value| value.size)
                .unwrap_or(0)
        };
        if offset > entry.source_identity.size {
            return Err(transfer_error(format!(
                "断点文件大于源文件：{}",
                entry.relative_path
            )));
        }

        manifest.files[index].status = "running".to_string();
        manifest.files[index].transferred_bytes = offset;
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            "running",
            Some(if offset > 0 {
                format!("{}（从 {offset} bytes 继续）", entry.relative_path)
            } else {
                entry.relative_path.clone()
            }),
            true,
        )
        .await?;

        if task.direction == "upload" {
            let plan = upload_plan.as_ref().expect("upload plan exists");
            if plan.upload_needed {
                worker_call(app, tab_id, |respond_to| WorkerCmd::UploadLocalFile {
                    local_path: entry.source_path.clone(),
                    remote_path: plan.upload_path.clone(),
                    resume_offset: offset,
                    transfer_id: transfer_id.to_string(),
                    cancel: cancel.clone(),
                    respond_to,
                })
                .await?;
            }
        } else {
            if let Some(parent) = Path::new(&entry.partial_path).parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| transfer_error(error.to_string()))?;
            }
            worker_call(app, tab_id, |respond_to| WorkerCmd::DownloadRemoteFile {
                remote_path: entry.source_path.clone(),
                local_path: entry.partial_path.clone(),
                resume_offset: offset,
                transfer_id: transfer_id.to_string(),
                cancel: cancel.clone(),
                respond_to,
            })
            .await?;
        }
        if cancel.is_cancelled() {
            return Ok(());
        }

        let completed_size = if task.direction == "upload" {
            let plan = upload_plan.as_ref().expect("upload plan exists");
            if plan.partial_ready {
                entry.source_identity.size
            } else {
                stat_remote_transfer_size(app, tab_id, &plan.upload_path)
                    .await?
                    .unwrap_or(0)
            }
        } else {
            stat_local_transfer_file(&entry.partial_path)
                .await
                .map(|value| value.size)
                .unwrap_or(0)
        };
        if completed_size != entry.source_identity.size {
            return Err(transfer_error(format!(
                "传输校验失败：{} 断点大小为 {completed_size}，期望 {}",
                entry.relative_path, entry.source_identity.size
            )));
        }

        manifest.files[index].transferred_bytes = entry.source_identity.size;

        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            "finalizing",
            Some(format!("正在提交 {}", entry.relative_path)),
            true,
        )
        .await?;
        if task.direction == "upload" {
            finalize_remote_upload(
                app,
                tab_id,
                &entry.partial_path,
                entry.staging_path.as_deref(),
                &entry.destination_path,
                entry.source_identity.size,
                upload_plan
                    .as_ref()
                    .expect("upload plan exists")
                    .partial_ready,
            )
            .await?;
        } else {
            replace_local_file(
                Path::new(&entry.partial_path),
                Path::new(&entry.destination_path),
            )
            .await?;
        }
        manifest.files[index].status = "done".to_string();
        manifest.files[index].transferred_bytes = entry.source_identity.size;
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            "running",
            Some(entry.relative_path),
            true,
        )
        .await?;
    }

    update_directory_manifest(app, transfer_id, &manifest, "done", None, true).await?;
    if task.direction == "upload" {
        refresh_remote_listing(app, tab_id).await?;
    }
    Ok(())
}

async fn replace_local_file(partial: &Path, destination: &Path) -> Result<(), AppError> {
    let backup = destination.with_file_name(format!(
        "{}.fileterm-backup-{}",
        task_name(&destination.to_string_lossy()),
        uuid::Uuid::new_v4()
    ));
    let moved_destination = if tokio::fs::try_exists(destination).await.unwrap_or(false) {
        tokio::fs::rename(destination, &backup)
            .await
            .map_err(|error| transfer_error(error.to_string()))?;
        true
    } else {
        false
    };
    if let Err(error) = tokio::fs::rename(partial, destination).await {
        if moved_destination {
            let _ = tokio::fs::rename(&backup, destination).await;
        }
        return Err(transfer_error(error.to_string()));
    }
    if moved_destination {
        let _ = tokio::fs::remove_file(backup).await;
    }
    Ok(())
}

async fn fail_if_running(
    app: &AppHandle,
    transfer_id: &str,
    error: String,
) -> Result<(), AppError> {
    crate::services::logging::error(
        app,
        &format!("transfer:{transfer_id}"),
        format!("failed error={error}"),
    );
    let task = task_for(app, transfer_id).await?;
    if matches!(task.status.as_str(), "paused" | "canceled") {
        return Ok(());
    }
    if let Some(mut manifest) = task.manifest.clone() {
        let mut resumable = true;
        if let Some(entry) = manifest
            .files
            .iter_mut()
            .find(|entry| entry.status == "running")
        {
            let partial_size = if task.direction == "upload" {
                match task.tab_id.as_deref() {
                    Some(tab_id) => stat_remote_upload_progress(
                        app,
                        tab_id,
                        &entry.partial_path,
                        entry.staging_path.as_deref(),
                    )
                    .await,
                    None => None,
                }
            } else {
                stat_local_transfer_file(&entry.partial_path)
                    .await
                    .map(|identity| identity.size)
            };
            let Some(partial_size) = partial_size else {
                entry.transferred_bytes = 0;
                entry.status = "pending".to_string();
                update_directory_manifest(app, transfer_id, &manifest, "failed", Some(error), true)
                    .await?;
                return Ok(());
            };
            if partial_size > entry.source_identity.size {
                resumable = false;
            }
            entry.transferred_bytes = partial_size.min(entry.source_identity.size);
            entry.status = "pending".to_string();
        }
        let (transferred, total) = manifest_totals(&manifest);
        patch_task(
            app,
            transfer_id,
            |task| {
                task.manifest = Some(manifest);
                task.status = if resumable { "paused" } else { "failed" }.to_string();
                task.message = Some(error);
                task.speed = None;
                task.transferred_bytes = Some(transferred);
                task.total_bytes = Some(total);
                task.progress = if total == 0 {
                    0.0
                } else {
                    ((transferred as f64 / total as f64) * 100.0).min(99.0)
                };
                task.resumable = resumable;
            },
            true,
        )
        .await?;
        return Ok(());
    }
    let partial_size = if task.direction == "upload" {
        if let (Some(tab_id), Some(partial)) = (task.tab_id.as_deref(), task.partial_path.as_deref())
        {
            stat_remote_upload_progress(
                app,
                tab_id,
                partial,
                task.staging_path.as_deref(),
            )
            .await
        } else {
            None
        }
    } else {
        match task.partial_path.as_deref() {
            Some(path) => tokio::fs::metadata(path)
                .await
                .ok()
                .map(|metadata| metadata.len()),
            None => None,
        }
    };
    let source_size = task
        .source_identity
        .as_ref()
        .map(|identity| identity.size)
        .or(task.total_bytes);
    let resumable =
        matches!((partial_size, source_size), (Some(partial), Some(total)) if partial <= total);
    patch_task(
        app,
        transfer_id,
        |task| {
            task.status = if resumable { "paused" } else { "failed" }.to_string();
            task.message = Some(error);
            task.speed = None;
            task.transferred_bytes = partial_size.or(task.transferred_bytes);
            task.progress = match (partial_size, source_size) {
                (Some(partial), Some(total)) if total > 0 => {
                    ((partial as f64 / total as f64) * 100.0).min(99.0)
                }
                _ => task.progress,
            };
            task.resumable = resumable;
        },
        true,
    )
    .await?;
    Ok(())
}

pub async fn pause(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let task = task_for(app, &transfer_id).await?;
    if !task.active() || !task.resumable {
        return Ok(());
    }
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(token) = state
        .transfer_controls
        .read()
        .await
        .get(&transfer_id)
        .cloned()
    {
        token.cancel();
    }
    patch_task(
        app,
        &transfer_id,
        |task| {
            task.status = "paused".to_string();
            task.message = Some("传输已暂停，可继续".to_string());
            task.speed = None;
        },
        true,
    )
    .await?;
    crate::services::logging::warn(
        app,
        &format!("transfer:{transfer_id}"),
        "paused by user",
    );
    Ok(())
}

pub async fn pause_for_tab(app: &AppHandle, tab_id: &str, message: &str) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let transfer_ids = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|task| task.tab_id.as_deref() == Some(tab_id) && task.active() && task.resumable)
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    for transfer_id in transfer_ids {
        if let Some(token) = state
            .transfer_controls
            .read()
            .await
            .get(&transfer_id)
            .cloned()
        {
            token.cancel();
        }
        patch_task(
            app,
            &transfer_id,
            |task| {
                task.status = "paused".to_string();
                task.message = Some(message.to_string());
                task.speed = None;
            },
            true,
        )
        .await?;
    }
    Ok(())
}

pub async fn discard(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let task = task_for(app, &transfer_id).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(token) = state
        .transfer_controls
        .read()
        .await
        .get(&transfer_id)
        .cloned()
    {
        token.cancel();
    }
    patch_task(
        app,
        &transfer_id,
        |task| {
            task.status = "canceled".to_string();
            task.message = Some("传输已取消，正在清理断点".to_string());
            task.speed = None;
            task.resumable = false;
        },
        true,
    )
    .await?;
    crate::services::logging::warn(
        app,
        &format!("transfer:{transfer_id}"),
        "canceled by user; cleaning partial data",
    );
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let cleanup = if let Some(manifest) = task.manifest {
            let mut failures = Vec::new();
            for entry in manifest.files {
                let result = if task.direction == "upload" {
                    match task.tab_id.as_deref() {
                        Some(tab_id) => remove_remote_upload_artifacts(
                            &app_handle,
                            tab_id,
                            &entry.partial_path,
                            entry.staging_path.as_deref(),
                        )
                        .await,
                        None => Ok(()),
                    }
                } else {
                    tokio::fs::remove_file(entry.partial_path)
                        .await
                        .or_else(|error| {
                            (error.kind() == std::io::ErrorKind::NotFound)
                                .then_some(())
                                .ok_or(error)
                        })
                        .map_err(|error| transfer_error(error.to_string()))
                };
                if let Err(error) = result {
                    failures.push(error.to_string());
                }
            }
            if failures.is_empty() {
                Ok(())
            } else {
                Err(transfer_error(failures.join("；")))
            }
        } else if task.direction == "upload" {
            match (task.tab_id, task.partial_path) {
                (Some(tab_id), Some(path)) => remove_remote_upload_artifacts(
                    &app_handle,
                    &tab_id,
                    &path,
                    task.staging_path.as_deref(),
                )
                .await,
                _ => Ok(()),
            }
        } else if let Some(path) = task.partial_path {
            tokio::fs::remove_file(path)
                .await
                .or_else(|error| {
                    (error.kind() == std::io::ErrorKind::NotFound)
                        .then_some(())
                        .ok_or(error)
                })
                .map_err(|error| transfer_error(error.to_string()))
        } else {
            Ok(())
        };
        let message = cleanup
            .map(|_| "传输已取消，断点已清理".to_string())
            .unwrap_or_else(|error| format!("传输已取消，但断点清理失败：{error}"));
        let _ = patch_task(
            &app_handle,
            &transfer_id,
            |task| task.message = Some(message),
            true,
        )
        .await;
    });
    Ok(())
}

pub async fn resume(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let task = task_for(app, &transfer_id).await?;
    if !task.resumable || !matches!(task.status.as_str(), "paused" | "interrupted" | "failed") {
        return Err(transfer_error("该传输没有可用断点"));
    }
    let profile_id = task
        .profile_id
        .clone()
        .ok_or_else(|| transfer_error("传输任务缺少连接信息"))?;
    let tab_id = find_connected_tab(app, &profile_id)
        .await
        .ok_or_else(|| transfer_error("请先打开并连接原传输使用的连接，再继续任务"))?;
    if let Some(expected_mode) = task.file_access_mode.as_deref() {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let current_mode = state
            .sessions
            .read()
            .await
            .get(&tab_id)
            .map(|session| session.file_access_mode.clone())
            .unwrap_or_else(|| "user".to_string());
        if current_mode != expected_mode {
            return Err(transfer_error(
                "该任务的文件访问权限模式已变化，请切换回创建任务时的视图后再继续",
            ));
        }
    }
    patch_task(
        app,
        &transfer_id,
        |task| {
            task.tab_id = Some(tab_id);
            task.status = "queued".to_string();
            task.message = Some("等待继续传输".to_string());
            task.speed = None;
        },
        true,
    )
    .await?;
    crate::services::logging::info(
        app,
        &format!("transfer:{transfer_id}"),
        "resume queued",
    );
    start(app.clone(), transfer_id);
    Ok(())
}

pub async fn clear(app: &AppHandle, transfer_ids: Vec<String>) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let ids = transfer_ids
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    state
        .transfers
        .write()
        .await
        .retain(|task| !(ids.contains(&task.id) && task.terminal() && !task.resumable));
    persist(app).await
}

pub async fn shutdown(app: &AppHandle) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let controls = state
        .transfer_controls
        .read()
        .await
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for token in controls {
        token.cancel();
    }
    let mut tasks = state.transfers.write().await;
    let active_count = tasks.iter().filter(|task| task.active()).count();
    for task in tasks.iter_mut().filter(|task| task.active()) {
        task.status = if task.resumable { "paused" } else { "canceled" }.to_string();
        task.message = Some("应用退出时已暂停，可手动继续".to_string());
        task.speed = None;
        task.updated_at = Some(now_ms());
    }
    drop(tasks);
    crate::services::logging::info(
        app,
        "transfer",
        format!("shutdown active_tasks={active_count}"),
    );
    persist(app).await
}

#[cfg(test)]
mod tests {
    use super::{
        join_remote_path, manifest_totals, normalize_root_upload_staging, partial_path,
        TransferFileIdentity, TransferManifest, TransferManifestEntry, TransferTask,
    };

    #[test]
    fn creates_posix_paths_without_double_slashes() {
        assert_eq!(join_remote_path("/", "file.txt"), "/file.txt");
        assert_eq!(
            join_remote_path("/var/tmp/", "file.txt"),
            "/var/tmp/file.txt"
        );
        assert_eq!(
            partial_path("/var/tmp/file.txt"),
            "/var/tmp/file.txt.fileterm-part"
        );
    }

    #[test]
    fn migrates_legacy_root_uploads_to_two_stage_staging() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id": "transfer-root",
            "direction": "upload",
            "name": "config.toml",
            "progress": 42,
            "status": "paused",
            "fileAccessMode": "root",
            "targetType": "file",
            "destinationPath": "/etc/fileterm/config.toml",
            "partialPath": "/tmp/fileterm-root-upload-legacy.part",
            "resumable": true
        }))
        .unwrap();

        normalize_root_upload_staging(&mut task);

        assert_eq!(
            task.staging_path.as_deref(),
            Some("/tmp/fileterm-root-upload-legacy.part")
        );
        assert_eq!(
            task.partial_path.as_deref(),
            Some("/etc/fileterm/config.toml.fileterm-part")
        );
    }

    #[test]
    fn migrates_legacy_root_directory_entries_independently() {
        let mut task: TransferTask = serde_json::from_value(serde_json::json!({
            "id": "transfer-root-folder",
            "direction": "upload",
            "name": "configs",
            "progress": 10,
            "status": "paused",
            "fileAccessMode": "root",
            "targetType": "folder",
            "manifest": {
                "version": 1,
                "directories": ["/etc/fileterm"],
                "files": [{
                    "relativePath": "app.toml",
                    "sourcePath": "/local/app.toml",
                    "destinationPath": "/etc/fileterm/app.toml",
                    "partialPath": "/tmp/fileterm-root-upload-entry.part",
                    "sourceIdentity": { "size": 12 },
                    "status": "pending",
                    "transferredBytes": 4
                }]
            },
            "resumable": true
        }))
        .unwrap();

        normalize_root_upload_staging(&mut task);
        let entry = &task.manifest.as_ref().unwrap().files[0];

        assert_eq!(
            entry.staging_path.as_deref(),
            Some("/tmp/fileterm-root-upload-entry.part")
        );
        assert_eq!(
            entry.partial_path,
            "/etc/fileterm/app.toml.fileterm-part"
        );
    }

    #[test]
    fn manifest_totals_count_completed_and_partial_files_once() {
        let manifest = TransferManifest {
            version: 1,
            directories: vec!["/tmp/export".to_string()],
            files: vec![
                TransferManifestEntry {
                    relative_path: "done.txt".to_string(),
                    source_path: "/remote/done.txt".to_string(),
                    destination_path: "/tmp/export/done.txt".to_string(),
                    partial_path: "/tmp/export/done.txt.fileterm-part".to_string(),
                    staging_path: None,
                    source_identity: TransferFileIdentity {
                        size: 10,
                        modified_at: None,
                    },
                    status: "done".to_string(),
                    transferred_bytes: 10,
                },
                TransferManifestEntry {
                    relative_path: "partial.txt".to_string(),
                    source_path: "/remote/partial.txt".to_string(),
                    destination_path: "/tmp/export/partial.txt".to_string(),
                    partial_path: "/tmp/export/partial.txt.fileterm-part".to_string(),
                    staging_path: None,
                    source_identity: TransferFileIdentity {
                        size: 20,
                        modified_at: None,
                    },
                    status: "running".to_string(),
                    transferred_bytes: 7,
                },
            ],
        };
        assert_eq!(manifest_totals(&manifest), (17, 30));
    }
}
