
/// Returns `Ok(true)` when the worker should exit (Disconnect requested),
/// `Ok(false)` otherwise.
///
/// When a server accepts an SSH shell but refuses its `sftp` subsystem, keep
/// terminal and tunnel commands operational. Every file operation is replied
/// to immediately with the cached handshake failure instead of falling back
/// to shell commands or leaving the caller waiting on a nonexistent channel.
#[allow(clippy::too_many_arguments)] // Worker state is borrowed separately to avoid a second mutable aggregate.
async fn handle_worker_cmd_without_sftp(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
    shell_writer: &SshShellWriteHalf,
    file_access_mode: &mut String,
    root_file_access_method: &mut RootFileAccessMethod,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    _saved_sudo_password: &mut Option<String>,
    _saved_su_password: &mut Option<String>,
    tab_id: &str,
    state: &crate::services::workspace::WorkspaceState,
    tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
    unavailable_reason: &str,
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
            // Best-effort resize, mirroring handle_worker_cmd. Without a
            // timeout a stuck SSH transport would freeze the worker loop and
            // make Ctrl+C unresponsive.
            match timeout(
                TERMINAL_RESIZE_TIMEOUT,
                shell_writer.window_change(cols, rows, 0, 0),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    // SFTP is already unavailable here, so the user already
                    // has a degraded session; do not escalate resize errors.
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
        WorkerCmd::ListRemoteFiles { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::ReadRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile { respond_to, .. }
        | WorkerCmd::CreateRemoteDirectory { respond_to, .. }
        | WorkerCmd::CreateRemoteFile { respond_to, .. }
        | WorkerCmd::CopyRemotePath { respond_to, .. }
        | WorkerCmd::MoveRemotePath { respond_to, .. }
        | WorkerCmd::RenameRemotePath { respond_to, .. }
        | WorkerCmd::DeleteRemotePath { respond_to, .. }
        | WorkerCmd::ChangeRemotePermissions { respond_to, .. }
        | WorkerCmd::UploadLocalFile { respond_to, .. }
        | WorkerCmd::DownloadRemoteFile { respond_to, .. }
        | WorkerCmd::ReplaceRemoteFile { respond_to, .. }
        | WorkerCmd::CommitRemoteStaging { respond_to, .. }
        | WorkerCmd::RemoveRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::StatRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::SetRemoteFileAccessMode {
            mode, respond_to, ..
        } => {
            if mode == "root" {
                let message = if exec_channel_enabled {
                    "SFTP 文件通道不可用，当前不能启用 root 文件视图；请启用 SFTP 后重连。"
                } else {
                    "SSH Exec 通道已关闭，无法启用 root 文件视图。"
                };
                let _ = respond_to.send(Err(message.to_string()));
                return Ok(false);
            }
            if mode != "user" {
                let _ = respond_to.send(Err(format!("SFTP 不可用时不支持文件访问模式：{mode}")));
                return Ok(false);
            }

            *file_access_mode = mode.clone();
            let has_reusable =
                *root_file_access_method == RootFileAccessMethod::Sudo && sudo_password.is_some();
            let su_user = sudo_user.clone();
            let mut sessions = state.sessions.write().await;
            if let Some(s) = sessions.get_mut(tab_id) {
                s.file_access_mode = mode;
                s.sudo_user = su_user;
                s.has_reusable_sudo_auth = has_reusable;
            }
            let _ = respond_to.send(Ok(()));
            Ok(false)
        }
        WorkerCmd::Disconnect => Ok(true),
    }
}
