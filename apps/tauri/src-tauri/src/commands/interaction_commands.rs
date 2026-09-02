// SSH interaction, password, approval, and AI handoff commands.
fn summarize_ssh_interaction_response(response: &serde_json::Value) -> String {
    let kind = response
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let canceled = response
        .get("canceled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let mut fields = vec![format!("kind={}", kind.escape_default()), format!("canceled={canceled}")];
    match kind {
        "keyboard-interactive" => fields.push(format!(
            "answers={}",
            response
                .get("answers")
                .and_then(serde_json::Value::as_array)
                .map_or(0, Vec::len)
        )),
        "host-verification" => fields.push(format!(
            "decision={}",
            response
                .get("decision")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("missing")
                .escape_default()
        )),
        "credentials" => fields.extend([
            format!(
                "username_present={}",
                response
                    .get("username")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            ),
            format!(
                "password_present={}",
                response
                    .get("password")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.is_empty())
            ),
        ]),
        "key-passphrase" => fields.extend([
            format!(
                "passphrase_present={}",
                response
                    .get("passphrase")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|value| !value.is_empty())
            ),
            format!(
                "save_passphrase={}",
                response
                    .get("savePassphrase")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            ),
        ]),
        _ => {}
    }
    fields.join(" ")
}

#[tauri::command]
pub async fn app_resolve_ssh_interaction(
    app: AppHandle,
    request_id: String,
    response: serde_json::Value,
) -> Result<(), AppError> {
    let response_summary = summarize_ssh_interaction_response(&response);
    crate::services::logging::debug(
        &app,
        "ssh-interaction",
        format!(
            "resolve requested request_id={} {}",
            request_id.escape_default(),
            response_summary
        ),
    );
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let (sender, pending_after) = {
        let mut pending = state.pending_interactions.write().await;
        let sender = pending.remove(&request_id);
        (sender, pending.len())
    };
    let Some(tx) = sender else {
        crate::services::logging::warn(
            &app,
            "ssh-interaction",
            format!(
                "resolve rejected request_id={} reason=no-longer-pending pending={pending_after} {}",
                request_id.escape_default(),
                response_summary
            ),
        );
        return Err(AppError::Command(
            "SSH interaction request is no longer pending".to_string(),
        ));
    };
    if tx.send(response).is_err() {
        crate::services::logging::warn(
            &app,
            "ssh-interaction",
            format!(
                "resolve rejected request_id={} reason=receiver-closed pending={pending_after} {}",
                request_id.escape_default(),
                response_summary
            ),
        );
        return Err(AppError::Command(
            "SSH interaction receiver is no longer available".to_string(),
        ));
    }
    crate::services::logging::info(
        &app,
        "ssh-interaction",
        format!(
            "resolve delivered request_id={} pending={pending_after} {}",
            request_id.escape_default(),
            response_summary
        ),
    );
    Ok(())
}

#[tauri::command]
pub async fn app_resolve_backup_password(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid backup password request".to_string(),
        ));
    }
    let value = if cancelled {
        None
    } else {
        let value =
            value.ok_or_else(|| AppError::Command("Backup password is required".to_string()))?;
        if value.is_empty()
            || value.len() > 8 * 1024
            || value
                .chars()
                .any(|character| matches!(character, '\0' | '\r' | '\n' | '\u{1b}'))
        {
            return Err(AppError::Command("Backup password is invalid".to_string()));
        }
        Some(value)
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let pending = state
        .pending_backup_passwords
        .write()
        .await
        .remove(request_id);
    if let Some(pending) = pending {
        let _ = pending
            .sender
            .send(crate::services::workspace::BackupPasswordResponse { cancelled, value });
    }
    Ok(())
}

/// Resolve a one-time sudo/su password prompt. The value is accepted only by
/// the main renderer and is forwarded to the waiting exec task through a
/// single-use channel; it never enters terminal input, chat history, or logs.
#[tauri::command]
pub async fn app_resolve_sudo_password_prompt(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
    save: Option<bool>,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid privileged password request".to_string(),
        ));
    }
    let value = if cancelled {
        None
    } else {
        let value = value.ok_or_else(|| {
            AppError::Command("Privileged command password is required".to_string())
        })?;
        if value.is_empty() || value.len() > 4 * 1024 || value.chars().any(char::is_control) {
            return Err(AppError::Command(
                "Privileged command password is invalid".to_string(),
            ));
        }
        Some(value)
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let pending = state
        .pending_sudo_passwords
        .write()
        .await
        .remove(request_id);
    if let Some(pending) = pending {
        let current_revision = state.ai_session_revision(&pending.tab_id).await.to_string();
        let session_is_still_connected = state
            .sessions
            .read()
            .await
            .get(&pending.tab_id)
            .is_some_and(|session| session.connected);
        let target_is_current =
            session_is_still_connected && current_revision == pending.expected_session_revision;
        let _ = pending
            .sender
            .send(crate::services::workspace::SudoPasswordResponse {
                cancelled: cancelled || !target_is_current,
                value: target_is_current.then_some(value).flatten(),
                save: target_is_current && !cancelled && save.unwrap_or(false),
            });
    }
    Ok(())
}

#[tauri::command]
pub async fn app_set_sudo_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    if window.label() != "main" {
        return Err(AppError::Window(
            "Only the FileTerm main window may receive privileged password input".to_string(),
        ));
    }
    let registration_id = registration_id.trim();
    if registration_id.is_empty() || registration_id.len() > 200 {
        return Err(AppError::Command(
            "Invalid privileged password renderer registration".to_string(),
        ));
    }
    app.state::<crate::services::workspace::WorkspaceState>()
        .set_sudo_password_renderer_ready(registration_id, ready)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn app_set_backup_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    if window.label() != "main" {
        return Err(AppError::Window(
            "Only the FileTerm main window may receive backup password input".to_string(),
        ));
    }
    let registration_id = registration_id.trim();
    if registration_id.is_empty() || registration_id.len() > 200 {
        return Err(AppError::Command(
            "Invalid backup password renderer registration".to_string(),
        ));
    }
    app.state::<crate::services::workspace::WorkspaceState>()
        .set_backup_password_renderer_ready(registration_id, ready)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn app_resolve_mcp_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    crate::services::action_review::resolve_action_approval(&app, &request_id, approved).await
}

#[tauri::command]
pub async fn app_resolve_action_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    crate::services::action_review::resolve_action_approval(&app, &request_id, approved).await
}

#[tauri::command]
pub async fn app_execute_ai_terminal_handoff(
    app: AppHandle,
    request_id: String,
    tab_id: String,
    command: String,
) -> Result<(), AppError> {
    crate::services::action_review::execute_ai_terminal_handoff(
        &app,
        &request_id,
        &tab_id,
        &command,
    )
    .await
}
