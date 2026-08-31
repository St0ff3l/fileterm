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
        if immediate {
            PatchDelivery::PersistedEvent
        } else {
            PatchDelivery::Event
        },
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

    if !resume_requested {
        for entry in &mut manifest.files {
            if task.direction == "upload" {
                remove_remote_upload_artifacts(
                    app,
                    tab_id,
                    &entry.partial_path,
                    entry.staging_path.as_deref(),
                    Some(&cancel),
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
            ensure_remote_directory(app, tab_id, directory, Some(&cancel)).await?;
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
            let stat = worker_call_with_cancel(app, tab_id, &cancel, |respond_to, token| {
                WorkerCmd::StatRemoteFile {
                    path: entry.source_path.clone(),
                    cancellation: token,
                    respond_to,
                }
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
                worker_call_with_cancel(app, tab_id, &cancel, |respond_to, token| {
                    WorkerCmd::StatRemoteFile {
                        path: entry.destination_path.clone(),
                        cancellation: token,
                        respond_to,
                    }
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
                    Some(&cancel),
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
            let plan = upload_plan
                .as_ref()
                .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?;
            if plan.upload_needed {
                worker_data_call_with_cancel(app, tab_id, &cancel, |respond_to, _token| {
                    WorkerCmd::UploadLocalFile {
                        local_path: entry.source_path.clone(),
                        remote_path: plan.upload_path.clone(),
                        resume_offset: offset,
                        transfer_id: transfer_id.to_string(),
                        cancel: cancel.clone(),
                        respond_to,
                    }
                })
                .await?;
            }
        } else {
            if let Some(parent) = Path::new(&entry.partial_path).parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|error| transfer_error(error.to_string()))?;
            }
            worker_data_call_with_cancel(app, tab_id, &cancel, |respond_to, _token| {
                WorkerCmd::DownloadRemoteFile {
                    remote_path: entry.source_path.clone(),
                    local_path: entry.partial_path.clone(),
                    resume_offset: offset,
                    transfer_id: transfer_id.to_string(),
                    cancel: cancel.clone(),
                    respond_to,
                }
            })
            .await?;
        }
        if cancel.is_cancelled() {
            return Ok(());
        }

        let completed_size = if task.direction == "upload" {
            let plan = upload_plan
                .as_ref()
                .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?;
            if plan.partial_ready {
                entry.source_identity.size
            } else {
                stat_remote_transfer_size(app, tab_id, &plan.upload_path, Some(&cancel))
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
                RemoteUploadFinalize {
                    partial_path: &entry.partial_path,
                    staging_path: entry.staging_path.as_deref(),
                    destination_path: &entry.destination_path,
                    source_size: entry.source_identity.size,
                    partial_ready: upload_plan
                        .as_ref()
                        .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?
                        .partial_ready,
                },
                Some(&cancel),
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
