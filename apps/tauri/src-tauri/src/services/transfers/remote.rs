#[derive(Debug, PartialEq, Eq)]
struct RemoteUploadPlan {
    upload_path: String,
    resume_offset: u64,
    upload_needed: bool,
    partial_ready: bool,
}

struct RemoteUploadFinalize<'a> {
    partial_path: &'a str,
    staging_path: Option<&'a str>,
    destination_path: &'a str,
    source_size: u64,
    partial_ready: bool,
}

async fn stat_remote_transfer_size(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<Option<u64>, AppError> {
    let call = |respond_to, token| WorkerCmd::StatRemoteFile {
        path: path.to_string(),
        cancellation: token,
        respond_to,
    };
    let stat = match cancellation {
        Some(cancellation) => worker_call_with_cancel(app, tab_id, cancellation, call).await?,
        None => worker_call(app, tab_id, call).await?,
    };
    Ok(stat.map(|value| value.size))
}

async fn remove_remote_transfer_file(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<(), AppError> {
    let call = |respond_to, token| WorkerCmd::RemoveRemoteFile {
        path: path.to_string(),
        cancellation: token,
        respond_to,
    };
    match cancellation {
        Some(cancellation) => worker_call_with_cancel(app, tab_id, cancellation, call).await,
        None => worker_call(app, tab_id, call).await,
    }
}

async fn stat_remote_upload_progress(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
    cancellation: Option<&CancellationToken>,
) -> Option<u64> {
    if let Some(staging_path) = staging_path {
        if let Some(size) = stat_remote_transfer_size(app, tab_id, staging_path, cancellation)
            .await
            .ok()
            .flatten()
        {
            return Some(size);
        }
    }
    stat_remote_transfer_size(app, tab_id, partial_path, cancellation)
        .await
        .ok()
        .flatten()
}

async fn remove_remote_upload_artifacts(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
    cancellation: Option<&CancellationToken>,
) -> Result<(), AppError> {
    remove_remote_transfer_file(app, tab_id, partial_path, cancellation).await?;
    if let Some(staging_path) = staging_path {
        remove_remote_transfer_file(app, tab_id, staging_path, cancellation).await?;
    }
    Ok(())
}

async fn prepare_remote_upload(
    app: &AppHandle,
    tab_id: &str,
    partial_path: &str,
    staging_path: Option<&str>,
    source_size: u64,
    cancellation: Option<&CancellationToken>,
) -> Result<RemoteUploadPlan, AppError> {
    if let Some(staging_path) = staging_path {
        let partial_size =
            stat_remote_transfer_size(app, tab_id, partial_path, cancellation).await?;
        if partial_size == Some(source_size) {
            return Ok(RemoteUploadPlan {
                upload_path: staging_path.to_string(),
                resume_offset: source_size,
                upload_needed: false,
                partial_ready: true,
            });
        }
        if partial_size.is_some() {
            remove_remote_transfer_file(app, tab_id, partial_path, cancellation).await?;
        }

        let staging_size =
            stat_remote_transfer_size(app, tab_id, staging_path, cancellation).await?;
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

    let partial_size = stat_remote_transfer_size(app, tab_id, partial_path, cancellation).await?;
    let resume_offset = partial_size.unwrap_or(0);
    if resume_offset > source_size {
        return Err(transfer_error("断点文件大于源文件，请丢弃断点后重新传输"));
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
    finalize: RemoteUploadFinalize<'_>,
    cancellation: Option<&CancellationToken>,
) -> Result<(), AppError> {
    if let Some(staging_path) = finalize.staging_path {
        if !finalize.partial_ready {
            let call = |respond_to, token| WorkerCmd::CommitRemoteStaging {
                staging_path: staging_path.to_string(),
                partial_path: finalize.partial_path.to_string(),
                cancellation: token,
                respond_to,
            };
            match cancellation {
                Some(cancellation) => {
                    worker_call_with_cancel(app, tab_id, cancellation, call).await?
                }
                None => worker_call(app, tab_id, call).await?,
            };
        }
        let committed_size =
            stat_remote_transfer_size(app, tab_id, finalize.partial_path, cancellation)
                .await?
                .unwrap_or(0);
        if committed_size != finalize.source_size {
            return Err(transfer_error(format!(
                "root 目标目录断点校验失败：{} bytes，期望 {}",
                committed_size, finalize.source_size
            )));
        }
    }

    let call = |respond_to, token| WorkerCmd::ReplaceRemoteFile {
        partial_path: finalize.partial_path.to_string(),
        destination_path: finalize.destination_path.to_string(),
        cancellation: token,
        respond_to,
    };
    match cancellation {
        Some(cancellation) => worker_call_with_cancel(app, tab_id, cancellation, call).await,
        None => worker_call(app, tab_id, call).await,
    }
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
    cancellation: Option<&CancellationToken>,
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
    let call = |respond_to, token| WorkerCmd::CreateRemoteDirectory {
        parent_path: parent,
        name,
        cancellation: token,
        respond_to,
    };
    match cancellation {
        Some(cancellation) => worker_call_with_cancel(app, tab_id, cancellation, call).await,
        None => worker_call(app, tab_id, call).await,
    }
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
    let mut pending = vec![(root.to_string(), 0usize)];
    let mut visited_directories = HashSet::new();
    let mut entry_count = 0usize;
    let mut total_bytes = 0u64;
    while let Some((directory, depth)) = pending.pop() {
        if !visited_directories.insert(directory.clone()) {
            return Err(transfer_error(format!(
                "远端目录传输检测到循环路径：{directory}"
            )));
        }
        let entries = worker_call(app, tab_id, |respond_to, cancellation| {
            WorkerCmd::ListRemoteFiles {
                path: directory.clone(),
                cancellation,
                respond_to,
            }
        })
        .await?;
        for entry in entries {
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_REMOTE_TREE_ENTRIES {
                return Err(transfer_error(format!(
                    "远端目录传输超过 {} 个条目，已停止以保护本机资源",
                    MAX_REMOTE_TREE_ENTRIES
                )));
            }
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
            if entry
                .get("isSymlink")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(transfer_error(format!("目录传输不跟随符号链接：{path}")));
            }
            if entry.get("type").and_then(|value| value.as_str()) == Some("folder") {
                if depth >= MAX_REMOTE_TREE_DEPTH {
                    return Err(transfer_error(format!(
                        "远端目录传输超过 {} 层，已停止以保护本机资源",
                        MAX_REMOTE_TREE_DEPTH
                    )));
                }
                directories.push(path.clone());
                pending.push((path, depth + 1));
                continue;
            }
            let identity = worker_call(app, tab_id, |respond_to, cancellation| {
                WorkerCmd::StatRemoteFile {
                    path: path.clone(),
                    cancellation,
                    respond_to,
                }
            })
            .await?
            .ok_or_else(|| transfer_error(format!("无法读取远端文件信息: {path}")))?;
            total_bytes = total_bytes
                .checked_add(identity.size)
                .ok_or_else(|| transfer_error("远端目录总大小超出支持范围"))?;
            if total_bytes > MAX_REMOTE_TREE_BYTES {
                return Err(transfer_error(format!(
                    "远端目录总大小超过 {} GiB，已停止以保护本机资源",
                    MAX_REMOTE_TREE_BYTES / (1024 * 1024 * 1024)
                )));
            }
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
    fn normalized_segments(value: &str) -> Result<Vec<&str>, AppError> {
        let mut segments = Vec::new();
        for segment in value.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    segments.pop().ok_or_else(|| {
                        transfer_error(format!("远端路径包含越界父目录：{value}"))
                    })?;
                }
                _ => segments.push(segment),
            }
        }
        Ok(segments)
    }

    let root_segments = normalized_segments(root)?;
    let path_segments = normalized_segments(path)?;
    let relative = path_segments
        .strip_prefix(root_segments.as_slice())
        .filter(|segments| !segments.is_empty())
        .ok_or_else(|| transfer_error(format!("路径 {path} 不在根目录 {root} 内")))?;
    Ok(relative.join("/"))
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
