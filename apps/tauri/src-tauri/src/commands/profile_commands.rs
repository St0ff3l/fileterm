// Profile, folder, and command-template commands.
// ==========================================
// Phase 2 commands: profile / folder / command CRUD
// ==========================================
//
// These commands delegate to `services::profile_ops`, which mirrors the
// Electron `FileProfileRepository` semantics (group/parentId self-healing,
// secrets stripping, cascade rename / delete, ordering).

#[tauri::command]
pub async fn app_create_profile(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::create_profile(&app, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_profile(
    app: AppHandle,
    profile_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let profile = crate::services::profile_ops::update_profile(&app, &profile_id, input)?;
    let resolved_profile = resolve_profile_for_session(&app, &profile)?;
    let reconnect_mode = crate::services::workspace::reconnect_mode_for_profile(&resolved_profile);
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    for session in sessions.values_mut() {
        if session.profile_id == profile_id {
            session.reconnect_mode = reconnect_mode.clone();
        }
    }
    drop(sessions);
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_clear_trusted_host_fingerprint(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::clear_trusted_host_fingerprint(&app, &profile_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_test_connection(
    app: AppHandle,
    window: WebviewWindow,
    profile_id: Option<String>,
    input: serde_json::Value,
) -> Result<(), AppError> {
    let library_guard = lock_library_after_transfer_hydration(&app).await?;
    let profile = crate::services::profile_ops::profile_for_connection_test(
        &app,
        profile_id.as_deref(),
        input,
    )?;
    let resolved_profile = resolve_profile_for_session(&app, &profile)?;
    drop(library_guard);

    // A connection test can remain in the SSH handshake while it waits for a
    // host-key decision. Guard it in Rust as well as in the renderer: the
    // standalone form and the main window are separate WebViews and can both
    // invoke this command before either one observes the other's busy state.
    // The key deliberately contains no credentials.
    let test_key = profile_id
        .as_deref()
        .map(|id| format!("profile:{id}"))
        .unwrap_or_else(|| {
            let host = resolved_profile
                .get("host")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let port = resolved_profile
                .get("port")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            let username = resolved_profile
                .get("username")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            format!("endpoint:{username}@{host}:{port}")
        });
    let connection_tests_in_flight = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_tests_in_flight
        .clone();
    {
        let mut active_tests = connection_tests_in_flight.lock().await;
        if !active_tests.insert(test_key.clone()) {
            return Err(AppError::Command(
                "Connection test already in progress".to_string(),
            ));
        }
    }

    let connection_tests_last_started = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_tests_last_started
        .clone();
    let now = Instant::now();
    let cooldown_error = {
        let mut last_started = connection_tests_last_started.lock().await;
        if let Some(started_at) = last_started.get(&test_key) {
            if started_at.elapsed() < CONNECTION_TEST_RETRY_COOLDOWN {
                Some("Connection test cooldown active; please wait before retrying".to_string())
            } else {
                last_started.insert(test_key.clone(), now);
                None
            }
        } else {
            last_started.insert(test_key.clone(), now);
            None
        }
    };
    if let Some(error) = cooldown_error {
        connection_tests_in_flight.lock().await.remove(&test_key);
        return Err(AppError::Command(error));
    }

    let test_tab_id = format!("connection-test-{}", uuid::Uuid::new_v4());
    let profile_type = resolved_profile
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ssh");
    let interaction_window_label = window.label().to_string();
    let result = match profile_type {
        "ssh" => {
            crate::sessions::ssh::test_connection(
                &app,
                &resolved_profile,
                &test_tab_id,
                interaction_window_label,
            )
            .await
        }
        "ftp" => crate::sessions::ftp::test_connection(&resolved_profile).await,
        "telnet" => crate::sessions::telnet::test_connection(&resolved_profile).await,
        "serial" => {
            crate::sessions::serial::test_connection(&app, &resolved_profile, &test_tab_id).await
        }
        other => Err(format!("Unsupported connection type: {other}")),
    };
    match &result {
        Ok(()) => crate::services::logging::session(
            &app,
            "INFO",
            "connection-test",
            &test_tab_id,
            format!("connection test command completed type={profile_type}"),
        ),
        Err(error) => crate::services::logging::session(
            &app,
            "ERROR",
            "connection-test",
            &test_tab_id,
            format!("connection test command failed type={profile_type} error={error}"),
        ),
    }
    connection_tests_in_flight.lock().await.remove(&test_key);
    result.map_err(AppError::Command)
}

#[tauri::command]
pub async fn app_delete_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_profile(&app, &profile_id)?;
    if let Err(error) = clear_deleted_profile_from_mcp_policy(&app, &profile_id) {
        crate::services::logging::warn(
            &app,
            "ui-preferences",
            format!("failed to clean deleted profile from MCP policy: {error}"),
        );
    }
    get_workspace_snapshot_and_emit(&app).await
}

fn clear_deleted_profile_from_mcp_policy(
    app: &AppHandle,
    profile_id: &str,
) -> Result<(), AppError> {
    let mut preferences = app_get_ui_preferences(app.clone())?;
    let original_len = preferences.mcp_agent.allowed_profile_ids.len();
    preferences
        .mcp_agent
        .allowed_profile_ids
        .retain(|allowed_id| allowed_id != profile_id);
    if preferences.mcp_agent.allowed_profile_ids.len() == original_len {
        return Ok(());
    }

    let path = crate::storage::state_path(app)?;
    let content = serde_json::to_string_pretty(&preferences)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(path, content).map_err(|error| AppError::Storage(error.to_string()))?;
    let _ = app.emit("app:ui-preferences-changed", &preferences);
    Ok(())
}

#[tauri::command]
pub async fn app_update_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_entity_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_entity_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_template(
    app: AppHandle,
    command_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_template(&app, &command_id, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_template(
    app: AppHandle,
    command_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_template(&app, &command_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

/// Render and send a command template to an active SSH session.
///
/// This intentionally performs the rendering in the main process: the command
/// source is durable storage, while the renderer only supplies positional
/// arguments and whether the final carriage return is desired. It mirrors the
/// Electron workspace service and keeps arbitrary command text out of the IPC
/// surface.
#[tauri::command]
pub async fn app_execute_command_template(
    app: AppHandle,
    tab_id: String,
    command_id: String,
    args: Option<Vec<String>>,
    options: Option<Value>,
) -> Result<Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
    };
    if session_type.as_deref() != Some("ssh") {
        return Err(AppError::Command("只有 SSH 会话支持快捷命令".to_string()));
    }

    let commands = read_json_array(&app, "commands.json")?;
    let command = commands
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(command_id.as_str()))
        .ok_or_else(|| AppError::Storage(format!("Command not found: {command_id}")))?;
    let template = command
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Storage(format!("Command is invalid: {command_id}")))?;
    let rendered_command = render_command_template(template, args.as_deref().unwrap_or_default());
    let append_carriage_return = options
        .as_ref()
        .and_then(|value| value.get("appendCarriageReturn"))
        .and_then(Value::as_bool)
        .or_else(|| command.get("appendCarriageReturn").and_then(Value::as_bool))
        .unwrap_or(true);

    let payload = if append_carriage_return {
        format!("{rendered_command}\r")
    } else {
        rendered_command.clone()
    };
    send_terminal_input(&state, &tab_id, payload).await?;

    Ok(serde_json::json!({ "renderedCommand": rendered_command }))
}

fn render_command_template(template: &str, args: &[String]) -> String {
    // `[p#1]` is the durable command-template placeholder format shared with
    // Electron. Invalid/out-of-range references deliberately render as an
    // empty string so existing command libraries retain their behavior.
    let placeholder = Regex::new(r"\[p#(\d+)\]").expect("constant placeholder regex must compile");
    placeholder
        .replace_all(template, |captures: &regex::Captures<'_>| {
            captures
                .get(1)
                .and_then(|index| index.as_str().parse::<usize>().ok())
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| args.get(index))
                .cloned()
                .unwrap_or_default()
        })
        .into_owned()
}
