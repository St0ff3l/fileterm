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

struct DirectoryTransferThrottler {
    last_emitted: std::time::Instant,
    last_persisted: std::time::Instant,
    completed_since_persist: usize,
}

impl DirectoryTransferThrottler {
    fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            last_emitted: now,
            last_persisted: now,
            completed_since_persist: 0,
        }
    }

    fn delivery_for_file_start(&mut self) -> PatchDelivery {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_emitted) >= Duration::from_millis(150) {
            self.last_emitted = now;
            PatchDelivery::Event
        } else {
            PatchDelivery::Silent
        }
    }

    fn delivery_for_file_completed(&mut self) -> PatchDelivery {
        self.completed_since_persist += 1;
        let now = std::time::Instant::now();
        let should_persist = now.duration_since(self.last_persisted) >= Duration::from_secs(2)
            || self.completed_since_persist >= 50;
        if should_persist {
            self.last_persisted = now;
            self.last_emitted = now;
            self.completed_since_persist = 0;
            PatchDelivery::PersistedEvent
        } else if now.duration_since(self.last_emitted) >= Duration::from_millis(150) {
            self.last_emitted = now;
            PatchDelivery::Event
        } else {
            PatchDelivery::Silent
        }
    }
}

struct DirectoryManifestPatch<'a> {
    status: &'a str,
    message: Option<String>,
    transferred: u64,
    total: u64,
    delivery: PatchDelivery,
    changed_entry: Option<usize>,
}

async fn update_directory_manifest(
    app: &AppHandle,
    transfer_id: &str,
    manifest: &TransferManifest,
    patch: DirectoryManifestPatch<'_>,
) -> Result<(), AppError> {
    let DirectoryManifestPatch {
        status,
        message,
        transferred,
        total,
        delivery,
        changed_entry,
    } = patch;
    patch_task(
        app,
        transfer_id,
        |task| {
            sync_directory_manifest(&mut task.manifest, manifest, changed_entry);
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
        delivery,
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
    let files = worker_call(app, tab_id, |respond_to, cancellation| {
        WorkerCmd::ListRemoteFiles {
            path: path.clone(),
            cancellation,
            respond_to,
        }
    })
    .await?;
    if let Some(session) = app
        .state::<crate::services::workspace::WorkspaceState>()
        .sessions
        .write()
        .await
        .get_mut(tab_id)
    {
        session.remote_files = files.clone();
    }
    let payload = serde_json::json!({
        "tabId": tab_id,
        "path": path,
        "files": files,
    });
    if let Err(error) = app.emit_to(
        EventTarget::webview_window("main"),
        "workspace:remote-files",
        &payload,
    ) {
        crate::services::logging::warn(
            app,
            "transfer",
            format!("remote listing changed but event emission failed: {error}"),
        );
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

    let (initial_transferred, total_bytes) = manifest_totals(&manifest);
    let mut current_transferred = initial_transferred;
    let mut throttler = DirectoryTransferThrottler::new();

    if !resume_requested {
        for entry in &mut manifest.files {
            entry.status = "pending".to_string();
            entry.transferred_bytes = 0;
        }
        current_transferred = 0;
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            DirectoryManifestPatch {
                changed_entry: None,
                status: "running",
                message: Some("正在准备目录传输".to_string()),
                transferred: 0,
                total: total_bytes,
                delivery: PatchDelivery::PersistedEvent,
            },
        )
        .await?;
    }

    for directory in &manifest.directories {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let mut attempt = 0_u32;
        loop {
            attempt += 1;
            let result = if task.direction == "upload" {
                ensure_remote_directory(app, tab_id, directory, Some(&cancel)).await
            } else {
                tokio::fs::create_dir_all(directory).await.map_err(|error| {
                    transfer_error(format!("无法创建本地目录 {directory}: {error}"))
                })
            };
            match result {
                Ok(()) => break,
                Err(error) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    if is_permanent_transfer_error(&error.to_string())
                        || attempt >= TRANSIENT_MAX_ATTEMPTS
                    {
                        return Err(error);
                    }
                    sleep_transient_backoff(&cancel, transient_retry_backoff(attempt)).await;
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                }
            }
        }
    }

    let mut failed_files: Vec<String> = Vec::new();
    for index in 0..manifest.files.len() {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let entry = manifest.files[index].clone();
        let mut attempt = 0_u32;
        loop {
            attempt += 1;
            let outcome = transfer_directory_entry_once(
                app,
                transfer_id,
                tab_id,
                &task.direction,
                task.session_type.as_deref(),
                &cancel,
                resume_requested,
                &mut manifest,
                index,
                current_transferred,
                total_bytes,
                &mut throttler,
            )
            .await;
            match outcome {
                Ok(DirectoryEntryAttempt::Transferred) => {
                    manifest.files[index].status = "done".to_string();
                    current_transferred = current_transferred
                        .saturating_sub(manifest.files[index].transferred_bytes)
                        .saturating_add(entry.source_identity.size);
                    manifest.files[index].transferred_bytes = entry.source_identity.size;

                    let done_delivery = throttler.delivery_for_file_completed();
                    update_directory_manifest(
                        app,
                        transfer_id,
                        &manifest,
                        DirectoryManifestPatch {
                            changed_entry: Some(index),
                            status: "running",
                            message: Some(entry.relative_path),
                            transferred: current_transferred,
                            total: total_bytes,
                            delivery: done_delivery,
                        },
                    )
                    .await?;
                    break;
                }
                Ok(DirectoryEntryAttempt::AlreadyDone) => break,
                Err(error) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    if is_permanent_transfer_error(&error.to_string()) {
                        // 永久错误：失败半径收敛到当前文件，跳过并继续其余文件。
                        // transferred_bytes 保留最后一次尝试写入的偏移（保守值），
                        // 恢复时由 prepare_remote_upload 重新探测真实断点。
                        crate::services::logging::warn(
                            app,
                            &format!("transfer:{transfer_id}"),
                            format!(
                                "file skipped after permanent error: {} attempt={attempt} error={error}",
                                entry.relative_path
                            ),
                        );
                        manifest.files[index].status = "failed".to_string();
                        failed_files.push(format!("{}：{error}", entry.relative_path));
                        let failed_delivery = throttler.delivery_for_file_completed();
                        update_directory_manifest(
                            app,
                            transfer_id,
                            &manifest,
                            DirectoryManifestPatch {
                                changed_entry: Some(index),
                                status: "running",
                                message: Some(format!("已跳过失败文件 {}", entry.relative_path)),
                                transferred: current_transferred,
                                total: total_bytes,
                                delivery: failed_delivery,
                            },
                        )
                        .await?;
                        break;
                    }
                    if attempt >= TRANSIENT_MAX_ATTEMPTS {
                        // 瞬时错误重试耗尽：大概率链路已断，继续只会让每个文件
                        // 都空耗重试预算。交由任务级失败路径保留断点并进入 paused。
                        return Err(error);
                    }
                    crate::services::logging::warn(
                        app,
                        &format!("transfer:{transfer_id}"),
                        format!(
                            "retrying file after transient error: {} attempt={attempt}/{} error={error}",
                            entry.relative_path, TRANSIENT_MAX_ATTEMPTS
                        ),
                    );
                    let retry_delivery = throttler.delivery_for_file_start();
                    update_directory_manifest(
                        app,
                        transfer_id,
                        &manifest,
                        DirectoryManifestPatch {
                            changed_entry: Some(index),
                            status: "running",
                            message: Some(format!(
                                "{}（第 {attempt} 次重试）",
                                entry.relative_path
                            )),
                            transferred: current_transferred,
                            total: total_bytes,
                            delivery: retry_delivery,
                        },
                    )
                    .await?;
                    sleep_transient_backoff(&cancel, transient_retry_backoff(attempt)).await;
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                }
            }
        }
    }

    if failed_files.is_empty() {
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            DirectoryManifestPatch {
                changed_entry: None,
                status: "done",
                message: None,
                transferred: total_bytes,
                total: total_bytes,
                delivery: PatchDelivery::PersistedEvent,
            },
        )
        .await?;
    } else {
        // 部分文件失败：任务进入可续传的 paused 状态而非终态，点击继续时
        // 已完成文件被远端大小校验跳过，仅失败与未完成文件会重试。
        let sample = failed_files.iter().take(3).cloned().collect::<Vec<_>>();
        let summary = if failed_files.len() > sample.len() {
            format!(
                "{} 个文件传输失败（如 {}），点击继续可重试失败项",
                failed_files.len(),
                sample.join("；")
            )
        } else {
            format!(
                "{} 个文件传输失败，点击继续可重试失败项：{}",
                failed_files.len(),
                sample.join("；")
            )
        };
        update_directory_manifest(
            app,
            transfer_id,
            &manifest,
            DirectoryManifestPatch {
                changed_entry: None,
                status: "paused",
                message: Some(summary),
                transferred: current_transferred,
                total: total_bytes,
                delivery: PatchDelivery::PersistedEvent,
            },
        )
        .await?;
    }
    if task.direction == "upload" {
        if let Err(error) = refresh_remote_listing(app, tab_id).await {
            crate::services::logging::warn(
                app,
                &format!("transfer:{transfer_id}"),
                format!("directory upload completed but remote listing refresh failed: {error}"),
            );
        }
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
    if task.terminal() || task.status == "paused" {
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
                    Some(tab_id) => {
                        stat_remote_upload_progress(
                            app,
                            tab_id,
                            &entry.partial_path,
                            entry.staging_path.as_deref(),
                            None,
                        )
                        .await
                    }
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
                let (transferred, total) = manifest_totals(&manifest);
                update_directory_manifest(
                    app,
                    transfer_id,
                    &manifest,
                    DirectoryManifestPatch {
                        changed_entry: None,
                        status: "failed",
                        message: Some(error),
                        transferred,
                        total,
                        delivery: PatchDelivery::PersistedEvent,
                    },
                )
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
                task.status = failure_status(resumable).to_string();
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
            PatchDelivery::PersistedEvent,
        )
        .await?;
        return Ok(());
    }
    let partial_size = if task.direction == "upload" {
        if let (Some(tab_id), Some(partial)) =
            (task.tab_id.as_deref(), task.partial_path.as_deref())
        {
            stat_remote_upload_progress(app, tab_id, partial, task.staging_path.as_deref(), None)
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
            task.status = failure_status(resumable).to_string();
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
        PatchDelivery::PersistedEvent,
    )
    .await?;
    Ok(())
}

/// Keep every entry transition in memory, including throttled/silent ones.
/// Cloning the whole manifest per tiny file makes directory uploads quadratic.
fn sync_directory_manifest(
    current: &mut Option<TransferManifest>,
    manifest: &TransferManifest,
    changed_entry: Option<usize>,
) {
    if let (Some(current), Some(index)) = (current.as_mut(), changed_entry) {
        if let (Some(target), Some(source)) =
            (current.files.get_mut(index), manifest.files.get(index))
        {
            *target = source.clone();
            return;
        }
    }
    *current = Some(manifest.clone());
}
