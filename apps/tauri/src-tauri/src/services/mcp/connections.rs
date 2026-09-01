// Connection open/wait, session lifecycle, and remote command actions.
async fn open_connection(
    app: &AppHandle,
    params: &Value,
    source: WorkspaceSessionSource,
    progress_sender: Option<mpsc::UnboundedSender<BridgeProgress>>,
    progress_token: Option<Value>,
) -> Result<Value, String> {
    let profile_id = required_string(params, "profile_id", 256)?;
    let execution_mode = requested_execution_mode(params)?;
    let (operation, created) = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_operations
        .begin_or_join(profile_id.clone())
        .await;
    let (tab_id, snapshot) = if created {
        match crate::commands::app_open_profile_with_operation(
            app.clone(),
            profile_id,
            operation.id.clone(),
            execution_mode == EXECUTION_MODE_BACKGROUND,
            source,
        )
        .await
        {
            Ok((tab_id, snapshot)) => (Some(tab_id), snapshot),
            Err(error) => {
                app.state::<crate::services::workspace::WorkspaceState>()
                    .connection_operations
                    .fail_for_operation(&operation.id, FILETERM_CONNECTION_FAILED)
                    .await;
                return Err(public_app_error(error));
            }
        }
    } else {
        let info = app
            .state::<crate::services::workspace::WorkspaceState>()
            .connection_operations
            .info(&operation.id)
            .await
            .map_err(|error| format!("{MCP_CONNECTION_OPERATION_NOT_FOUND}: {error}"))?;
        let snapshot = crate::commands::get_workspace_snapshot(app.clone())
            .await
            .map_err(public_app_error)?;
        (info.tab_id, snapshot)
    };
    let wait_for_ready = optional_bool(params, "wait_for_ready")?.unwrap_or(true);
    if !wait_for_ready {
        let status = match operation.receiver.borrow().clone() {
            ConnectionOperationState::Connected => "connected",
            ConnectionOperationState::Pending | ConnectionOperationState::Connecting => {
                "connecting"
            }
            ConnectionOperationState::Failed { code } => {
                return Err(format!(
                    "{code}: FileTerm could not establish the saved connection (operation_id={})",
                    operation.id
                ));
            }
        };
        return Ok(with_execution_mode(
            connection_operation_result(
                compact_snapshot(&snapshot, tab_id.as_deref(), "open_connection"),
                &operation.id,
                status,
                false,
            ),
            execution_mode,
        ));
    }
    let result = wait_for_connection_operation(
        app,
        &operation.id,
        params,
        progress_sender,
        progress_token,
        "open_connection",
    )
    .await?;
    Ok(with_execution_mode(result, execution_mode))
}

fn requested_execution_mode(params: &Value) -> Result<&'static str, String> {
    match optional_string(params, "execution_mode", 32)? {
        None => Ok(EXECUTION_MODE_BACKGROUND),
        Some(mode) if mode == EXECUTION_MODE_BACKGROUND => Ok(EXECUTION_MODE_BACKGROUND),
        Some(mode) if mode == EXECUTION_MODE_VISIBLE_TERMINAL => {
            Ok(EXECUTION_MODE_VISIBLE_TERMINAL)
        }
        Some(_) => Err(format!(
            "execution_mode must be {EXECUTION_MODE_BACKGROUND} or {EXECUTION_MODE_VISIBLE_TERMINAL}"
        )),
    }
}

async fn wait_for_connection(
    app: &AppHandle,
    params: &Value,
    progress_sender: Option<mpsc::UnboundedSender<BridgeProgress>>,
    progress_token: Option<Value>,
) -> Result<Value, String> {
    let operation_id = required_string(params, "operation_id", 256)?;
    wait_for_connection_operation(
        app,
        &operation_id,
        params,
        progress_sender,
        progress_token,
        "wait_for_connection",
    )
    .await
}

async fn wait_for_connection_operation(
    app: &AppHandle,
    operation_id: &str,
    params: &Value,
    progress_sender: Option<mpsc::UnboundedSender<BridgeProgress>>,
    progress_token: Option<Value>,
    operation_name: &str,
) -> Result<Value, String> {
    let timeout_ms = optional_u64(params, "timeout_ms")?.unwrap_or(MCP_CONNECTION_WAIT_DEFAULT_MS);
    if !(1_000..=MCP_CONNECTION_WAIT_MAX_MS).contains(&timeout_ms) {
        return Err(format!(
            "timeout_ms must be between 1000 and {MCP_CONNECTION_WAIT_MAX_MS}"
        ));
    }

    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

    let info = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_operations
        .info(operation_id)
        .await
        .map_err(|error| format!("{MCP_CONNECTION_OPERATION_NOT_FOUND}: {error}"))?;
    if let Some(sender) = progress_sender {
        let _ = sender.send(BridgeProgress::connection_waiting(progress_token));
    }

    let mut tab_id = info.tab_id;
    let mut receiver = info.receiver;
    loop {
        let state = {
            let borrowed_state = receiver.borrow();
            borrowed_state.clone()
        };
        match state {
            ConnectionOperationState::Connected => {
                let Some(tab_id) = tab_id.as_deref() else {
                    // The registry publishes Connecting only after attaching
                    // the tab, but keep this path defensive for a future
                    // operation source that may complete without a tab.
                    return Err(format!(
                        "{MCP_CONNECTION_OPERATION_NOT_READY}: connection worker has no session tab"
                    ));
                };
                let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                    .await
                    .map_err(public_app_error)?;
                return Ok(connection_operation_result(
                    compact_snapshot(&snapshot, Some(tab_id), operation_name),
                    operation_id,
                    "connected",
                    false,
                ));
            }
            ConnectionOperationState::Failed { code } => {
                return Err(format!(
                    "{code}: FileTerm could not establish the saved connection (operation_id={operation_id})"
                ));
            }
            ConnectionOperationState::Pending | ConnectionOperationState::Connecting => {}
        }

        match tokio::time::timeout_at(deadline, receiver.changed()).await {
            Ok(Ok(())) => {
                if tab_id.is_none() {
                    tab_id = app
                        .state::<crate::services::workspace::WorkspaceState>()
                        .connection_operations
                        .info(operation_id)
                        .await
                        .map_err(|error| format!("{MCP_CONNECTION_OPERATION_NOT_FOUND}: {error}"))?
                        .tab_id;
                }
            }
            Ok(Err(_)) => {
                return Err(format!(
                    "{FILETERM_CONNECTION_FAILED}: connection operation ended unexpectedly (operation_id={operation_id})"
                ));
            }
            Err(_) => {
                let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                    .await
                    .map_err(public_app_error)?;
                let compact = tab_id
                    .as_deref()
                    .map(|tab_id| compact_snapshot(&snapshot, Some(tab_id), operation_name))
                    .unwrap_or_else(|| json!({ "operation": operation_name, "session": null }));
                return Ok(connection_operation_result(
                    compact,
                    operation_id,
                    "connecting",
                    true,
                ));
            }
        }
    }
}

fn connection_operation_result(
    mut result: Value,
    operation_id: &str,
    status: &str,
    timed_out: bool,
) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "connectionOperationId".to_string(),
            Value::String(operation_id.to_string()),
        );
        object.insert(
            "connectionStatus".to_string(),
            Value::String(status.to_string()),
        );
        object.insert("timedOut".to_string(), Value::Bool(timed_out));
    }
    result
}

fn with_execution_mode(mut result: Value, execution_mode: &str) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert(
            "executionMode".to_string(),
            Value::String(execution_mode.to_string()),
        );
    }
    result
}

async fn activate_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let mut snapshot = crate::commands::app_attach_background_session(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    let root_tab_id = snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|tab| tab.get("id").and_then(Value::as_str) == Some(tab_id.as_str()))
        .and_then(|tab| {
            tab.get("paneRootTabId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    tab.get("paneRoot")
                        .filter(|value| value.is_object())
                        .map(|_| tab_id.clone())
                })
        });
    if let Some(root_tab_id) = root_tab_id {
        snapshot = crate::commands::app_set_active_pane(app.clone(), root_tab_id, tab_id.clone())
            .await
            .map_err(public_app_error)?;
    }
    crate::show_main_window(app);
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "activate_session",
    ))
}

async fn reconnect_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_reconnect_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "reconnect_session",
    ))
}

async fn disconnect_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_disconnect_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(
        &snapshot,
        Some(&tab_id),
        "disconnect_session",
    ))
}

async fn close_session(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let snapshot = crate::commands::app_close_tab(app.clone(), tab_id.clone())
        .await
        .map_err(public_app_error)?;
    Ok(compact_snapshot(&snapshot, Some(&tab_id), "close_session"))
}

async fn execute_remote_command(
    app: &AppHandle,
    params: &Value,
    progress_sender: Option<mpsc::UnboundedSender<BridgeProgress>>,
    progress_token: Option<Value>,
) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let command = required_text(params, "command", 64 * 1024)?;
    let cwd = optional_string(params, "cwd", 4_096)?;
    let timeout_ms = optional_u64(params, "timeout_ms")?;
    let sudo_password = optional_secret_string(params, "sudo_password", 4 * 1024)?;
    let su_password = optional_secret_string(params, "su_password", 4 * 1024)?;
    let save_sudo_password = optional_bool(params, "save_sudo_password")?.unwrap_or(false);
    let save_su_password = optional_bool(params, "save_su_password")?.unwrap_or(false);
    let privileged_prompt_notice = progress_sender.map(|sender| {
        let progress_token = progress_token.clone();
        Arc::new(move |needed_code: &str| {
            let _ = sender.send(BridgeProgress::privileged_password_prompt(
                needed_code,
                progress_token.clone(),
            ));
        }) as crate::services::action_review::PrivilegedPromptNotice
    });
    let result = crate::services::action_review::execute_background_remote_command(
        app,
        crate::services::action_review::RemoteExecRequest {
            tab_id: tab_id.clone(),
            command,
            cwd,
            timeout_ms,
            expected_session_revision: None,
            sudo_password,
            su_password,
            save_sudo_password,
            save_su_password,
            allow_local_privileged_prompt: true,
            privileged_prompt_notice,
        },
    )
    .await
    .map_err(public_app_error)?;
    Ok(json!({
        "tabId": tab_id,
        "executionMode": EXECUTION_MODE_BACKGROUND,
        "result": result,
    }))
}

async fn execute_visible_command(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    let command = required_text(params, "command", 64 * 1024)?;
    let timeout_ms = optional_u64(params, "timeout_ms")?;
    let result = crate::services::action_review::execute_visible_terminal_command(
        app, &tab_id, &command, timeout_ms,
    )
    .await
    .map_err(public_app_error)?;
    Ok(json!({
        "tabId": tab_id,
        "executionMode": EXECUTION_MODE_VISIBLE_TERMINAL,
        "accepted": true,
        "result": result,
    }))
}

async fn execute_command_template(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let tab_id = required_string(params, "tab_id", 256)?;
    crate::services::action_review::ensure_visible_terminal_session_active(app, &tab_id)
        .await
        .map_err(public_app_error)?;
    let command_id = required_string(params, "command_id", 256)?;
    let args = optional_string_array(params, "args", 64, 4_096)?;
    let options = params.get("options").cloned();
    crate::commands::app_execute_command_template(
        app.clone(),
        tab_id.clone(),
        command_id,
        args,
        options,
    )
    .await
    .map(|result| json!({ "tabId": tab_id, "result": result }))
    .map_err(public_app_error)
}
