pub async fn ensure_loaded(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut loaded = state.transfer_journal_loaded.lock().await;
    if *loaded {
        return Ok(());
    }
    let tasks = read_journal(app)?;
    *state.transfers.write().await = tasks.clone();
    {
        let _write = state.transfer_journal_write.lock().await;
        write_journal(app, &tasks)?;
    }
    // Publish the loaded flag only after the normalized journal is durable.
    // Otherwise a concurrent mutation can write a newer snapshot and then be
    // overwritten by this initial, stale `tasks` clone.
    *loaded = true;
    drop(loaded);
    // `retry_pending_local_cleanup` updates the now-loaded journal through the
    // normal patch path. Box this one-time continuation so the compiler does
    // not have to construct a recursively sized future for the fast-path
    // `ensure_loaded` call inside that patch path.
    Box::pin(retry_pending_local_cleanup(app)).await
}

pub async fn list(app: &AppHandle) -> Result<Vec<TransferTask>, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let transfers = state.transfers.read().await.clone();
    Ok(transfers)
}

async fn persist(app: &AppHandle) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _write = state.transfer_journal_write.lock().await;
    let tasks = state.transfers.read().await.clone();
    write_journal(app, &tasks)
}

// Transfer progress belongs to the main workspace window. Keep it separate
// from workspace snapshots so standalone editors do not rehydrate while a
// background upload or download advances.
async fn emit_task(app: &AppHandle, task: TransferTask) {
    let _ = app.emit_to(EventTarget::webview_window("main"), "transfer:update", task);
}

#[derive(Clone, Copy)]
enum PatchDelivery {
    Silent,
    Event,
    PersistedEvent,
}

async fn patch_task(
    app: &AppHandle,
    transfer_id: &str,
    patch: impl FnOnce(&mut TransferTask),
    delivery: PatchDelivery,
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
    if matches!(delivery, PatchDelivery::PersistedEvent) {
        persist(app).await?;
    }
    match delivery {
        PatchDelivery::Silent => {}
        PatchDelivery::Event | PatchDelivery::PersistedEvent => emit_task(app, task.clone()).await,
    }
    Ok(Some(task))
}

pub async fn report_progress(app: &AppHandle, transfer_id: &str, transferred: u64, total: u64) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let should_emit = {
        let mut last_events = state.transfer_last_event.lock().await;
        let now = std::time::Instant::now();
        let should_emit = progress_event_due(last_events.get(transfer_id).copied(), now);
        if should_emit {
            last_events.insert(transfer_id.to_string(), now);
        }
        should_emit
    };
    let speed = {
        let now = std::time::Instant::now();
        let mut samples = state.transfer_progress_samples.lock().await;
        match samples.get_mut(transfer_id) {
            Some(sample)
                if transferred >= sample.bytes
                    && now.saturating_duration_since(sample.sampled_at)
                        >= SPEED_SAMPLE_INTERVAL =>
            {
                let elapsed = now
                    .saturating_duration_since(sample.sampled_at)
                    .as_secs_f64();
                let bytes_per_second = (transferred - sample.bytes) as f64 / elapsed;
                sample.bytes = transferred;
                sample.sampled_at = now;
                format_transfer_speed(bytes_per_second)
            }
            Some(sample) if transferred < sample.bytes => {
                sample.bytes = transferred;
                sample.sampled_at = now;
                None
            }
            Some(_) => None,
            None => {
                samples.insert(
                    transfer_id.to_string(),
                    crate::services::workspace::TransferProgressSample {
                        bytes: transferred,
                        sampled_at: now,
                    },
                );
                None
            }
        }
    };
    let _ = patch_task(
        app,
        transfer_id,
        |task| {
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
            if let Some(speed) = speed {
                task.speed = Some(speed);
            }
            task.resumable = true;
        },
        if should_emit {
            PatchDelivery::Event
        } else {
            PatchDelivery::Silent
        },
    )
    .await;
}

pub(crate) async fn worker_call<T>(
    app: &AppHandle,
    tab_id: &str,
    make_command: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
) -> Result<T, AppError> {
    worker_call_with_timeout(
        app,
        tab_id,
        TRANSFER_WORKER_CONTROL_TIMEOUT,
        None,
        make_command,
    )
    .await
}

pub(crate) async fn worker_call_with_cancel<T>(
    app: &AppHandle,
    tab_id: &str,
    cancellation: &CancellationToken,
    make_command: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
) -> Result<T, AppError> {
    worker_call_with_timeout(
        app,
        tab_id,
        TRANSFER_WORKER_CONTROL_TIMEOUT,
        Some(cancellation.clone()),
        make_command,
    )
    .await
}

pub(crate) async fn worker_data_call_with_cancel<T>(
    app: &AppHandle,
    tab_id: &str,
    cancellation: &CancellationToken,
    make_command: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
) -> Result<T, AppError> {
    worker_call_with_timeout(
        app,
        tab_id,
        TRANSFER_WORKER_DATA_TIMEOUT,
        Some(cancellation.clone()),
        make_command,
    )
    .await
}

async fn worker_call_with_timeout<T>(
    app: &AppHandle,
    tab_id: &str,
    response_timeout: Duration,
    cancellation: Option<CancellationToken>,
    make_command: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
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
    let cancellation = cancellation.unwrap_or_default();
    let command = make_command(respond_to, cancellation.clone());
    let send_result = tokio::select! {
        _ = cancellation.cancelled() => return Err(transfer_error("传输已取消")),
        result = tokio::time::timeout(TRANSFER_WORKER_SEND_TIMEOUT, sender.send(command)) => result,
    };
    match send_result {
        Ok(Ok(())) => {}
        Ok(Err(_)) => return Err(transfer_error("传输会话已关闭")),
        Err(_) => {
            cancellation.cancel();
            return Err(transfer_error("传输操作发送超时"));
        }
    }

    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(transfer_error("传输已取消")),
        result = tokio::time::timeout(response_timeout, result) => result,
    };
    let response = match response {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => return Err(transfer_error("传输会话未返回结果")),
        Err(_) => {
            cancellation.cancel();
            return Err(transfer_error("传输操作响应超时，后台操作已取消"));
        }
    };
    response.map_err(transfer_error)
}
