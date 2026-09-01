// Remote transfer commands.
#[tauri::command]
pub async fn app_queue_upload(
    app: AppHandle,
    file_names: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::queue_upload(&app, file_names).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_upload_file(
    app: AppHandle,
    tab_id: String,
    local_path: String,
    remote_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_upload(
        &app,
        tab_id,
        local_path,
        remote_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_file(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_download(
        &app,
        tab_id,
        remote_path,
        local_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_remote_path(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    target_type: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    match target_type.as_str() {
        "file" => app_download_file(app, tab_id, remote_path, local_directory, options).await,
        "folder" => {
            crate::services::transfers::create_download_directory(
                &app,
                tab_id,
                remote_path,
                local_directory,
                target_name,
            )
            .await?;
            get_workspace_snapshot(app).await
        }
        _ => Err(AppError::Command("远端传输目标类型无效".to_string())),
    }
}

#[tauri::command]
pub async fn app_cancel_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_pause_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::pause(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_resume_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::resume(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_discard_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_clear_transfers(
    app: AppHandle,
    transfer_ids: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::clear(&app, transfer_ids).await?;
    get_workspace_snapshot(app).await
}
