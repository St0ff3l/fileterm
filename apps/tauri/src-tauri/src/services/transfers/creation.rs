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
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let metadata = tokio::fs::metadata(&local_path)
        .await
        .map_err(|error| transfer_error(format!("无法读取本地上传文件: {error}")))?;
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
                let entry_staging =
                    (file_access_mode == "root").then(|| root_staging_path(&relative_path));
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
            retry_attempt: None,
            cleanup_pending: false,
            created_at: Some(now),
            updated_at: Some(now),
        };
        state.transfers.write().await.push(task.clone());
        persist(app).await?;
        emit_task(app, task.clone()).await;
        start(app.clone(), task.id).await?;
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
        retry_attempt: None,
        cleanup_pending: false,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone()).await;
    start(app.clone(), task.id).await?;
    Ok(())
}

pub async fn create_download(
    app: &AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    target_name: Option<String>,
) -> Result<String, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
    let tab = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .cloned()
        .ok_or_else(|| transfer_error("目标标签页不存在"))?;
    let size = worker_call(app, &tab_id, |respond_to, cancellation| {
        WorkerCmd::StatRemoteFile {
            path: remote_path.clone(),
            cancellation,
            respond_to,
        }
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
        retry_attempt: None,
        cleanup_pending: false,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone()).await;
    let task_id = task.id.clone();
    start(app.clone(), task_id.clone()).await?;
    Ok(task_id)
}

pub async fn create_download_directory(
    app: &AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    target_name: Option<String>,
) -> Result<String, AppError> {
    ensure_loaded(app).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _lifecycle = state.transfer_lifecycle.lock().await;
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
        retry_attempt: None,
        cleanup_pending: false,
        created_at: Some(now),
        updated_at: Some(now),
    };
    state.transfers.write().await.push(task.clone());
    persist(app).await?;
    emit_task(app, task.clone()).await;
    let task_id = task.id.clone();
    start(app.clone(), task_id.clone()).await?;
    Ok(task_id)
}
