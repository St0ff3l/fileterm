// The prompt payload is kept explicit at this boundary so the password source,
// target identity, and local-only display fields cannot be accidentally mixed
// with the visible terminal input route.
#[allow(clippy::too_many_arguments)]
async fn request_sudo_password_prompt(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    expected_session_revision: Option<&str>,
    kind: PrivilegedCommandKind,
    host: &str,
    shell_user: Option<&str>,
    cwd: Option<&str>,
    command: &str,
    privileged_prompt_notice: Option<PrivilegedPromptNotice>,
    cancellation: Option<&CancellationToken>,
) -> Result<(String, bool), AppError> {
    let needed_code = match kind {
        PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
        PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
    };
    if !state.has_sudo_password_renderer().await || !main_window_exists(app) {
        return Err(AppError::Command(needed_code.to_string()));
    }
    crate::show_main_window(app);
    let current_session_revision = state.ai_session_revision(tab_id).await.to_string();
    let expected_session_revision = expected_session_revision
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&current_session_revision)
        .to_string();
    let request_id = format!("sudo-password-{}", uuid::Uuid::new_v4());
    let (sender, receiver) = oneshot::channel();
    let pending = crate::services::workspace::PendingSudoPassword {
        tab_id: tab_id.to_string(),
        expected_session_revision,
        sender,
    };
    if !state
        .insert_pending_sudo_password(request_id.clone(), pending)
        .await
    {
        return Err(AppError::Command(needed_code.to_string()));
    }
    let payload = serde_json::json!({
        "requestId": request_id,
        "tabId": tab_id,
        "kind": match kind {
            PrivilegedCommandKind::Sudo => "sudo",
            PrivilegedCommandKind::Su => "su",
        },
        "host": host,
        "shellUser": shell_user,
        "cwd": cwd,
        "command": command,
    });
    if let Err(error) = app.emit("sudo:password-request", payload) {
        state
            .pending_sudo_passwords
            .write()
            .await
            .remove(&request_id);
        crate::services::logging::warn(
            app,
            "security",
            format!("privileged password prompt delivery failed: {error}"),
        );
        return Err(AppError::Command(needed_code.to_string()));
    }
    if let Some(notice) = privileged_prompt_notice {
        notice(needed_code);
    }

    let response = match cancellation {
        Some(cancellation) => tokio::select! {
            _ = cancellation.cancelled() => {
                state
                    .pending_sudo_passwords
                    .write()
                    .await
                    .remove(&request_id);
                emit_sudo_password_prompt_cancelled(app, &request_id);
                return Err(remote_exec_cancelled_error());
            }
            response = timeout(PRIVILEGED_PASSWORD_PROMPT_TIMEOUT, receiver) => response,
        },
        None => timeout(PRIVILEGED_PASSWORD_PROMPT_TIMEOUT, receiver).await,
    };
    let response = match response {
        Ok(Ok(response)) => response,
        Ok(Err(_)) | Err(_) => {
            state
                .pending_sudo_passwords
                .write()
                .await
                .remove(&request_id);
            emit_sudo_password_prompt_cancelled(app, &request_id);
            return Err(AppError::Command(
                match kind {
                    PrivilegedCommandKind::Sudo => SUDO_PASSWORD_CANCELLED,
                    PrivilegedCommandKind::Su => SU_PASSWORD_CANCELLED,
                }
                .to_string(),
            ));
        }
    };
    check_cancellation(cancellation)?;
    if response.cancelled {
        return Err(AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_CANCELLED,
                PrivilegedCommandKind::Su => SU_PASSWORD_CANCELLED,
            }
            .to_string(),
        ));
    }
    let password = response.value.ok_or_else(|| {
        AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
                PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
            }
            .to_string(),
        )
    })?;
    validate_privileged_password(&password)?;
    Ok((password, response.save))
}

fn remote_exec_cancelled_error() -> AppError {
    AppError::Command(AI_REQUEST_CANCELLED.to_string())
}

fn check_cancellation(cancellation: Option<&CancellationToken>) -> Result<(), AppError> {
    if cancellation.is_some_and(|token| token.is_cancelled()) {
        return Err(remote_exec_cancelled_error());
    }
    Ok(())
}

fn emit_sudo_password_prompt_cancelled(app: &AppHandle, request_id: &str) {
    let _ = app.emit(
        "sudo:password-request-cancelled",
        serde_json::json!({ "requestId": request_id }),
    );
}

fn main_window_exists(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some()
}

fn privileged_command_kind(command: &str) -> Option<PrivilegedCommandKind> {
    let trimmed = command.trim_start();
    if trimmed == "sudo"
        || trimmed
            .strip_prefix("sudo")
            .is_some_and(starts_with_shell_space)
    {
        return Some(PrivilegedCommandKind::Sudo);
    }
    if trimmed == "su"
        || trimmed
            .strip_prefix("su")
            .is_some_and(starts_with_shell_space)
    {
        return Some(PrivilegedCommandKind::Su);
    }
    None
}

fn starts_with_shell_space(value: &str) -> bool {
    value.chars().next().is_some_and(char::is_whitespace)
}

fn validate_privileged_password(password: &str) -> Result<(), AppError> {
    if password.is_empty()
        || password.len() > MAX_REMOTE_EXEC_SECRET_BYTES
        || password.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "Privileged command password input is invalid".to_string(),
        ));
    }
    Ok(())
}

fn wrap_sudo_command(command: &str) -> String {
    let trimmed = command.trim_start();
    let suffix = trimmed.strip_prefix("sudo").unwrap_or_default();
    format!("sudo -S -p ''{suffix}")
}

fn resolve_privileged_password(
    kind: PrivilegedCommandKind,
    explicit_password: Option<String>,
    saved_password: Option<String>,
) -> Result<String, AppError> {
    let password = explicit_password.or(saved_password).ok_or_else(|| {
        AppError::Command(
            match kind {
                PrivilegedCommandKind::Sudo => SUDO_PASSWORD_NEEDED,
                PrivilegedCommandKind::Su => SU_PASSWORD_NEEDED,
            }
            .to_string(),
        )
    })?;
    validate_privileged_password(&password)?;
    Ok(password)
}

fn detect_privileged_auth_failure(output: &str, kind: PrivilegedCommandKind) -> bool {
    let output = output.to_ascii_lowercase();
    let patterns: &[&str] = match kind {
        PrivilegedCommandKind::Sudo => &[
            "sorry, try again",
            "incorrect password",
            "authentication failure",
            "a password is required",
        ],
        PrivilegedCommandKind::Su => &[
            "su: authentication failure",
            "su: incorrect password",
            "su: sorry",
            "authentication failure",
        ],
    };
    patterns.iter().any(|pattern| output.contains(pattern))
}

fn prepare_remote_exec(
    app: &AppHandle,
    profile_id: &str,
    command: &str,
    sudo_password: Option<String>,
    su_password: Option<String>,
    save_sudo_password: bool,
    save_su_password: bool,
) -> Result<PreparedRemoteExec, AppError> {
    let kind = privileged_command_kind(command);
    let has_any_credential_input =
        sudo_password.is_some() || su_password.is_some() || save_sudo_password || save_su_password;
    let Some(kind) = kind else {
        if has_any_credential_input {
            return Err(AppError::Command(
                "Privileged password parameters require a sudo or su command".to_string(),
            ));
        }
        return Ok(PreparedRemoteExec {
            command: command.to_string(),
            stdin: None,
            request_pty: false,
            kind: None,
            used_saved_password: false,
            save_password: None,
        });
    };

    let (explicit_password, save_password) = match kind {
        PrivilegedCommandKind::Sudo => {
            if su_password.is_some() || save_su_password {
                return Err(AppError::Command(
                    "su password parameters cannot be used with a sudo command".to_string(),
                ));
            }
            (sudo_password, save_sudo_password)
        }
        PrivilegedCommandKind::Su => {
            if sudo_password.is_some() || save_sudo_password {
                return Err(AppError::Command(
                    "sudo password parameters cannot be used with a su command".to_string(),
                ));
            }
            (su_password, save_su_password)
        }
    };

    if save_password && explicit_password.is_none() {
        return Err(AppError::Command(
            "Saving a privileged password requires a one-shot password value".to_string(),
        ));
    }
    let saved_password = match kind {
        PrivilegedCommandKind::Sudo => {
            crate::services::profile_ops::read_sudo_password(app, profile_id)?
        }
        PrivilegedCommandKind::Su => {
            crate::services::profile_ops::read_su_password(app, profile_id)?
        }
    };
    let used_saved_password = explicit_password.is_none() && saved_password.is_some();
    let password = resolve_privileged_password(kind, explicit_password, saved_password)?;

    let save_password = if save_password {
        Some((profile_id.to_string(), kind, password.clone()))
    } else {
        None
    };
    Ok(PreparedRemoteExec {
        command: match kind {
            PrivilegedCommandKind::Sudo => wrap_sudo_command(command),
            PrivilegedCommandKind::Su => command.to_string(),
        },
        stdin: Some(format!("{password}\n")),
        request_pty: matches!(kind, PrivilegedCommandKind::Su),
        kind: Some(kind),
        used_saved_password,
        save_password,
    })
}
