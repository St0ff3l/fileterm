#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopySource {
    pub profile_id: String,
    pub tab_id: String,
    pub name: String,
    pub target_name: String,
    pub source_endpoint: String,
    pub target_endpoint: String,
    pub replace_existing: bool,
}

pub async fn browse_copy_directory(
    app: &AppHandle,
    tab_id: &str,
    path: &str,
) -> Result<Vec<Value>, AppError> {
    copy_session(app, tab_id).await?;
    validate_copy_path(path)?;
    worker_call(app, tab_id, |respond_to, cancellation| {
        WorkerCmd::ListRemoteFiles {
            path: path.into(),
            cancellation,
            respond_to,
        }
    })
    .await
}

fn validate_copy_path(path: &str) -> Result<(), AppError> {
    if !path.starts_with('/')
        || path.contains(['\0', '\r', '\n'])
        || path.split('/').any(|part| part == "..")
    {
        return Err(transfer_error("传输路径必须是无父目录跳转的绝对路径"));
    }
    Ok(())
}

async fn copy_session(app: &AppHandle, tab_id: &str) -> Result<(String, String, String), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session = state
        .sessions
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("请先连接服务器"))?;
    let tab = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("连接标签不存在"))?;
    if !session.connected
        || session.file_access_mode != "user"
        || !matches!(tab.session_type.as_str(), "ssh" | "sftp" | "ftp")
    {
        return Err(transfer_error("请选择已连接的普通用户 SFTP/FTP 会话"));
    }
    let endpoint = session
        .transfer_endpoint
        .ok_or_else(|| transfer_error("连接缺少已建立的文件端点身份，请重新连接"))?;
    Ok((session.profile_id, tab.title, endpoint))
}

fn ensure_copy_endpoint(expected: &str, connected: &str) -> Result<(), AppError> {
    if expected != connected {
        return Err(transfer_error("连接端点已改变，请新建传输任务"));
    }
    Ok(())
}

async fn copy_entry_metadata(
    app: &AppHandle,
    tab: &str,
    path: &str,
    cancel: &CancellationToken,
) -> Result<Option<Value>, AppError> {
    let entries = worker_call_with_cancel(app, tab, cancel, |respond_to, cancellation| {
        WorkerCmd::ListRemoteFiles {
            path: parent_remote_path(path),
            cancellation,
            respond_to,
        }
    })
    .await?;
    let name = task_name(path);
    Ok(entries
        .into_iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(&name)))
}

#[allow(clippy::too_many_arguments)]
pub async fn create_remote_copy(
    app: &AppHandle,
    source_tab: String,
    target_tab: String,
    source_path: String,
    target_directory: String,
    replace_existing: bool,
) -> Result<String, AppError> {
    ensure_loaded(app).await?;
    validate_copy_path(&source_path)?;
    validate_copy_path(&target_directory)?;
    let source = copy_session(app, &source_tab).await?;
    let target = copy_session(app, &target_tab).await?;
    if source_tab == target_tab || source.0 == target.0 || source.2 == target.2 {
        return Err(transfer_error(
            "跨服务器复制请选择不同的连接；同服务器请使用原有复制功能",
        ));
    }
    let name = download_target_name(&source_path, None, false)?;
    let destination_path = join_remote_path(&target_directory, &name);
    let cancel = CancellationToken::new();
    let metadata = copy_entry_metadata(app, &source_tab, &source_path, &cancel)
        .await?
        .ok_or_else(|| transfer_error("源路径不存在"))?;
    if metadata
        .get("isSymlink")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(transfer_error("跨服务器复制不跟随符号链接"));
    }
    let folder = metadata.get("type").and_then(Value::as_str) == Some("folder");
    let id = format!("transfer-{}", uuid::Uuid::new_v4());
    let now = now_ms();
    let task = TransferTask {
        id: id.clone(),
        direction: "copy".into(),
        name,
        progress: 0.0,
        status: "queued".into(),
        message: Some("等待扫描源文件".into()),
        speed: None,
        transferred_bytes: Some(0),
        total_bytes: None,
        tab_id: Some(target_tab),
        profile_id: Some(target.0),
        session_type: None,
        file_access_mode: Some("user".into()),
        target_type: Some(if folder { "folder" } else { "file" }.into()),
        source_path: Some(source_path),
        destination_path: Some(destination_path),
        partial_path: None,
        staging_path: None,
        source_identity: None,
        manifest: None,
        conflict_policy: None,
        copy_source: Some(CopySource {
            profile_id: source.0,
            tab_id: source_tab,
            name: source.1,
            target_name: target.1,
            source_endpoint: source.2,
            target_endpoint: target.2,
            replace_existing,
        }),
        resumable: true,
        retry_attempt: None,
        cleanup_pending: false,
        created_at: Some(now),
        updated_at: Some(now),
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    {
        let mut tasks = state.transfers.write().await;
        ensure_copy_target_is_free(&tasks, &task)?;
        tasks.push(task.clone());
    }
    persist(app).await?;
    emit_task(app, &task).await;
    start(app.clone(), id.clone()).await?;
    Ok(id)
}

fn ensure_copy_target_is_free(tasks: &[TransferTask], next: &TransferTask) -> Result<(), AppError> {
    if next.direction == "download" {
        return Ok(());
    }
    let Some(destination) = next.destination_path.as_deref() else {
        return Ok(());
    };
    let conflict = tasks.iter().any(|task| {
        (task.direction == "copy" || next.direction == "copy")
            && matches!(task.direction.as_str(), "upload" | "copy")
            && task.profile_id == next.profile_id
            && (task.active() || task.resumable || task.cleanup_pending)
            && task.destination_path.as_deref().is_some_and(|path| {
                path == destination
                    || path.starts_with(&format!("{destination}/"))
                    || destination.starts_with(&format!("{path}/"))
            })
    });
    if conflict {
        Err(transfer_error(
            "目标已有未完成的传输任务，请完成或丢弃该任务后重试",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/copy_tests.rs"]
mod copy_tests;
