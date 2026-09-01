// Terminal and remote-path opening commands.
#[tauri::command]
pub async fn app_open_local_terminal(
    app: AppHandle,
    options: Option<crate::sessions::local_terminal::LocalTerminalLaunchOptions>,
) -> Result<serde_json::Value, AppError> {
    crate::services::logging::info(
        &app,
        "local",
        format!(
            "open command received options_present={}",
            options.is_some()
        ),
    );
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut launch_options = options.unwrap_or_default();
    if launch_options.shell.is_none() {
        let preferences = app_get_ui_preferences(app.clone())?;
        launch_options.shell = Some(current_local_terminal_shell(&preferences));
    }
    let launch = match crate::sessions::local_terminal::resolve_launch(Some(launch_options)) {
        Ok(launch) => launch,
        Err(error) => {
            crate::services::logging::error(
                &app,
                "local",
                format!("launch resolution failed error={error}"),
            );
            return Err(AppError::Command(error));
        }
    };
    // Start the PTY asynchronously so opening a local terminal does not block
    // the button for the readiness timeout. Every snapshot carries a
    // monotonic revision, so a connected workspace event cannot be overwritten
    // by this command's earlier connecting response when IPC delivery crosses.
    let tab_id = spawn_local_terminal_tab(&app, &state, launch, None).await;
    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(tab_id);
    }
    // The renderer replaces the active home tab with the returned session in
    // the same turn. Emitting here races that replacement and briefly exposes
    // the new session as an additional tab before the old placeholder closes.
    let snapshot = get_workspace_snapshot(app.clone()).await;
    if let Ok(snapshot) = &snapshot {
        let workspace_revision = snapshot
            .get("workspaceRevision")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        crate::services::logging::session(
            &app,
            "INFO",
            "local",
            snapshot
                .get("activeTabId")
                .and_then(Value::as_str)
                .unwrap_or("<unknown>"),
            format!("open completed with current workspace snapshot revision={workspace_revision}"),
        );
    }
    snapshot
}

#[tauri::command]
pub async fn app_write_terminal(
    app: AppHandle,
    tab_id: String,
    data: String,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let data_len = data.len();
    let result = send_terminal_input(&state, &tab_id, data).await;
    if let Err(error) = &result {
        crate::services::logging::warn(
            &app,
            "terminal",
            format!(
                "write failed tab={} bytes={} error={error}",
                tab_id, data_len
            ),
        );
    }
    result
}

#[tauri::command]
pub fn app_subscribe_terminal_data(app: AppHandle, channel: Channel<serde_json::Value>) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let channel_id = channel.id();
    state.register_terminal_output_channel(channel);
    crate::services::logging::debug(
        &app,
        "terminal",
        format!("terminal output channel registered id={channel_id}"),
    );
}

#[tauri::command]
pub async fn app_resize_terminal(
    app: AppHandle,
    tab_id: String,
    cols: u32,
    rows: u32,
    width: u32,
    height: u32,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = state.workers.read().await.get(&tab_id).cloned();
    if let Some(sender) = sender {
        match timeout(
            WORKER_CMD_SEND_TIMEOUT,
            sender.send(WorkerCmd::ResizeTerminal {
                cols,
                rows,
                width,
                height,
            }),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => crate::services::logging::warn(
                &app,
                "terminal",
                format!("resize delivery failed tab={} error={error}", tab_id),
            ),
            Err(_) => crate::services::logging::warn(
                &app,
                "terminal",
                format!(
                    "resize delivery timed out tab={} cols={} rows={}",
                    tab_id, cols, rows
                ),
            ),
        }
    } else if state
        .tabs
        .read()
        .await
        .iter()
        .any(|tab| tab.id == tab_id && tab.session_type == "local")
    {
        crate::services::logging::warn(
            &app,
            "local",
            format!("resize dropped because PTY worker is missing tab={tab_id}"),
        );
    }
    Ok(())
}
