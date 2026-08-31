// Serial controls, transfers, and session-log commands.
fn parse_serial_control_action(
    action: &str,
) -> Result<crate::sessions::SerialControlAction, AppError> {
    match action {
        "set-dtr" => Ok(crate::sessions::SerialControlAction::SetDtr),
        "set-rts" => Ok(crate::sessions::SerialControlAction::SetRts),
        "pulse-dtr" => Ok(crate::sessions::SerialControlAction::PulseDtr),
        "pulse-rts" => Ok(crate::sessions::SerialControlAction::PulseRts),
        "send-break" => Ok(crate::sessions::SerialControlAction::SendBreak),
        "clear-buffers" => Ok(crate::sessions::SerialControlAction::ClearBuffers),
        "reset" => Ok(crate::sessions::SerialControlAction::Reset),
        "status" => Ok(crate::sessions::SerialControlAction::Status),
        _ => Err(AppError::Command("串口控制操作无效".to_string())),
    }
}

fn parse_serial_transfer_direction(
    direction: &str,
) -> Result<crate::sessions::SerialTransferDirection, AppError> {
    match direction {
        "send" => Ok(crate::sessions::SerialTransferDirection::Send),
        "receive" => Ok(crate::sessions::SerialTransferDirection::Receive),
        _ => Err(AppError::Command("串口传输方向无效".to_string())),
    }
}

fn parse_serial_transfer_mode(mode: &str) -> Result<crate::sessions::SerialTransferMode, AppError> {
    match mode {
        "raw" => Ok(crate::sessions::SerialTransferMode::Raw),
        "xmodem" => Ok(crate::sessions::SerialTransferMode::Xmodem),
        "ymodem" => Ok(crate::sessions::SerialTransferMode::Ymodem),
        "zmodem" => Ok(crate::sessions::SerialTransferMode::Zmodem),
        "kermit" => Ok(crate::sessions::SerialTransferMode::Kermit),
        _ => Err(AppError::Command("串口传输协议无效".to_string())),
    }
}

fn resolve_serial_transfer_path(
    direction: crate::sessions::SerialTransferDirection,
    local_path: &str,
    file_name: Option<&str>,
) -> Result<String, AppError> {
    let path = Path::new(local_path);
    if local_path.trim().is_empty() {
        return Err(AppError::Command("串口传输路径不能为空".to_string()));
    }
    match direction {
        crate::sessions::SerialTransferDirection::Send => {
            if !path.is_file() {
                return Err(AppError::Command(
                    "串口发送文件不存在或不是文件".to_string(),
                ));
            }
            Ok(path.to_string_lossy().into_owned())
        }
        crate::sessions::SerialTransferDirection::Receive => {
            if !path.is_dir() {
                return Err(AppError::Command("串口接收目录不存在".to_string()));
            }
            let file_name = file_name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::Command("串口接收文件名不能为空".to_string()))?;
            if !is_safe_serial_file_name(file_name) {
                return Err(AppError::Command("串口接收文件名无效".to_string()));
            }
            let target = path.join(file_name);
            if target.exists() {
                return Err(AppError::Command(
                    "串口接收目标文件已存在，请更换文件名".to_string(),
                ));
            }
            Ok(target.to_string_lossy().into_owned())
        }
    }
}

fn is_safe_serial_file_name(file_name: &str) -> bool {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        || file_name.ends_with('.')
        || file_name.ends_with(' ')
    {
        return false;
    }

    let stem = file_name
        .trim_end_matches(['.', ' '])
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_windows_numbered_device_name(&stem, "COM")
        && !is_windows_numbered_device_name(&stem, "LPT")
}

fn is_windows_numbered_device_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
    })
}

fn resolve_serial_transfer_directory(local_path: &str) -> Result<String, AppError> {
    if local_path.trim().is_empty() {
        return Err(AppError::Command("串口传输路径不能为空".to_string()));
    }
    let path = Path::new(local_path);
    if !path.is_dir() {
        return Err(AppError::Command("串口文件传输接收目录不存在".to_string()));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn app_serial_control(
    app: AppHandle,
    tab_id: String,
    action: String,
    value: Option<bool>,
    duration_ms: Option<u64>,
) -> Result<crate::sessions::SerialLineStatus, AppError> {
    let control = parse_serial_control_action(&action)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_serial = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .is_some_and(|tab| tab.session_type == "serial");
    if !is_serial {
        return Err(AppError::Command("当前会话不是串口会话".to_string()));
    }

    send_worker_cmd(&app, &tab_id, |respond_to| WorkerCmd::SerialControl {
        action: control,
        value,
        duration_ms,
        respond_to,
    })
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn app_serial_transfer(
    app: AppHandle,
    tab_id: String,
    direction: String,
    mode: String,
    local_path: String,
    file_name: Option<String>,
    local_paths: Option<Vec<String>>,
    xmodem_preserve_padding: Option<bool>,
) -> Result<crate::sessions::SerialTransferResult, AppError> {
    let direction = parse_serial_transfer_direction(&direction)?;
    let mode = parse_serial_transfer_mode(&mode)?;
    let resolved_paths = match (direction, mode) {
        (
            crate::sessions::SerialTransferDirection::Send,
            crate::sessions::SerialTransferMode::Ymodem
            | crate::sessions::SerialTransferMode::Zmodem
            | crate::sessions::SerialTransferMode::Kermit,
        ) => {
            let candidates = local_paths
                .filter(|paths| !paths.is_empty())
                .unwrap_or_else(|| vec![local_path.clone()]);
            candidates
                .iter()
                .map(|path| resolve_serial_transfer_path(direction, path, None))
                .collect::<Result<Vec<_>, _>>()?
        }
        (
            crate::sessions::SerialTransferDirection::Receive,
            crate::sessions::SerialTransferMode::Ymodem
            | crate::sessions::SerialTransferMode::Zmodem
            | crate::sessions::SerialTransferMode::Kermit,
        ) => vec![resolve_serial_transfer_directory(&local_path)?],
        _ => vec![resolve_serial_transfer_path(
            direction,
            &local_path,
            file_name.as_deref(),
        )?],
    };
    let local_path = resolved_paths
        .first()
        .cloned()
        .ok_or_else(|| AppError::Command("串口传输路径不能为空".to_string()))?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_serial = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .is_some_and(|tab| tab.session_type == "serial");
    if !is_serial {
        return Err(AppError::Command("当前会话不是串口会话".to_string()));
    }
    let worker_cancellation = state
        .worker_controls
        .read()
        .await
        .get(&tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("串口会话未运行".to_string()))?;
    let cancellation = worker_cancellation.child_token();
    let transfer_id = uuid::Uuid::new_v4().to_string();
    {
        let mut active_transfers = state.serial_transfer_cancellations.write().await;
        if active_transfers.contains_key(&tab_id) {
            return Err(AppError::Command(
                "当前串口会话已有文件传输正在进行".to_string(),
            ));
        }
        active_transfers.insert(tab_id.clone(), (transfer_id.clone(), cancellation.clone()));
    }

    let result = send_worker_cmd_with_response_timeout(
        &app,
        &tab_id,
        SERIAL_TRANSFER_RESPONSE_TIMEOUT,
        |respond_to| WorkerCmd::SerialTransfer {
            request: crate::sessions::SerialTransferRequest {
                direction,
                mode,
                local_path,
                local_paths: resolved_paths,
                xmodem_preserve_padding: xmodem_preserve_padding.unwrap_or(true),
            },
            cancellation: cancellation.clone(),
            respond_to,
        },
    )
    .await;
    if result.is_err() {
        cancellation.cancel();
    }
    let mut active_transfers = state.serial_transfer_cancellations.write().await;
    if active_transfers
        .get(&tab_id)
        .is_some_and(|(active_id, _)| active_id == &transfer_id)
    {
        active_transfers.remove(&tab_id);
    }
    result
}

#[tauri::command]
pub async fn app_serial_cancel_transfer(app: AppHandle, tab_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let cancellation = state
        .serial_transfer_cancellations
        .read()
        .await
        .get(&tab_id)
        .map(|(_, cancellation)| cancellation.clone())
        .ok_or_else(|| AppError::Command("当前没有进行中的串口文件传输".to_string()))?;
    cancellation.cancel();
    Ok(())
}

#[tauri::command]
pub async fn app_save_session_log(
    app: AppHandle,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    crate::services::session_logs::save_current_session(&app, &tab_id).await
}

