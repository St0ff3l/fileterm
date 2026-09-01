// Font, SSH key, connection import/export, WebDAV, and S3 commands.
#[tauri::command]
pub fn app_list_imported_fonts(
    app: AppHandle,
) -> Result<Vec<crate::services::fonts::ImportedFont>, AppError> {
    crate::services::fonts::list(&app)
}

#[tauri::command]
pub async fn app_import_font(
    app: AppHandle,
) -> Result<Option<crate::services::fonts::ImportedFont>, AppError> {
    crate::services::fonts::import(&app).await
}

#[tauri::command]
pub fn app_get_imported_font_data(
    app: AppHandle,
    font_id: String,
) -> Result<Option<String>, AppError> {
    crate::services::fonts::data_url(&app, &font_id)
}

#[tauri::command]
pub fn app_delete_imported_font(app: AppHandle, font_id: String) -> Result<bool, AppError> {
    crate::services::fonts::delete(&app, &font_id)
}

#[tauri::command]
pub fn app_list_ssh_keys(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    crate::services::ssh_keys::list(&app)
}

#[tauri::command]
pub async fn app_select_ssh_key_file(
    app: AppHandle,
) -> Result<Option<serde_json::Value>, AppError> {
    crate::services::ssh_keys::select_file(&app).await
}

#[tauri::command]
pub fn app_import_ssh_key(
    app: AppHandle,
    input: Option<ImportSshKeyInput>,
) -> Result<Option<serde_json::Value>, AppError> {
    let input = input.unwrap_or(ImportSshKeyInput {
        source_path: None,
        content: None,
        note: None,
    });
    let result =
        crate::services::ssh_keys::import(&app, input.source_path, input.content, input.note)?;
    if result.is_some() {
        emit_ssh_keys_changed(&app)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn app_update_ssh_key_note(
    app: AppHandle,
    key_id: String,
    note: String,
) -> Result<serde_json::Value, AppError> {
    let updated = crate::services::ssh_keys::update_note(&app, &key_id, note)?;
    emit_ssh_keys_changed(&app)?;
    Ok(updated)
}

#[tauri::command]
pub fn app_delete_ssh_key(app: AppHandle, key_id: String) -> Result<(), AppError> {
    crate::services::ssh_keys::delete(&app, &key_id)?;
    emit_ssh_keys_changed(&app)
}

fn emit_ssh_keys_changed(app: &AppHandle) -> Result<(), AppError> {
    app.emit("sshKeys:changed", crate::services::ssh_keys::list(app)?)
        .map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_preview_connection_import(
    app: AppHandle,
    source: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("Connection files", &["json", "config", "txt"])
        .set_title("选择连接配置或目录");
    let paths = match source.as_deref() {
        Some("folder") => dialog
            .pick_folder()
            .await
            .map(|folder| vec![folder.path().to_path_buf()]),
        Some("files") | None => dialog.pick_files().await.map(|files| {
            files
                .into_iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        }),
        _ => return Err(AppError::Command("导入来源无效".to_string())),
    };
    let Some(paths) = paths else {
        return Ok(None);
    };
    crate::services::connections::create_import_plan_from_paths(&app, paths)
        .await
        .map(Some)
}

#[tauri::command]
pub async fn app_commit_connection_json_import(
    app: AppHandle,
    plan_id: String,
    options: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let selected_ids = options
        .get("selectedItemIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let strategy = options
        .get("conflictStrategy")
        .and_then(Value::as_str)
        .unwrap_or("skip");
    crate::services::connections::commit_import_plan(&app, &plan_id, &selected_ids, strategy).await
}

#[tauri::command]
pub async fn app_export_connections(app: AppHandle, format: String) -> Result<bool, AppError> {
    let extension = if format == "compatible" {
        "json"
    } else {
        "fileterm.json"
    };
    let Some(target) = rfd::AsyncFileDialog::new()
        .set_file_name(format!("fileterm-connections.{extension}"))
        .add_filter("JSON", &["json"])
        .save_file()
        .await
    else {
        return Ok(false);
    };
    let bytes = crate::services::connections::export_bundle(&app, &format)?;
    tokio::fs::write(target.path(), bytes)
        .await
        .map_err(|error| AppError::Storage(format!("无法写入导出文件: {error}")))?;
    Ok(true)
}

#[tauri::command]
pub async fn app_export_connections_as_files(
    app: AppHandle,
    format: String,
) -> Result<bool, AppError> {
    let Some(target) = rfd::AsyncFileDialog::new().pick_folder().await else {
        return Ok(false);
    };
    let (profiles, _) = crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let mut used_names = std::collections::HashSet::new();
    for profile in profiles {
        let id = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("connection");
        let name = profile.get("name").and_then(Value::as_str).unwrap_or(id);
        let filename = format!(
            "{}.json",
            crate::services::connections::export_filename(name, id, &mut used_names)
        );
        let payload = if format == "compatible" {
            crate::services::connections::build_compatible_profile_payload(&profile)
        } else {
            serde_json::json!({
                "schemaVersion": 1,
                "generatedAt": crate::services::webdav::export_timestamp(),
                "profiles": [profile],
            })
        };
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        tokio::fs::write(target.path().join(filename), bytes)
            .await
            .map_err(|error| AppError::Storage(format!("无法写入单连接导出: {error}")))?;
    }
    Ok(true)
}

#[tauri::command]
pub fn app_get_webdav_sync_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::get_config(&app)
}

#[tauri::command]
pub fn app_set_webdav_sync_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::save_config(&app, input)
}

#[tauri::command]
pub async fn app_test_webdav_sync(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::test_connection(&app).await
}

#[tauri::command]
pub async fn app_upload_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::upload(&app, mode.as_deref()).await
}

#[tauri::command]
pub async fn app_download_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let result = crate::services::webdav::download(&app, mode.as_deref()).await?;
    let changed = result.get("imported").and_then(Value::as_u64).unwrap_or(0)
        + result.get("updated").and_then(Value::as_u64).unwrap_or(0);
    if changed > 0 || result.get("mode").and_then(Value::as_str) == Some("overwrite-local") {
        if let Ok(snapshot) = get_workspace_snapshot_unlocked(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn app_get_s3_backup_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::get_config(&app)
}

#[tauri::command]
pub fn app_set_s3_backup_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::save_config(&app, input)
}

#[tauri::command]
pub async fn app_test_s3_backup(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::test_connection(&app).await
}

#[tauri::command]
pub async fn app_upload_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::upload(&app, mode.as_deref()).await
}

#[tauri::command]
pub async fn app_download_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let result = crate::services::s3_backup::download(&app, mode.as_deref()).await?;
    let changed = result.get("imported").and_then(Value::as_u64).unwrap_or(0)
        + result.get("updated").and_then(Value::as_u64).unwrap_or(0);
    if changed > 0 || result.get("mode").and_then(Value::as_str) == Some("overwrite-local") {
        if let Ok(snapshot) = get_workspace_snapshot_unlocked(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(result)
}
