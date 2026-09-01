// Workspace snapshot, worker command helpers, and session runtime.
pub async fn get_workspace_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    get_workspace_snapshot_unlocked(app).await
}

async fn get_workspace_snapshot_and_emit(app: &AppHandle) -> Result<serde_json::Value, AppError> {
    let snapshot = get_workspace_snapshot_unlocked(app.clone()).await?;
    if let Err(error) = app.emit("workspace:snapshot", snapshot.clone()) {
        // Persistence has already succeeded. A failed best-effort broadcast
        // must not turn a successful mutation into a retryable renderer error
        // that can create duplicate folders/commands/profiles.
        crate::services::logging::warn(
            app,
            "workspace",
            format!("failed to broadcast workspace snapshot: {error}"),
        );
    }
    Ok(snapshot)
}

async fn send_worker_cmd<T>(
    app: &AppHandle,
    tab_id: &str,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    send_worker_cmd_with_response_timeout(app, tab_id, WORKER_FILE_RESPONSE_TIMEOUT, make_cmd).await
}

/// Send a file operation with a command-scoped cancellation token. The SSH
/// worker runs file operations in detached tasks so its terminal input loop
/// stays responsive; dropping the renderer response alone therefore cannot
/// stop a timed-out write/delete/rename. Keep the token in the WorkerCmd and
/// cancel it whenever the IPC boundary times out or the send cannot complete.
async fn send_worker_file_cmd<T>(
    app: &AppHandle,
    tab_id: &str,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
) -> Result<T, AppError> {
    send_worker_file_cmd_with_response_timeout(app, tab_id, WORKER_FILE_RESPONSE_TIMEOUT, make_cmd)
        .await
}

async fn send_worker_file_cmd_with_response_timeout<T>(
    app: &AppHandle,
    tab_id: &str,
    response_timeout: Duration,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>, CancellationToken) -> WorkerCmd,
) -> Result<T, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = state
        .workers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| {
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                "file command rejected: session not found",
            );
            AppError::Storage("Session not found".to_string())
        })?;
    let (tx, rx) = oneshot::channel();
    let cancellation = CancellationToken::new();
    let cmd = make_cmd(tx, cancellation.clone());
    let send_result = timeout(WORKER_FILE_CMD_SEND_TIMEOUT, sender.send(cmd)).await;
    match send_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            cancellation.cancel();
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                format!("file command send failed error={error}"),
            );
            return Err(AppError::Storage(error.to_string()));
        }
        Err(_) => {
            cancellation.cancel();
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                format!(
                    "file command send timed out timeout_secs={}",
                    WORKER_FILE_CMD_SEND_TIMEOUT.as_secs()
                ),
            );
            return Err(AppError::Storage(
                "Worker busy: command send timeout".to_string(),
            ));
        }
    }

    match timeout(response_timeout, rx).await {
        Ok(Ok(Ok(result))) => Ok(result),
        Ok(Ok(Err(error))) => {
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                format!("file command failed error={error}"),
            );
            Err(AppError::Storage(error))
        }
        Ok(Err(error)) => {
            cancellation.cancel();
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                format!("file command receiver closed error={error}"),
            );
            Err(AppError::Storage(error.to_string()))
        }
        Err(_) => {
            cancellation.cancel();
            crate::services::logging::warn(
                app,
                &format!("worker:{tab_id}"),
                format!(
                    "file command response timed out timeout_secs={}",
                    response_timeout.as_secs()
                ),
            );
            Err(AppError::Storage(
                "远程操作超时，后台操作已取消，请检查连接后重试".to_string(),
            ))
        }
    }
}

pub(crate) async fn send_worker_cmd_with_response_timeout<T>(
    app: &AppHandle,
    tab_id: &str,
    response_timeout: Duration,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    send_worker_cmd_with_response_timeout_cancellable(app, tab_id, response_timeout, None, make_cmd)
        .await
}

/// Send a worker command with an optional request cancellation boundary. A
/// command may already be queued while Copilot is waiting for its response;
/// selecting cancellation here prevents the AI request from remaining stuck
/// behind an unrelated worker operation until the normal response timeout.
pub(crate) async fn send_worker_cmd_with_response_timeout_cancellable<T>(
    app: &AppHandle,
    tab_id: &str,
    response_timeout: Duration,
    cancellation: Option<&CancellationToken>,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let workers = state.workers.read().await;
    let sender = workers
        .get(tab_id)
        .ok_or_else(|| AppError::Storage("Session not found".to_string()))?
        .clone();
    drop(workers);

    let (tx, rx) = oneshot::channel();
    let cmd = make_cmd(tx);
    if cancellation.is_some_and(|token| token.is_cancelled()) {
        return Err(AppError::Command(
            crate::services::action_review::AI_REQUEST_CANCELLED.to_string(),
        ));
    }

    // 不持有 workers 读锁跨 await：clone sender 后立即释放，避免后续写锁死锁。
    // send 必须超时，worker 卡死时前端能拿到明确错误而不是永久 hang；Copilot
    // 还要能在 send 阶段被 Stop 立即唤醒。
    let send_result = if let Some(cancellation) = cancellation {
        tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(AppError::Command(
                    crate::services::action_review::AI_REQUEST_CANCELLED.to_string(),
                ));
            }
            result = timeout(WORKER_FILE_CMD_SEND_TIMEOUT, sender.send(cmd)) => result,
        }
    } else {
        timeout(WORKER_FILE_CMD_SEND_TIMEOUT, sender.send(cmd)).await
    };
    send_result
        .map_err(|_| AppError::Storage("Worker busy: command send timeout".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let response = if let Some(cancellation) = cancellation {
        tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(AppError::Command(
                    crate::services::action_review::AI_REQUEST_CANCELLED.to_string(),
                ));
            }
            result = timeout(response_timeout, rx) => result,
        }
    } else {
        timeout(response_timeout, rx).await
    };
    let res = response
        .map_err(|_| AppError::Storage("远程操作超时，请检查连接后重试".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?
        .map_err(AppError::Storage)?;
    Ok(res)
}

async fn refresh_remote_files(app: &AppHandle, tab_id: &str, path: &str) -> Result<(), AppError> {
    let started_at = Instant::now();
    crate::services::logging::debug(
        app,
        &format!("sftp:{tab_id}"),
        format!("remote directory listing started path={path}"),
    );
    let files = match send_worker_file_cmd(app, tab_id, |tx, cancellation| {
        WorkerCmd::ListRemoteFiles {
            path: path.to_string(),
            cancellation,
            respond_to: tx,
        }
    })
    .await
    {
        Ok(files) => files,
        Err(error) => {
            crate::services::logging::warn(
                app,
                &format!("sftp:{tab_id}"),
                format!(
                    "remote directory listing failed path={path} elapsed_ms={} error={error}",
                    started_at.elapsed().as_millis()
                ),
            );
            return Err(error);
        }
    };
    crate::services::logging::debug(
        app,
        &format!("sftp:{tab_id}"),
        format!(
            "remote directory listing completed path={path} entries={} elapsed_ms={}",
            files.len(),
            started_at.elapsed().as_millis()
        ),
    );

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.remote_files = files;
    }
    Ok(())
}

/// Refresh a file pane from a shell CWD while respecting the fact that SSH
/// and SFTP may expose different filesystem roots. The worker uses the same
/// candidate order for event-driven CWD updates; keeping the toggle recovery
/// path here in sync prevents enabling Follow terminal from reintroducing the
/// Synology failure.
async fn refresh_remote_files_for_shell_cwd(
    app: &AppHandle,
    tab_id: &str,
    shell_cwd: &str,
    use_sftp_namespace: bool,
) -> Result<String, AppError> {
    let candidates = if use_sftp_namespace {
        crate::sessions::ssh::shell_cwd_sftp_path_candidates(shell_cwd)
    } else {
        vec![shell_cwd.to_string()]
    };
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        match refresh_remote_files(app, tab_id, candidate).await {
            Ok(()) => return Ok(candidate.clone()),
            Err(error) => {
                let can_try_next = use_sftp_namespace
                    && index + 1 < candidates.len()
                    && crate::sessions::ssh::is_sftp_path_not_found_message(&error.to_string());
                last_error = Some(error);
                if !can_try_next {
                    break;
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| AppError::Storage(format!("无法列出 Shell 当前目录：{shell_cwd}"))))
}

/// Read-only MCP surface for browsing an already-open file-capable session.
/// The MCP adapter intentionally cannot open profiles or access profile
/// secrets; the desktop UI owns both actions and this helper only delegates to
/// an existing protocol worker.
pub(crate) async fn mcp_list_remote_directory(
    app: AppHandle,
    tab_id: String,
    requested_path: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let path = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.capabilities.files {
            return Err(AppError::Command(
                "This FileTerm session does not provide remote file access".to_string(),
            ));
        }
        requested_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| session.remote_path.clone())
    };

    if path.len() > 4_096 {
        return Err(AppError::Command(
            "Remote path exceeds the FileTerm MCP limit".to_string(),
        ));
    }

    refresh_remote_files(&app, &tab_id, &path).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sessions = state.sessions.read().await;
    let session = sessions.get(&tab_id).ok_or_else(|| {
        AppError::Command("FileTerm session closed while listing directory".to_string())
    })?;
    Ok(serde_json::json!({
        "tabId": tab_id,
        "path": path,
        "items": session.remote_files,
    }))
}

/// Execute a bounded command through the SSH command boundary. Ordinary
/// servers use a dedicated exec channel; network-device sessions send the
/// native command through their already-visible raw PTY because they do not
/// expose a POSIX shell or a portable process-exit protocol.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn app_execute_remote_command(
    app: AppHandle,
    tab_id: String,
    command: String,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    sudo_password: Option<String>,
    su_password: Option<String>,
    save_sudo_password: Option<bool>,
    save_su_password: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    let result = crate::services::action_review::execute_remote_command(
        &app,
        crate::services::action_review::RemoteExecRequest {
            tab_id,
            command,
            cwd,
            timeout_ms,
            expected_session_revision: None,
            sudo_password,
            su_password,
            save_sudo_password: save_sudo_password.unwrap_or(false),
            save_su_password: save_su_password.unwrap_or(false),
            allow_local_privileged_prompt: true,
            privileged_prompt_notice: None,
        },
    )
    .await?;
    serde_json::to_value(result).map_err(|error| AppError::Serialization(error.to_string()))
}

fn create_tab_layout(profile: &serde_json::Value) -> String {
    let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("ssh");
    match profile_type {
        "ssh"
            if crate::services::workspace::ConnectionCapabilities::is_network_device_profile(
                profile,
            ) =>
        {
            "terminal-only".to_string()
        }
        "ssh" => "terminal-file".to_string(),
        "ftp" => "file-only".to_string(),
        _ => "terminal-only".to_string(),
    }
}

#[cfg(test)]
mod tab_layout_tests {
    use super::create_tab_layout;

    #[test]
    fn network_device_ssh_profiles_start_with_terminal_only_layout() {
        assert_eq!(
            create_tab_layout(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device"
            })),
            "terminal-only"
        );
        assert_eq!(
            create_tab_layout(&serde_json::json!({ "type": "ssh" })),
            "terminal-file"
        );
    }
}

fn start_session_worker(
    tab_id: String,
    profile: serde_json::Value,
    receiver: mpsc::Receiver<WorkerCmd>,
    terminal_input_receiver: Option<mpsc::UnboundedReceiver<String>>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    match profile.get("type").and_then(Value::as_str).unwrap_or("ssh") {
        "ftp" => {
            crate::sessions::ftp::start_ftp_worker(tab_id, profile, receiver, app, cancellation)
        }
        "telnet" => crate::sessions::telnet::start_telnet_worker(
            tab_id,
            profile,
            receiver,
            app,
            cancellation,
        ),
        "serial" => crate::sessions::serial::start_serial_worker(
            tab_id,
            profile,
            receiver,
            app,
            cancellation,
        ),
        _ => crate::sessions::ssh::start_ssh_worker(
            tab_id,
            profile,
            receiver,
            terminal_input_receiver.expect("SSH worker requires a terminal input channel"),
            app,
            cancellation,
        ),
    }
}

async fn stop_session_worker(state: &crate::services::workspace::WorkspaceState, tab_id: &str) {
    state.connection_operations.forget_tab(tab_id).await;
    crate::sessions::local_terminal::deactivate_local_terminal_runtime(state, tab_id).await;
    if let Some((_, cancellation)) = state
        .serial_transfer_cancellations
        .write()
        .await
        .remove(tab_id)
    {
        cancellation.cancel();
    }
    if let Some(control) = state.worker_controls.write().await.remove(tab_id) {
        // Cancel first: a command sender cannot wake a worker which is inside
        // an SSH read/metrics parse. This also prevents a stale worker from
        // emitting state over a replacement connection after reconnect.
        control.cancel();
    }
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .remove(tab_id);
    state.terminal_inputs.write().await.remove(tab_id);
    let sender = state.workers.write().await.remove(tab_id);
    if let Some(sender) = sender {
        // 超时即放弃：worker 主循环卡死时 channel 已满，send 不进去；
        // 但 sender 已经从 workers map 移除并即将 drop，worker 的
        // `cmd_rx.recv()` 会返回 None 走清理路径，无需依赖这条 Disconnect。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
}

/// Roll back a session that was created for a split pane but could not be
/// attached to the current pane tree. Split creation awaits PTY/SSH startup,
/// so the source tab may be closed or moved by another command before the
/// tree update gets the write lock. Leaving the newly created worker in that
/// case would leak a background PTY that is no longer reachable from the UI.
async fn cleanup_unattached_session(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
) {
    stop_session_worker(state, tab_id).await;
    crate::services::session_logs::stop_for_tab(state, tab_id).await;
    state.serial_reconnect_attempts.write().await.remove(tab_id);

    state.tabs.write().await.retain(|tab| tab.id != tab_id);
    state.sessions.write().await.remove(tab_id);
    state.local_terminal_launches.write().await.remove(tab_id);
    state.remote_forwards.write().await.remove(tab_id);
    state
        .active_pane_tab_id_by_root
        .write()
        .await
        .retain(|root_id, active_tab_id| root_id != tab_id && active_tab_id != tab_id);
    state.remove_ai_session_revision(tab_id).await;
}

pub async fn shutdown_session_workers(app: &AppHandle) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let transfer_cancellations = state
        .serial_transfer_cancellations
        .write()
        .await
        .drain()
        .map(|(_, (_, cancellation))| cancellation)
        .collect::<Vec<_>>();
    for cancellation in transfer_cancellations {
        cancellation.cancel();
    }
    let controls = state
        .worker_controls
        .write()
        .await
        .drain()
        .map(|(_, control)| control)
        .collect::<Vec<_>>();
    for control in controls {
        control.cancel();
    }
    state.local_terminal_runtime_ids.write().await.clear();
    let local_gates = state
        .local_terminal_runtime_gates
        .write()
        .await
        .drain()
        .map(|(_, gate)| gate)
        .collect::<Vec<_>>();
    for gate in local_gates {
        gate.deactivate().await;
    }
    state.local_terminal_launches.write().await.clear();
    state.terminal_inputs.write().await.clear();
    state.pending_backup_passwords.write().await.clear();
    let senders = state
        .workers
        .write()
        .await
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();
    for sender in senders {
        // Cmd+Q 退出链路：任何单个卡死 worker 都不能阻塞整体退出。
        // 超时后直接 drop sender，worker 收到 recv()==None 自动清理。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
    crate::services::session_logs::shutdown(&state).await;
}
