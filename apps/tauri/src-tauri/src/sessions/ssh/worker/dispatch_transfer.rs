/// Dispatch the SSH worker commands owned by the transfer group.
#[allow(clippy::too_many_arguments)]
async fn dispatch_transfer_cmd(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
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
                    match timeout(
                        operation_timeout,
                        transfer_remote_metadata(&sftp_guard, &path),
                    )
                    .await
                    {
                        Ok(Ok(metadata)) if metadata.is_dir() => Ok(None),
                        Ok(Ok(metadata)) => Ok(Some(TransferFileStat {
                            size: metadata.size.unwrap_or(0),
                            modified_at: metadata.mtime.map(|value| value as u64 * 1000),
                        })),
                        Ok(Err(error)) if is_sftp_not_found(&error) => Ok(None),
                        Ok(Err(error)) => Err(sftp_transfer_error("STAT", &path, error)),
                        Err(_) => Err(sftp_transfer_error("STAT", &path, "timeout")),
                    }
                };
                result
            });
            Ok(false)
        }
        WorkerCmd::RelayFile {
            request,
            mut stream,
            cancellation,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            let root = file_access_mode == "root";
            spawn_cancellable_file_operation(cancellation.clone(), respond_to, async move {
                if root {
                    return Err("跨服务器传输暂不支持 root 文件模式".into());
                }
                let transfer =
                    acquire_transfer_sftp(&handle, &sftp, &slot, &app, &tab_id, operation_timeout)
                        .await;
                let guard = transfer.write().await;
                let result = relay_sftp_file(
                    &guard,
                    request,
                    &mut stream,
                    &cancellation,
                    operation_timeout,
                )
                .await;
                drop(guard);
                if result.is_err() {
                    invalidate_transfer_sftp(&transfer, &sftp, &slot).await;
                }
                result
            });
            Ok(false)
        }
        WorkerCmd::UploadLocalFile {
            local_path,
            source_size,
            remote_path,
            resume_offset,
            transfer_id,
            cancel,
            verify_checksum,
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
                let result = if use_login_sftp_staging || fam != "root" {
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-channel", timeout_secs = operation_timeout.as_secs(), verify_checksum, "SFTP transfer stage started");
                    let transfer_sftp = acquire_transfer_sftp(
                        &handle,
                        &sftp,
                        &transfer_sftp_slot,
                        &app,
                        &tab_id,
                        operation_timeout,
                    )
                    .await;
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-lock", "SFTP transfer stage started");
                    let sftp_guard = transfer_sftp.write().await;
                    let result = upload_local_file(
                        &sftp_guard,
                        &local_path,
                        source_size,
                        &remote_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        Some(&app),
                        operation_timeout,
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
                        source_size,
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
                let checksum_available = transfer_checksum_available(
                    exec_channel_enabled,
                    &fam,
                    app.state::<crate::services::workspace::WorkspaceState>()
                        .sessions
                        .read()
                        .await
                        .get(&tab_id)
                        .and_then(|session| session.remote_capabilities.as_ref())
                        .map(|caps| caps.checksum_algorithms.as_slice()),
                );
                let result = match result {
                    Ok(hash)
                        if (verify_checksum && checksum_available)
                            || (fam == "root" && !use_login_sftp_staging && resume_offset > 0) =>
                    {
                        tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "remote-checksum", "SFTP upload stage started");
                        match timeout(
                            checksum_timeout,
                            verify_sftp_transfer_sha256(
                                &handle,
                                &hash,
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
                            Ok(Ok(false))
                                if fam == "root"
                                    && !use_login_sftp_staging
                                    && resume_offset > 0 =>
                            {
                                Err("远端不支持 SHA-256，无法安全校验 root 上传断点，请从头传输"
                                    .to_string())
                            }
                            Ok(result) => result.map(|_| ()),
                            Err(_) => Err("SFTP 传输校验超时".to_string()),
                        }
                    }
                    Ok(_) => {
                        if verify_checksum && !checksum_available {
                            tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "checksum", reason = "SSH exec SHA-256 capability unavailable or unconfirmed", "SFTP checksum skipped; file size will still be verified");
                        }
                        Ok(())
                    }
                    Err(error) => Err(error),
                };
                match &result {
                    Ok(()) => {
                        tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-worker", "SFTP transfer worker completed")
                    }
                    Err(error) => {
                        tracing::warn!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-worker", %error, "SFTP transfer worker failed")
                    }
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
            verify_checksum,
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
                        cancel.clone(),
                        &app,
                        method,
                        &su,
                        &sp,
                    )
                    .await
                } else {
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-channel", timeout_secs = operation_timeout.as_secs(), verify_checksum, "SFTP transfer stage started");
                    let transfer_sftp = acquire_transfer_sftp(
                        &handle,
                        &sftp,
                        &transfer_sftp_slot,
                        &app,
                        &tab_id,
                        operation_timeout,
                    )
                    .await;
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-lock", "SFTP transfer stage started");
                    let sftp_guard = transfer_sftp.write().await;
                    let result = download_remote_file(
                        &sftp_guard,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel.clone(),
                        &app,
                        operation_timeout,
                    )
                    .await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
                };
                let checksum_available = transfer_checksum_available(
                    exec_channel_enabled,
                    &fam,
                    app.state::<crate::services::workspace::WorkspaceState>()
                        .sessions
                        .read()
                        .await
                        .get(&tab_id)
                        .and_then(|session| session.remote_capabilities.as_ref())
                        .map(|caps| caps.checksum_algorithms.as_slice()),
                );
                if result.is_ok() && verify_checksum && !checksum_available {
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "checksum", reason = "SSH exec SHA-256 capability unavailable or unconfirmed", "SFTP checksum skipped; file size will still be verified");
                }
                if result.is_ok() && verify_checksum && checksum_available {
                    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "local-checksum", "SFTP download stage started");
                    result = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
                        value = timeout(checksum_timeout, async {
                            let hash =
                                crate::sessions::file_integrity::sha256_file(&local_path).await?;
                            tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "local-checksum", "SFTP download stage completed");
                            tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "remote-checksum", "SFTP download stage started");
                            verify_sftp_transfer_sha256(
                                &handle,
                                &hash,
                                &remote_path,
                                &fam,
                                method,
                                &su,
                                &sp,
                                checksum_timeout,
                            )
                            .await?;
                            tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "remote-checksum", "SFTP download stage completed");
                            Ok(())
                        }) => match value {
                            Ok(result) => result,
                            Err(_) => Err("SFTP 传输校验超时".to_string()),
                        },
                    };
                }
                match &result {
                    Ok(()) => {
                        tracing::info!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-worker", "SFTP transfer worker completed")
                    }
                    Err(error) => {
                        tracing::warn!(category = "transfer", scope = "transfer", %transfer_id, %tab_id, stage = "transfer-worker", %error, "SFTP transfer worker failed")
                    }
                }
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::CommitRemoteFile {
            partial_path,
            destination_path,
            replace_existing,
            cancellation,
            respond_to,
        } => {
            let sftp = Arc::clone(sftp);
            let root = file_access_mode == "root";
            let no_delete = state.sessions.read().await.get(tab_id).map(|session| session.sftp_no_delete);
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                if root {
                    return Err("严格提交不支持 root 文件模式".into());
                }
                let no_delete = no_delete.ok_or("会话已关闭，无法确认 SFTP 安全设置")?;
                let guard = sftp.write().await;
                replace_remote_file_with_policy_mode(
                    &guard,
                    &partial_path,
                    &destination_path,
                    Some(replace_existing),
                    no_delete,
                )
                .await
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
            let no_delete = state.sessions.read().await.get(&tab_id).map(|session| session.sftp_no_delete);
            spawn_cancellable_file_operation(cancellation, respond_to, async move {
                let no_delete = no_delete.ok_or("会话已关闭，无法确认 SFTP 安全设置")?;
                let result = if fam == "root" {
                    if no_delete { return Err("禁止删除模式不支持 root 文件提交，请切换为普通文件访问模式".into()); }
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
                    let result = if no_delete {
                        replace_remote_file_with_policy_mode(
                            &sftp_guard, &partial_path, &destination_path, None, true,
                        ).await.map(|_| ())
                    } else {
                        replace_remote_file(&sftp_guard, &partial_path, &destination_path).await
                    };
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
                    match timeout(operation_timeout, remove_transfer_file_if_present(&sftp_guard, &path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(sftp_transfer_error("REMOVE cleanup", &path, "timeout")),
                    }
                };
                result
            });
            Ok(false)
        }
        _ => {
            dispatch_file_cmd(
                cmd,
                handle,
                sftp,
                transfer_sftp_slot,
                operation_timeout,
                file_access_mode,
                root_file_access_method,
                sudo_user,
                sudo_password,
                saved_sudo_password,
                saved_su_password,
                tab_id,
                app,
                state,
                tunnel_commands,
                exec_channel_enabled,
            )
            .await
        }
    }
}
