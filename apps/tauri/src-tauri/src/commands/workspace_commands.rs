// UI state, command history, snapshot, and connection-library commands.
#[tauri::command]
pub fn app_get_ui_state_item(app: AppHandle, key: String) -> Result<Option<String>, AppError> {
    Ok(read_ui_state(&app)?
        .get(&key)
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

#[tauri::command]
pub fn app_set_ui_state_item(app: AppHandle, key: String, value: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.insert(key, Value::String(value));
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_remove_ui_state_item(app: AppHandle, key: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.remove(&key);
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_get_terminal_command_history(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<TerminalCommandHistoryEntry>, AppError> {
    let value = crate::storage::read_json_object(&app, "command-history.json")?;
    Ok(value
        .get(&profile_id)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<TerminalCommandHistoryEntry>(entry).ok())
        .filter(|entry| !entry.command.trim().is_empty())
        .collect())
}

#[tauri::command]
pub fn app_set_terminal_command_history(
    app: AppHandle,
    profile_id: String,
    entries: Vec<TerminalCommandHistoryEntry>,
) -> Result<(), AppError> {
    let mut value = crate::storage::read_json_object(&app, "command-history.json")?;
    let sanitized = entries
        .into_iter()
        .filter(|entry| !entry.command.trim().is_empty())
        .take(500)
        .collect::<Vec<_>>();
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("命令历史文件格式无效".to_string()))?;
    object.insert(
        profile_id,
        serde_json::to_value(sanitized)
            .map_err(|error| AppError::Serialization(error.to_string()))?,
    );
    write_json_object(&app, "command-history.json", &value)
}

#[tauri::command]
pub fn app_get_command_send_preferences(
    app: AppHandle,
) -> Result<CommandSendPreferences, AppError> {
    let value = crate::storage::read_json_object(&app, "command-send-preferences.json")?;
    let preferences = serde_json::from_value::<CommandSendPreferences>(value).unwrap_or_default();
    Ok(CommandSendPreferences {
        send_scope: match preferences.send_scope.as_str() {
            "current" | "all-ssh" | "selected-ssh" => preferences.send_scope,
            _ => "current".to_string(),
        },
        ..preferences
    })
}

#[tauri::command]
pub fn app_set_command_send_preferences(
    app: AppHandle,
    preferences: CommandSendPreferences,
) -> Result<(), AppError> {
    if !matches!(
        preferences.send_scope.as_str(),
        "current" | "all-ssh" | "selected-ssh"
    ) {
        return Err(AppError::Command("命令发送范围无效".to_string()));
    }
    let selected_tab_ids = preferences
        .selected_tab_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .take(200)
        .collect::<Vec<_>>();
    write_json_object(
        &app,
        "command-send-preferences.json",
        &serde_json::to_value(CommandSendPreferences {
            selected_tab_ids,
            ..preferences
        })
        .map_err(|error| AppError::Serialization(error.to_string()))?,
    )
}

async fn lock_library_after_transfer_hydration(
    app: &AppHandle,
) -> Result<tokio::sync::OwnedMutexGuard<()>, AppError> {
    // Transfer hydration can emit a cleanup snapshot. Finish it before taking
    // the library lock so that nested snapshot cannot wait on this same lock.
    crate::services::transfers::ensure_loaded(app).await?;
    Ok(app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone()
        .lock_owned()
        .await)
}

#[tauri::command]
pub async fn app_get_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_get_connection_library(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let library_mutation = app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone();
    let _guard = library_mutation.lock().await;
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "profiles": profiles,
        "folders": folders,
    }))
}
