// Remote file, transfer, and SSH tunnel actions.
async fn write_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let path = required_string(params, "path", 4_096)?;
    let content = required_text(params, "content", MCP_MAX_FILE_CONTENT_BYTES)?;
    let encoding = optional_string(params, "encoding", 64)?;
    let snapshot = crate::commands::app_write_remote_file(
        app.clone(),
        tab_id.clone(),
        path,
        content,
        encoding,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "write_remote_file",
    ))
}

async fn create_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let parent_path = required_string(params, "parent_path", 4_096)?;
    let name = required_string(params, "name", 512)?;
    let snapshot = crate::commands::app_create_remote_directory(
        app.clone(),
        tab_id.clone(),
        parent_path,
        name,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "create_remote_directory",
    ))
}

async fn create_remote_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let parent_path = required_string(params, "parent_path", 4_096)?;
    let name = required_string(params, "name", 512)?;
    let snapshot =
        crate::commands::app_create_remote_file(app.clone(), tab_id.clone(), parent_path, name)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "create_remote_file",
    ))
}

async fn copy_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let destination_path = required_string(params, "destination_path", 4_096)?;
    let target_type = required_target_type(params)?;
    let snapshot = crate::commands::app_copy_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        destination_path,
        target_type,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "copy_remote_path",
    ))
}

async fn move_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let destination_path = required_string(params, "destination_path", 4_096)?;
    let snapshot = crate::commands::app_move_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        destination_path,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "move_remote_path",
    ))
}

async fn rename_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let new_name = required_string(params, "new_name", 512)?;
    let snapshot =
        crate::commands::app_rename_remote_path(app.clone(), tab_id.clone(), target_path, new_name)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "rename_remote_path",
    ))
}

async fn delete_remote_path(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "target_path", 4_096)?;
    let target_type = required_target_type(params)?;
    let target_is_symlink = params
        .get("target_is_symlink")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let snapshot = crate::commands::app_delete_remote_path(
        app.clone(),
        tab_id.clone(),
        target_path,
        target_type,
        target_is_symlink,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "delete_remote_path",
    ))
}

async fn change_remote_permissions(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let target_path = required_string(params, "path", 4_096)?;
    let mode = required_string(params, "mode", 4)?;
    let options = serde_json::from_value(json!({
        "mode": mode,
        "recursive": optional_bool(params, "recursive")?.unwrap_or(false),
        "applyTo": optional_string(params, "apply_to", 32)?,
    }))
    .map_err(|error| format!("Invalid remote permission options: {error}"))?;
    let snapshot = crate::commands::app_change_remote_permissions(
        app.clone(),
        tab_id.clone(),
        target_path,
        options,
    )
    .await
    .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "change_remote_permissions",
    ))
}

async fn set_remote_file_access_mode(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let mode = required_string(params, "mode", 16)?;
    if !matches!(mode.as_str(), "user" | "root") {
        return Err("mode must be user or root".to_string());
    }
    let snapshot =
        crate::commands::app_set_remote_file_access_mode(app.clone(), tab_id.clone(), mode, None)
            .await
            .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "set_remote_file_access_mode",
    ))
}

fn transfer_options(params: &Value) -> Result<Option<Value>, String> {
    Ok(optional_string(params, "target_name", 512)?
        .map(|target_name| json!({ "targetName": target_name })))
}

async fn upload_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let local_path = required_string(params, "local_path", 4_096)?;
    let remote_directory = required_string(params, "remote_directory", 4_096)?;
    let snapshot = crate::commands::app_upload_file(
        app.clone(),
        tab_id.clone(),
        local_path,
        remote_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "upload_file",
        "upload",
    ))
}

async fn download_file(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let remote_path = required_string(params, "remote_path", 4_096)?;
    let local_directory = required_string(params, "local_directory", 4_096)?;
    let snapshot = crate::commands::app_download_file(
        app.clone(),
        tab_id.clone(),
        remote_path,
        local_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "download_file",
        "download",
    ))
}

async fn download_remote_directory(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let remote_path = required_string(params, "remote_path", 4_096)?;
    let local_directory = required_string(params, "local_directory", 4_096)?;
    let snapshot = crate::commands::app_download_remote_path(
        app.clone(),
        tab_id.clone(),
        remote_path,
        "folder".to_string(),
        local_directory,
        transfer_options(params)?,
    )
    .await
    .map_err(public_app_error)?;
    Ok(transfer_snapshot(
        &snapshot,
        &tab_id,
        "download_remote_directory",
        "download",
    ))
}

async fn transfer_action(app: &AppHandle, params: &Value, action: &str) -> Result<Value, String> {
    let transfer_id = required_string(params, "transfer_id", 256)?;
    ensure_transfer_in_mcp_scope(app, &transfer_id).await?;
    let snapshot = match action {
        "pause" => crate::commands::app_pause_transfer(app.clone(), transfer_id.clone()).await,
        "resume" => crate::commands::app_resume_transfer(app.clone(), transfer_id.clone()).await,
        "discard" => crate::commands::app_discard_transfer(app.clone(), transfer_id.clone()).await,
        _ => return Err("Unsupported transfer action".to_string()),
    }
    .map_err(public_app_error)?;
    Ok(json!({
        "operation": format!("{action}_transfer"),
        "transferId": transfer_id,
        "transfer": find_transfer(&snapshot, &transfer_id),
    }))
}

async fn clear_transfers(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let transfer_ids = required_string_array(params, "transfer_ids", 256, 256)?;
    for transfer_id in &transfer_ids {
        ensure_transfer_in_mcp_scope(app, transfer_id).await?;
    }
    let snapshot = crate::commands::app_clear_transfers(app.clone(), transfer_ids.clone())
        .await
        .map_err(public_app_error)?;
    Ok(json!({
        "operation": "clear_transfers",
        "transferIds": transfer_ids,
        "remaining": snapshot.get("transfers").cloned().unwrap_or_else(|| json!([])),
    }))
}

async fn ensure_transfer_in_mcp_scope(app: &AppHandle, transfer_id: &str) -> Result<(), String> {
    let visibility = mcp_visibility(app).await?;
    let task = crate::services::transfers::list(app)
        .await
        .map_err(public_app_error)?
        .into_iter()
        .find(|task| task.id == transfer_id)
        .ok_or_else(|| format!("{MCP_TRANSFER_NOT_FOUND}: transfer was not found"))?;
    visibility
        .allows_transfer_task(&task)
        .then_some(())
        .ok_or_else(|| {
            format!("{MCP_SCOPE_DENIED}: this Agent cannot access the requested transfer")
        })
}

async fn create_ssh_tunnel(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let rule = params
        .get("rule")
        .cloned()
        .ok_or_else(|| "rule is required".to_string())?;
    if !rule.is_object() {
        return Err("rule must be an object".to_string());
    }
    let items = crate::commands::app_create_ssh_tunnel(app.clone(), tab_id.clone(), rule)
        .await
        .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}

async fn tunnel_action(app: &AppHandle, params: &Value, action: &str) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let rule_id = required_string(params, "rule_id", 256)?;
    let items = match action {
        "start" => {
            crate::commands::app_start_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await
        }
        "stop" => crate::commands::app_stop_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await,
        "delete" => {
            crate::commands::app_delete_ssh_tunnel(app.clone(), tab_id.clone(), rule_id).await
        }
        _ => return Err("Unsupported SSH tunnel action".to_string()),
    }
    .map_err(public_app_error)?;
    Ok(json!({ "tabId": tab_id, "items": items }))
}

