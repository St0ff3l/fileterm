async fn remove_transfer_run_if_generation(app: &AppHandle, transfer_id: &str, generation: u64) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut runs = state.transfer_runs.write().await;
    if runs
        .get(transfer_id)
        .is_some_and(|handle| handle.generation == generation)
    {
        runs.remove(transfer_id);
    }
}

async fn clear_transfer_progress_runtime(app: &AppHandle, transfer_id: &str) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.transfer_last_event.lock().await.remove(transfer_id);
    state
        .transfer_progress_samples
        .lock()
        .await
        .remove(transfer_id);
}

async fn cancel_and_wait_transfer_run(app: &AppHandle, transfer_id: &str) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let handle = state.transfer_runs.read().await.get(transfer_id).cloned();
    let Some(handle) = handle else {
        return Ok(());
    };

    handle.cancel.cancel();
    let generation = handle.generation;
    tokio::time::timeout(TRANSFER_STOP_TIMEOUT, handle.wait_until_settled())
        .await
        .map_err(|_| transfer_error("等待当前传输停止超时；未执行后续状态变更"))?;
    remove_transfer_run_if_generation(app, transfer_id, generation).await;
    clear_transfer_progress_runtime(app, transfer_id).await;
    Ok(())
}

async fn cancel_and_wait_all_transfer_runs(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let handles = state
        .transfer_runs
        .read()
        .await
        .iter()
        .map(|(transfer_id, handle)| (transfer_id.clone(), handle.clone()))
        .collect::<Vec<_>>();
    for (_, handle) in &handles {
        handle.cancel.cancel();
    }

    tokio::time::timeout(TRANSFER_STOP_TIMEOUT, async {
        for (_, handle) in &handles {
            handle.clone().wait_until_settled().await;
        }
    })
    .await
    .map_err(|_| transfer_error("等待活动传输停止超时，已取消退出以保护断点数据"))?;

    for (transfer_id, handle) in handles {
        remove_transfer_run_if_generation(app, &transfer_id, handle.generation).await;
        clear_transfer_progress_runtime(app, &transfer_id).await;
    }
    Ok(())
}

async fn start(app: AppHandle, transfer_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let generation = state
        .next_transfer_generation
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    let cancel = CancellationToken::new();
    let (settled_tx, settled_rx) = watch::channel(false);
    {
        let mut runs = state.transfer_runs.write().await;
        if runs.contains_key(&transfer_id) {
            return Err(transfer_error("该传输已有活动任务，不能重复启动"));
        }
        runs.insert(
            transfer_id.clone(),
            crate::services::workspace::TransferRunHandle {
                generation,
                cancel: cancel.clone(),
                settled: settled_rx,
            },
        );
    }

    tauri::async_runtime::spawn(async move {
        let cancel_for_error = cancel.clone();
        if let Err(error) = run(app.clone(), transfer_id.clone(), cancel).await {
            if !cancel_for_error.is_cancelled() {
                let _ = fail_if_running(&app, &transfer_id, error.to_string()).await;
            }
        }
        clear_transfer_progress_runtime(&app, &transfer_id).await;
        let _ = settled_tx.send(true);
        remove_transfer_run_if_generation(&app, &transfer_id, generation).await;
    });
    Ok(())
}

async fn run(
    app: AppHandle,
    transfer_id: String,
    cancel: CancellationToken,
) -> Result<(), AppError> {
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
    task = patch_task(
        &app,
        &transfer_id,
        |task| {
            task.tab_id = Some(tab_id.clone());
            task.status = "running".to_string();
            task.message = Some("正在检查断点...".to_string());
            task.speed = None;
        },
        PatchDelivery::PersistedEvent,
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
                let source =
                    worker_call_with_cancel(&app, &tab_id, &cancel, |respond_to, token| {
                        WorkerCmd::StatRemoteFile {
                            path: source_path.clone(),
                            cancellation: token,
                            respond_to,
                        }
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
                        Some(&cancel),
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
                PatchDelivery::PersistedEvent,
            )
            .await?;
            if task.direction == "upload" {
                let plan = upload_plan
                    .as_ref()
                    .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?;
                if plan.upload_needed {
                    worker_data_call_with_cancel(&app, &tab_id, &cancel, |respond_to, _token| {
                        WorkerCmd::UploadLocalFile {
                            local_path: source_path,
                            remote_path: plan.upload_path.clone(),
                            resume_offset: offset,
                            transfer_id: transfer_id.clone(),
                            cancel: cancel.clone(),
                            respond_to,
                        }
                    })
                    .await?;
                }
            } else {
                if let Some(parent) = Path::new(&partial).parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|error| transfer_error(error.to_string()))?;
                }
                worker_data_call_with_cancel(&app, &tab_id, &cancel, |respond_to, _token| {
                    WorkerCmd::DownloadRemoteFile {
                        remote_path: source_path,
                        local_path: partial.clone(),
                        resume_offset: offset,
                        transfer_id: transfer_id.clone(),
                        cancel: cancel.clone(),
                        respond_to,
                    }
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
                PatchDelivery::PersistedEvent,
            )
            .await?;
            let completed_size = if task.direction == "upload" {
                let plan = upload_plan
                    .as_ref()
                    .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?;
                if plan.partial_ready {
                    source_size
                } else {
                    stat_remote_transfer_size(&app, &tab_id, &plan.upload_path, Some(&cancel))
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
                PatchDelivery::PersistedEvent,
            )
            .await?;
            if task.direction == "upload" {
                finalize_remote_upload(
                    &app,
                    &tab_id,
                    RemoteUploadFinalize {
                        partial_path: &partial,
                        staging_path: staging.as_deref(),
                        destination_path: &destination_path,
                        source_size,
                        partial_ready: upload_plan
                            .as_ref()
                            .ok_or_else(|| transfer_error("上传任务缺少 upload plan"))?
                            .partial_ready,
                    },
                    Some(&cancel),
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
                PatchDelivery::PersistedEvent,
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
