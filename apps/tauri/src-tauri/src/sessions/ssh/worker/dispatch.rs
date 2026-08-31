
fn spawn_cancellable_file_operation<T, F>(
    cancellation: CancellationToken,
    respond_to: oneshot::Sender<Result<T, String>>,
    operation: F,
) where
    T: Send + 'static,
    F: Future<Output = Result<T, String>> + Send + 'static,
{
    tokio::spawn(async move {
        let result = tokio::select! {
            _ = cancellation.cancelled() => Err("远程文件操作已取消".to_string()),
            result = operation => result,
        };
        let _ = respond_to.send(result);
    });
}

/// Returns `Ok(true)` when the worker should exit (Disconnect requested),
/// `Ok(false)` otherwise.
///
/// 文件操作（List/Read/Write/Upload/Download/...）通过 `tokio::spawn` 分发到
/// 独立任务执行，主循环立即返回继续处理终端输入。这样单个慢速 SFTP 操作
/// 不会阻塞 `cmd_rx` 接收新的 `WriteTerminal` 命令——这是用户反馈"点上传
/// 后终端和文件都卡住"问题的根本修复。
#[allow(clippy::too_many_arguments)]
async fn handle_worker_cmd(
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
        WorkerCmd::StatRemoteFile {
            path,
            cancellation,
            respond_to,
        } => {
            // stat 也可能因 SFTP 卡住而阻塞，spawn 避免影响主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let result = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        stat_root_remote_file(&handle, &path, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(format!("获取文件{}信息超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, sftp_guard.metadata(&path)).await {
                        Ok(Ok(metadata)) if metadata.is_dir() => Ok(None),
                        Ok(Ok(metadata)) => Ok(Some(TransferFileStat {
                            size: metadata.size.unwrap_or(0),
                            modified_at: metadata.mtime.map(|value| value as u64 * 1000),
                        })),
                        Ok(Err(error)) if is_sftp_not_found(&error) => Ok(None),
                        Ok(Err(error)) => Err(error.to_string()),
                        Err(_) => Err(format!("获取文件{}信息超时", path)),
                    }
                };
                result
            });
            Ok(false)
        }
        WorkerCmd::UploadLocalFile {
            local_path,
            remote_path,
            resume_offset,
            transfer_id,
            cancel,
            respond_to,
        } => {
            // 上传可能持续数分钟，必须 spawn 到独立任务否则会阻塞整个 worker
            // 主循环，导致终端输入和文件浏览全部卡住。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            let checksum_timeout = operation_timeout;
            tokio::spawn(async move {
                // Root uploads are deliberately staged under /var/tmp first. The
                // login user's SFTP channel transfers the bulk bytes there,
                // then CommitRemoteStaging performs one short sudo/su command
                // to create the protected .fileterm-part. This keeps su out
                // of the data stream; its PTY/password semantics only apply
                // to the final privileged filesystem operation.
                let use_login_sftp_staging =
                    fam == "root" && is_root_upload_staging_path(&remote_path);
                let mut result = if use_login_sftp_staging || fam != "root" {
                    let transfer_sftp = acquire_transfer_sftp(
                        &handle,
                        &sftp,
                        &transfer_sftp_slot,
                        &app,
                        &tab_id,
                        operation_timeout,
                    )
                    .await;
                    let sftp_guard = transfer_sftp.write().await;
                    let result = upload_local_file(
                        &sftp_guard,
                        &local_path,
                        &remote_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                    )
                    .await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
                } else {
                    // Defensive fallback for legacy callers that do not use
                    // the root staging contract.
                    upload_root_local_file(
                        &handle,
                        &local_path,
                        &remote_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                        method,
                        &su,
                        &sp,
                    )
                    .await
                };
                if result.is_ok() {
                    result = match timeout(
                        checksum_timeout,
                        verify_sftp_transfer_sha256(
                            &handle,
                            &local_path,
                            &remote_path,
                            &fam,
                            method,
                            &su,
                            &sp,
                            checksum_timeout,
                        ),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err("SFTP 传输校验超时".to_string()),
                    };
                }
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::DownloadRemoteFile {
            remote_path,
            local_path,
            resume_offset,
            transfer_id,
            cancel,
            respond_to,
        } => {
            // 下载同样可能持续数分钟，必须 spawn。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            let checksum_timeout = operation_timeout;
            tokio::spawn(async move {
                let mut result = if fam == "root" {
                    download_root_remote_file(
                        &handle,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                        method,
                        &su,
                        &sp,
                    )
                    .await
                } else {
                    let transfer_sftp = acquire_transfer_sftp(
                        &handle,
                        &sftp,
                        &transfer_sftp_slot,
                        &app,
                        &tab_id,
                        operation_timeout,
                    )
                    .await;
                    let sftp_guard = transfer_sftp.write().await;
                    let result = download_remote_file(
                        &sftp_guard,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                    )
                    .await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
                };
                if result.is_ok() {
                    result = match timeout(
                        checksum_timeout,
                        verify_sftp_transfer_sha256(
                            &handle,
                            &local_path,
                            &remote_path,
                            &fam,
                            method,
                            &su,
                            &sp,
                            checksum_timeout,
                        ),
                    )
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => Err("SFTP 传输校验超时".to_string()),
                    };
                }
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::ReplaceRemoteFile {
            partial_path,
            destination_path,
            cancellation,
            respond_to,
        } => {
            // root 模式下需要 exec sudo/su mv，可能因认证或大文件 rename 慢，
            // spawn 避免阻塞主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let result = if fam == "root" {
                    replace_root_remote_file(
                        &handle,
                        &partial_path,
                        &destination_path,
                        method,
                        &su,
                        &sp,
                    )
                    .await
                } else {
                    let transfer_sftp = acquire_transfer_sftp(
                        &handle,
                        &sftp,
                        &transfer_sftp_slot,
                        &app,
                        &tab_id,
                        operation_timeout,
                    )
                    .await;
                    let sftp_guard = transfer_sftp.write().await;
                    let result =
                        replace_remote_file(&sftp_guard, &partial_path, &destination_path).await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
                };
                result
            });
            Ok(false)
        }
        WorkerCmd::CommitRemoteStaging {
            staging_path,
            partial_path,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let result = if fam == "root" {
                    commit_root_staging_file(
                        &handle,
                        &staging_path,
                        &partial_path,
                        method,
                        &su,
                        &sp,
                    )
                    .await
                } else {
                    Err("root staging 只能在 SSH root 文件模式下提交".to_string())
                };
                result
            });
            Ok(false)
        }
        WorkerCmd::RemoveRemoteFile {
            path,
            cancellation,
            respond_to,
        } => {
            // 单文件删除通常很快，但 SFTP 通道可能因前序操作卡住，spawn 避免
            // 阻塞主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let result = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!("rm -f -- {}", shell_quote(&path)),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()),
                        Err(_) => Err(format!("删除{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, async {
                        match sftp_guard.remove_file(&path).await {
                            Ok(()) => Ok(()),
                            Err(error) if is_sftp_not_found(&error) => Ok(()),
                            Err(error) => Err(error.to_string()),
                        }
                    })
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("删除{}超时", path)),
                    }
                };
                result
            });
            Ok(false)
        }
        WorkerCmd::ListRemoteFiles {
            path,
            cancellation,
            respond_to,
        } => {
            // spawn 避免阻塞主循环；timeout 防止 SFTP 卡住时任务永久挂起。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_list_dir_via_shell(&handle, &path, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程文件夹{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, list_dir(&sftp_guard, &path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程目录{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::ReadRemoteFile {
            path,
            encoding,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_read_file_via_shell(&handle, &path, &encoding, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, read_file(&sftp_guard, &path, &encoding)).await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile {
            path,
            content,
            encoding,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_write_file_via_shell(
                            &handle, &path, &content, &encoding, method, &su, &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        write_file(&sftp_guard, &path, &content, &encoding),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteDirectory {
            parent_path,
            name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!("mkdir -p {}", shell_quote(&full_path)),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, create_dir(&sftp_guard, &full_path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteFile {
            parent_path,
            name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_write_file_via_shell(
                            &handle, &full_path, "", "utf-8", method, &su, &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        write_file(&sftp_guard, &full_path, "", "utf-8"),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::CopyRemotePath {
            target_path,
            destination_path,
            target_type,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let dest_dir = std::path::Path::new(&destination_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let cp_cmd = if target_type == "folder" {
                    "cp -R"
                } else {
                    "cp"
                };
                let cmd_str = format!(
                    "mkdir -p {} && {} {} {}",
                    shell_quote(&dest_dir),
                    cp_cmd,
                    shell_quote(&target_path),
                    shell_quote(&destination_path)
                );
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                } else {
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::MoveRemotePath {
            target_path,
            destination_path,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!(
                                "mv {} {}",
                                shell_quote(&target_path),
                                shell_quote(&destination_path)
                            ),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        operation_timeout,
                        sftp_guard.rename(&target_path, &destination_path),
                    )
                    .await
                    {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::RenameRemotePath {
            target_path,
            new_name,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let parent = std::path::Path::new(&target_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let dest = format!("{}/{}", parent.trim_end_matches('/'), new_name);
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(
                            &handle,
                            &format!("mv {} {}", shell_quote(&target_path), shell_quote(&dest)),
                            method,
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(operation_timeout, sftp_guard.rename(&target_path, &dest)).await {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::DeleteRemotePath {
            target_path,
            target_type,
            target_is_symlink: _,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let cmd_str = if target_type == "folder" {
                    format!("rm -rf {}", shell_quote(&target_path))
                } else {
                    format!("rm -f {}", shell_quote(&target_path))
                };
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                } else {
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::ChangeRemotePermissions {
            target_path,
            permissions,
            recursive,
            apply_to,
            cancellation,
            respond_to,
        } => {
            // Mirrors Electron's `changeRemotePermissions`:
            // - `apply_to='all'` → `chmod -R` for recursive, plain `chmod` otherwise
            // - `apply_to='files'` → `chmod <mode> <path>` + `find <path> -type f -exec chmod <mode> {} +`
            // - `apply_to='directories'` → `chmod <mode> <path>` + `find <path> -type d -exec chmod <mode> {} +`
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let method = *root_file_access_method;
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let mode_str = format!("{:o}", permissions);
                let cmd_str = if !recursive {
                    format!("chmod {} {}", mode_str, shell_quote(&target_path))
                } else {
                    match apply_to.as_str() {
                        "files" => format!(
                            "chmod {} {} && find {} -type f -exec chmod {} {} +",
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        "directories" => format!(
                            "chmod {} {} && find {} -type d -exec chmod {} {} +",
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        _ => format!("chmod -R {} {}", mode_str, shell_quote(&target_path)),
                    }
                };
                let res = if fam == "root" {
                    match timeout(
                        operation_timeout,
                        exec_shell_file_command(&handle, &cmd_str, method, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                } else {
                    let wrapped = format!("sh -lc {}", shell_quote(&cmd_str));
                    match timeout(
                        operation_timeout,
                        crate::sessions::system_metrics::exec_command(&handle, &wrapped),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                };
                res
            });
            Ok(false)
        }
        WorkerCmd::SetRemoteFileAccessMode {
            mode,
            root_access_method: new_root_access_method,
            sudo_user: new_sudo_user,
            sudo_password: new_sudo_password,
            use_saved_password,
            respond_to,
        } => {
            if mode == "root" && !exec_channel_enabled {
                let _ = respond_to.send(Err(
                    "SSH Exec 通道已关闭，无法启用 root 文件视图。".to_string()
                ));
                return Ok(false);
            }
            let requested_access_method =
                match parse_root_file_access_method(new_root_access_method.as_deref()) {
                    Ok(method) => method,
                    Err(error) => {
                        let _ = respond_to.send(Err(error));
                        return Ok(false);
                    }
                };
            // 对照 Electron verifyRootFileAccess：切到 root 前先验证 sudo 凭据
            // 可用，失败则回滚状态并返回错误，让用户在弹窗里立即看到反馈，
            // 而不是等到第一次文件操作才失败（用户会以为"root 切换没接入"）。
            let prev_sudo_user = sudo_user.clone();
            let prev_sudo_password = sudo_password.clone();
            let prev_saved_sudo_password = saved_sudo_password.clone();
            let prev_saved_su_password = saved_su_password.clone();
            let prev_mode = file_access_mode.clone();
            let prev_access_method = *root_file_access_method;

            if let Some(next_user) = new_sudo_user.filter(|user| !user.trim().is_empty()) {
                *sudo_user = Some(next_user);
            }
            if mode == "root"
                && (use_saved_password
                    || requested_access_method != prev_access_method
                    || sudo_password.is_none())
            {
                *sudo_password = root_password_for_method(
                    requested_access_method,
                    saved_sudo_password,
                    saved_su_password,
                );
            }
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd.clone());
                    match requested_access_method {
                        RootFileAccessMethod::Sudo => *saved_sudo_password = Some(pwd),
                        RootFileAccessMethod::Su => *saved_su_password = Some(pwd),
                    }
                }
                // empty password ⇒ keep existing (cache reuse)
            }

            if mode == "root" {
                // 手动弹窗流程验证选择的 sudo/su 凭据，失败则回滚。`exec_shell_file_command`
                // 内部最长会等 SUDO_VERIFY_TIMEOUT（10s）才放弃，对 worker
                // 主循环来说太长——一旦 sudo 提示卡住或网络抖动，整个
                // 终端 select! 都停在这里，连 Ctrl+C 都进不去。这里用
                // ROOT_ACCESS_VERIFY_TIMEOUT（1.5s）做外层硬截断：超时
                // 同样视为验证失败并回滚，让用户拿到明确错误，而不是
                // 让 worker loop 沉默地阻塞数秒。
                let verify = match timeout(
                    ROOT_ACCESS_VERIFY_TIMEOUT,
                    exec_shell_file_command(
                        handle,
                        "true",
                        requested_access_method,
                        sudo_user,
                        sudo_password,
                    ),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_) => Err(format!(
                        "{} 验证超时：服务器未在 1.5 秒内响应",
                        root_file_access_method_label(requested_access_method)
                    )),
                };
                if let Err(err) = verify {
                    // 回滚到切换前的状态
                    *file_access_mode = prev_mode;
                    *root_file_access_method = prev_access_method;
                    *sudo_user = prev_sudo_user;
                    *sudo_password = prev_sudo_password;
                    *saved_sudo_password = prev_saved_sudo_password;
                    *saved_su_password = prev_saved_su_password;
                    let _ = respond_to.send(Err(err));
                    return Ok(false);
                }
                *root_file_access_method = requested_access_method;
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

// ─────────────────────────────────────────────────────────────────────────────
// SFTP helpers (russh-sftp 2.x)
// ─────────────────────────────────────────────────────────────────────────────
