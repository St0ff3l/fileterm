/// Handle one command received by the SSH worker's fair event loop.
///
/// Root-password capture is kept next to command dispatch because a terminal
/// write can complete an interactive `sudo`/`su` exchange before the command
/// is handed to the SFTP or shell path. `Ok(true)` means the caller should
/// flush output and terminate the worker.
#[allow(clippy::too_many_arguments)]
async fn handle_worker_command_event(
    cmd: Option<WorkerCmd>,
    network_device_mode: bool,
    handle: &Arc<Handle<ClientHandler>>,
    shell_writer: &Arc<SshShellWriteHalf>,
    sftp: Option<&SharedSftpSession>,
    transfer_sftp_slot: &TransferSftpSlot,
    operation_timeout: Duration,
    file_access_mode: &mut String,
    root_file_access_method: &mut RootFileAccessMethod,
    sudo_user: &mut Option<String>,
    root_password: &mut Option<String>,
    sudo_password: &mut Option<String>,
    su_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    tunnel_command_tx: &mpsc::UnboundedSender<TunnelCommand>,
    unavailable_reason: &str,
    exec_channel_enabled: bool,
    awaiting_root_access_auth: &mut Option<PendingRootAccessAuth>,
    pending_sudo_password: &mut String,
    recent_terminal_input: &mut String,
    last_authenticated_root_access: &mut Option<PendingRootAccessAuth>,
    pending_root_access_command: &mut Option<PendingRootAccessAuth>,
) -> Result<bool, String> {
    let Some(cmd) = cmd else {
        return Ok(true);
    };

    if !network_device_mode {
        if let WorkerCmd::WriteTerminal(data) = &cmd {
            let previous_pending_command = pending_root_access_command.clone();
            if capture_root_access_password_input(
                data,
                awaiting_root_access_auth,
                pending_sudo_password,
                recent_terminal_input,
                root_password,
                last_authenticated_root_access,
                pending_root_access_command,
            ) {
                cache_root_password_for_auth(
                    last_authenticated_root_access.as_ref(),
                    root_password,
                    sudo_password,
                    su_password,
                );
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    session.has_reusable_sudo_auth = matches!(
                        last_authenticated_root_access.as_ref(),
                        Some(auth) if auth.method == RootFileAccessMethod::Sudo
                    ) && root_password.is_some();
                }
            }
            if pending_root_access_command != &previous_pending_command {
                if let Some(auth) = pending_root_access_command.as_ref() {
                    crate::services::logging::ssh_debug(
                        app,
                        tab_id,
                        format!(
                            "interactive privilege command tracked method={:?} target_user={} interactive_shell={}",
                            auth.method, auth.target_user, auth.interactive_shell
                        ),
                    );
                }
            }
        }
    }

    let result = if let Some(sftp) = sftp {
        handle_worker_cmd(
            cmd,
            handle,
            shell_writer,
            sftp,
            transfer_sftp_slot,
            operation_timeout,
            file_access_mode,
            root_file_access_method,
            sudo_user,
            root_password,
            sudo_password,
            su_password,
            tab_id,
            app,
            state,
            tunnel_command_tx,
            exec_channel_enabled,
        )
        .await
    } else {
        handle_worker_cmd_without_sftp(
            cmd,
            handle,
            shell_writer,
            file_access_mode,
            root_file_access_method,
            sudo_user,
            root_password,
            sudo_password,
            su_password,
            tab_id,
            state,
            tunnel_command_tx,
            unavailable_reason,
            exec_channel_enabled,
        )
        .await
    };

    result
}
