fn validate_remote_exec_tab_id(raw_tab_id: &str) -> Result<String, AppError> {
    let tab_id = raw_tab_id.trim().to_string();
    if tab_id.is_empty()
        || tab_id.len() > MAX_REMOTE_EXEC_TAB_ID_BYTES
        || tab_id.chars().any(char::is_control)
    {
        return Err(AppError::Command(
            "FileTerm session was not found".to_string(),
        ));
    }
    Ok(tab_id)
}

fn validate_remote_exec_command(raw_command: &str) -> Result<String, AppError> {
    let command = raw_command.trim().to_string();
    if command.is_empty() {
        return Err(AppError::Command(
            "Remote command must not be empty".to_string(),
        ));
    }
    if command.len() > MAX_REMOTE_EXEC_COMMAND_BYTES {
        return Err(AppError::Command(format!(
            "Remote command exceeds the {} KiB limit",
            MAX_REMOTE_EXEC_COMMAND_BYTES / 1024
        )));
    }
    Ok(command)
}

fn validate_network_device_command(raw_command: &str) -> Result<String, AppError> {
    if raw_command.chars().any(char::is_control) {
        return Err(AppError::Command(
            NETWORK_DEVICE_COMMAND_INVALID.to_string(),
        ));
    }
    validate_remote_exec_command(raw_command)
}

fn validate_visible_terminal_command(raw_command: &str) -> Result<String, AppError> {
    if raw_command.chars().any(char::is_control) {
        return Err(AppError::Command(
            VISIBLE_TERMINAL_COMMAND_INVALID.to_string(),
        ));
    }
    validate_remote_exec_command(raw_command)
}

fn validate_remote_exec_cwd(cwd: Option<String>) -> Result<Option<String>, AppError> {
    let cwd = cwd
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if cwd
        .as_ref()
        .is_some_and(|value| value.len() > MAX_REMOTE_EXEC_CWD_BYTES)
    {
        return Err(AppError::Command(
            "Remote command working directory is too long".to_string(),
        ));
    }
    Ok(cwd)
}

fn parse_remote_exec_result(value: Value) -> Result<RemoteExecResult, AppError> {
    let output = value
        .get("output")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Serialization("Remote command output was invalid".to_string()))?
        .to_string();
    let exit_code = value
        .get("exitCode")
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| AppError::Serialization("Remote command exit code was invalid".to_string()))?;
    let timed_out = value
        .get("timedOut")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            AppError::Serialization("Remote command timeout state was invalid".to_string())
        })?;
    let output_truncated = value
        .get("outputTruncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let raw_terminal = value
        .get("rawTerminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let input_kind = value
        .get("inputKind")
        .and_then(Value::as_str)
        .filter(|kind| matches!(*kind, "secret" | "text"))
        .map(ToOwned::to_owned);
    let input_required = value
        .get("inputRequired")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| input_kind.is_some());
    let input_required = input_required && input_kind.is_some();
    Ok(RemoteExecResult {
        output,
        exit_code,
        timed_out,
        output_truncated,
        raw_terminal,
        input_required,
        input_kind,
    })
}
