/// Dispatch the SSH worker commands owned by the terminal group.
#[allow(clippy::too_many_arguments)]
async fn dispatch_terminal_cmd(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
    shell_writer: &SshShellWriteHalf,
    sftp: &SharedSftpSession,
    transfer_sftp_slot: &TransferSftpSlot,
    operation_timeout: Duration,
    file_access_mode: &mut String,
    root_file_access_method: &mut RootFileAccessMethod,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    saved_sudo_password: &mut Option<String>,
    saved_su_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
    exec_channel_enabled: bool,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::WriteTerminal(data) => {
            write_shell_data(shell_writer, data.into_bytes()).await?;
            Ok(false)
        }
        WorkerCmd::SerialControl { respond_to, .. } => {
            let _ = respond_to.send(Err("SSH 不支持串口控制".to_string()));
            Ok(false)
        }
        WorkerCmd::SerialTransfer { respond_to, .. } => {
            let _ = respond_to.send(Err("SSH 不支持串口文件传输".to_string()));
            Ok(false)
        }
        WorkerCmd::ResizeTerminal { cols, rows, .. } => {
            // Resize is best-effort: a stuck SSH transport must not pin the
            // worker loop. The 16ms flush and terminal_input_rx polling
            // depend on this branch returning quickly.
            match timeout(
                TERMINAL_RESIZE_TIMEOUT,
                shell_writer.window_change(cols, rows, 0, 0),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        format!("terminal resize failed: {error}"),
                    );
                }
                Err(_) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        "terminal resize timed out",
                    );
                }
            }
            Ok(false)
        }
        WorkerCmd::ExecuteRemoteCommand {
            command,
            cwd,
            timeout_ms,
            stdin,
            request_pty,
            cancellation,
            respond_to,
        } => {
            if exec_channel_enabled {
                spawn_remote_command(
                    handle,
                    command,
                    cwd,
                    timeout_ms,
                    stdin,
                    request_pty,
                    cancellation,
                    respond_to,
                );
            } else {
                let _ = respond_to.send(Err("SSH Exec 通道已关闭，无法执行远程命令。".to_string()));
            }
            Ok(false)
        }
        WorkerCmd::ListSshTunnels { respond_to } => {
            enqueue_tunnel_command(tunnel_commands, TunnelCommand::List { respond_to });
            Ok(false)
        }
        WorkerCmd::CreateSshTunnel { rule, respond_to } => {
            match serde_json::from_value::<SshTunnelRule>(rule) {
                Ok(rule) => enqueue_tunnel_command(
                    tunnel_commands,
                    TunnelCommand::Create { rule, respond_to },
                ),
                Err(error) => {
                    let _ = respond_to.send(Err(format!("Invalid tunnel rule: {error}")));
                }
            }
            Ok(false)
        }
        WorkerCmd::StartSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Start {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::StopSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Stop {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::DeleteSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Delete {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        _ => dispatch_transfer_cmd(cmd, handle, shell_writer, sftp, transfer_sftp_slot, operation_timeout, file_access_mode, root_file_access_method, sudo_user, sudo_password, saved_sudo_password, saved_su_password, tab_id, app, state, tunnel_commands, exec_channel_enabled).await,
    }
}
