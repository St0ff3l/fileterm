fn prepare_conflict_task(
    task: &mut TransferTask,
    policy: Option<TransferConflictPolicy>,
) -> Result<(), AppError> {
    let Some(policy) = policy else {
        return Ok(());
    };
    if task.file_access_mode.as_deref() == Some("root") {
        return Err(transfer_error("显式冲突策略仅支持普通用户文件模式"));
    }
    task.conflict_policy = Some(policy);
    if let Some(manifest) = &mut task.manifest {
        for entry in &mut manifest.files {
            entry.partial_path = format!("{}.{}.fileterm-part", entry.destination_path, task.id);
        }
    } else if let Some(destination) = &task.destination_path {
        task.partial_path = Some(format!("{destination}.{}.fileterm-part", task.id));
    }
    Ok(())
}

async fn skip_existing_transfer_target(
    app: &AppHandle,
    tab: &str,
    direction: &str,
    destination: &str,
    policy: Option<TransferConflictPolicy>,
    cancel: &CancellationToken,
) -> Result<bool, AppError> {
    if policy != Some(TransferConflictPolicy::Skip) {
        return Ok(false);
    }
    if direction == "upload" {
        Ok(copy_entry_metadata(app, tab, destination, cancel)
            .await?
            .is_some()
            || stat_remote_transfer_size(app, tab, destination, Some(cancel))
                .await?
                .is_some())
    } else {
        match tokio::fs::symlink_metadata(destination).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(transfer_error(error.to_string())),
        }
    }
}

async fn commit_remote_transfer(
    app: &AppHandle,
    tab: &str,
    partial: &str,
    destination: &str,
    policy: TransferConflictPolicy,
) -> Result<bool, AppError> {
    // Pause/discard must wait for replacement or rollback to settle, not interrupt it midway.
    worker_data_call_with_cancel(
        app,
        tab,
        &CancellationToken::new(),
        |respond_to, cancellation| WorkerCmd::CommitRemoteFile {
            partial_path: partial.into(),
            destination_path: destination.into(),
            replace_existing: policy == TransferConflictPolicy::Replace,
            cancellation,
            respond_to,
        },
    )
    .await
}

async fn commit_local_transfer(
    partial: &Path,
    destination: &Path,
    policy: Option<TransferConflictPolicy>,
) -> Result<bool, AppError> {
    if policy != Some(TransferConflictPolicy::Skip) {
        replace_local_file(partial, destination).await?;
        return Ok(true);
    }
    if !tokio::fs::symlink_metadata(partial)
        .await
        .map_err(|e| transfer_error(e.to_string()))?
        .is_file()
    {
        return Err(transfer_error("提交断点不是普通文件"));
    }
    // Creating a hard link never replaces an existing destination, including a dangling symlink.
    match tokio::fs::hard_link(partial, destination).await {
        Ok(()) => {
            tokio::fs::remove_file(partial)
                .await
                .map_err(|e| transfer_error(e.to_string()))?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(transfer_error(format!(
            "无法安全提交文件，断点已保留：{error}"
        ))),
    }
}

#[cfg(test)]
#[path = "tests/conflict_policy_tests.rs"]
mod conflict_policy_tests;
