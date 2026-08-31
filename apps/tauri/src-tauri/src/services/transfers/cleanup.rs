pub async fn pause(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let task = task_for(app, &transfer_id).await?;
    if !task.active() || !task.resumable {
        return Ok(());
    }
    cancel_and_wait_transfer_run(app, &transfer_id).await?;
    if task_for(app, &transfer_id).await?.terminal() {
        return Ok(());
    }
    patch_task(
        app,
        &transfer_id,
        |task| {
            task.status = "paused".to_string();
            task.message = Some("传输已暂停，可继续".to_string());
            task.speed = None;
        },
        PatchDelivery::PersistedEvent,
    )
    .await?;
    crate::services::logging::warn(app, &format!("transfer:{transfer_id}"), "paused by user");
    Ok(())
}

pub async fn pause_for_tab(app: &AppHandle, tab_id: &str, message: &str) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let transfer_ids = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|task| task.tab_id.as_deref() == Some(tab_id) && task.active() && task.resumable)
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    for transfer_id in transfer_ids {
        cancel_and_wait_transfer_run(app, &transfer_id).await?;
        if task_for(app, &transfer_id).await?.terminal() {
            continue;
        }
        patch_task(
            app,
            &transfer_id,
            |task| {
                task.status = "paused".to_string();
                task.message = Some(message.to_string());
                task.speed = None;
            },
            PatchDelivery::PersistedEvent,
        )
        .await?;
    }
    Ok(())
}

async fn remove_local_partial(path: &str) -> Result<(), AppError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(transfer_error(error.to_string())),
    }
}

async fn cleanup_transfer_partial(app: &AppHandle, task: &TransferTask) -> Result<(), AppError> {
    if let Some(manifest) = &task.manifest {
        let mut failures = Vec::new();
        for entry in &manifest.files {
            let result = if task.direction == "upload" {
                match task.tab_id.as_deref() {
                    Some(tab_id) => {
                        remove_remote_upload_artifacts(
                            app,
                            tab_id,
                            &entry.partial_path,
                            entry.staging_path.as_deref(),
                            None,
                        )
                        .await
                    }
                    None => Err(transfer_error("上传任务缺少连接标签，无法清理远端断点")),
                }
            } else {
                remove_local_partial(&entry.partial_path).await
            };
            if let Err(error) = result {
                failures.push(error.to_string());
            }
        }
        return if failures.is_empty() {
            Ok(())
        } else {
            Err(transfer_error(failures.join("；")))
        };
    }

    if task.direction == "upload" {
        return match (task.tab_id.as_deref(), task.partial_path.as_deref()) {
            (Some(tab_id), Some(path)) => {
                remove_remote_upload_artifacts(
                    app,
                    tab_id,
                    path,
                    task.staging_path.as_deref(),
                    None,
                )
                .await
            }
            (None, Some(_)) => Err(transfer_error("上传任务缺少连接标签，无法清理远端断点")),
            _ => Ok(()),
        };
    }

    match task.partial_path.as_deref() {
        Some(path) => remove_local_partial(path).await,
        None => Ok(()),
    }
}

async fn record_cleanup_attempt(
    app: &AppHandle,
    transfer_id: &str,
    tab_id: Option<String>,
    result: &Result<(), AppError>,
    success_message: &str,
    failure_prefix: &str,
) -> Result<(), AppError> {
    patch_task(
        app,
        transfer_id,
        |task| {
            if let Some(tab_id) = tab_id {
                task.tab_id = Some(tab_id);
            }
            match result {
                Ok(()) => {
                    task.message = Some(success_message.to_string());
                    task.cleanup_pending = false;
                    task.retry_attempt = None;
                }
                Err(error) => {
                    task.message = Some(format!("{failure_prefix}：{error}"));
                    task.cleanup_pending = true;
                    task.retry_attempt = Some(task.retry_attempt.unwrap_or(0).saturating_add(1));
                }
            }
        },
        PatchDelivery::PersistedEvent,
    )
    .await?;
    Ok(())
}

async fn retry_pending_local_cleanup(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let tasks = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|task| task.cleanup_pending && task.direction != "upload")
        .cloned()
        .collect::<Vec<_>>();
    for task in tasks {
        let result = cleanup_transfer_partial(app, &task).await;
        record_cleanup_attempt(
            app,
            &task.id,
            None,
            &result,
            "应用启动时已自动清理本地断点",
            "应用启动时自动清理本地断点失败",
        )
        .await?;
    }
    Ok(())
}

/// Retry remote partial cleanup after the matching SSH/FTP worker has fully
/// established its file channel. This keeps cleanupPending actionable across
/// app restarts and tab replacement instead of requiring the stale tab ID.
pub async fn retry_pending_cleanup_for_tab(app: &AppHandle, tab_id: &str) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let Some((profile_id, file_access_mode, true)) =
        state.sessions.read().await.get(tab_id).map(|session| {
            (
                session.profile_id.clone(),
                session.file_access_mode.clone(),
                session.connected,
            )
        })
    else {
        return Ok(());
    };
    let tasks = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|task| {
            task.cleanup_pending
                && task.direction == "upload"
                && task.profile_id.as_deref() == Some(profile_id.as_str())
                && task
                    .file_access_mode
                    .as_deref()
                    .is_none_or(|expected| expected == file_access_mode)
        })
        .cloned()
        .collect::<Vec<_>>();
    for mut task in tasks {
        task.tab_id = Some(tab_id.to_string());
        let result = cleanup_transfer_partial(app, &task).await;
        record_cleanup_attempt(
            app,
            &task.id,
            Some(tab_id.to_string()),
            &result,
            "连接恢复后已自动清理远端断点",
            "连接恢复后自动清理远端断点失败",
        )
        .await?;
    }
    Ok(())
}

pub async fn discard(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    task_for(app, &transfer_id).await?;
    cancel_and_wait_transfer_run(app, &transfer_id).await?;
    let task = task_for(app, &transfer_id).await?;
    if task.status == "done" {
        return Ok(());
    }
    patch_task(
        app,
        &transfer_id,
        |task| {
            task.status = "canceled".to_string();
            task.message = Some("传输已取消，正在清理断点".to_string());
            task.speed = None;
            task.resumable = false;
            task.cleanup_pending = true;
        },
        PatchDelivery::PersistedEvent,
    )
    .await?;
    crate::services::logging::warn(
        app,
        &format!("transfer:{transfer_id}"),
        "canceled by user; cleaning partial data",
    );
    let cleanup = cleanup_transfer_partial(app, &task).await;
    record_cleanup_attempt(
        app,
        &transfer_id,
        None,
        &cleanup,
        "传输已取消，断点已清理",
        "传输已取消，但断点清理失败",
    )
    .await?;
    clear_transfer_progress_runtime(app, &transfer_id).await;
    Ok(())
}

pub async fn resume(app: &AppHandle, transfer_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let task = task_for(app, &transfer_id).await?;
    if !task.resumable || !can_resume_from(task.status.as_str()) {
        return Err(transfer_error("该传输没有可用断点"));
    }
    if task.cleanup_pending {
        return Err(transfer_error("该传输仍有待清理断点，不能继续"));
    }
    if state.transfer_runs.read().await.contains_key(&transfer_id) {
        return Err(transfer_error("该传输仍有活动任务，不能重复继续"));
    }
    let profile_id = task
        .profile_id
        .clone()
        .ok_or_else(|| transfer_error("传输任务缺少连接信息"))?;
    let tab_id = find_connected_tab(app, &profile_id)
        .await
        .ok_or_else(|| transfer_error("请先打开并连接原传输使用的连接，再继续任务"))?;
    if let Some(expected_mode) = task.file_access_mode.as_deref() {
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
        PatchDelivery::PersistedEvent,
    )
    .await?;
    crate::services::logging::info(app, &format!("transfer:{transfer_id}"), "resume queued");
    start(app.clone(), transfer_id).await
}

pub async fn clear(app: &AppHandle, transfer_ids: Vec<String>) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let ids = transfer_ids
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let removed_ids = {
        let mut tasks = state.transfers.write().await;
        let removed = tasks
            .iter()
            .filter(|task| {
                ids.contains(&task.id)
                    && task.terminal()
                    && !task.resumable
                    && !task.cleanup_pending
            })
            .map(|task| task.id.clone())
            .collect::<Vec<_>>();
        tasks.retain(|task| !removed.contains(&task.id));
        removed
    };
    for transfer_id in removed_ids {
        clear_transfer_progress_runtime(app, &transfer_id).await;
    }
    persist(app).await
}

pub async fn shutdown(app: &AppHandle) -> Result<(), AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let active_ids = state
        .transfers
        .read()
        .await
        .iter()
        .filter(|task| task.active())
        .map(|task| task.id.clone())
        .collect::<std::collections::HashSet<_>>();
    cancel_and_wait_all_transfer_runs(app).await?;
    let mut tasks = state.transfers.write().await;
    let active_count = active_ids.len();
    for task in tasks
        .iter_mut()
        .filter(|task| active_ids.contains(&task.id) && !task.terminal())
    {
        task.status = interrupt_status(task.resumable).to_string();
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
