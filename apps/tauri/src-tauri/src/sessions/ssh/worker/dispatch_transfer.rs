/// Dispatch the SSH worker commands owned by the transfer group.
#[allow(clippy::too_many_arguments)]
async fn dispatch_transfer_cmd(
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
        _ => dispatch_file_cmd(cmd, handle, shell_writer, sftp, transfer_sftp_slot, operation_timeout, file_access_mode, root_file_access_method, sudo_user, sudo_password, saved_sudo_password, saved_su_password, tab_id, app, state, tunnel_commands, exec_channel_enabled).await,
    }
}
