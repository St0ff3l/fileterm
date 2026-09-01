pub fn start_ftp_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    crate::services::logging::session(&app, "INFO", "ftp", &tab_id, "worker starting");
    tauri::async_runtime::spawn(async move {
        let reconnect_mode = profile
            .get("reconnectMode")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let reconnect_policy = ReconnectPolicy::from_profile(&profile);
        let mut reconnect_attempt = 0;
        let mut command_rx = command_rx;
        loop {
            let result = {
                let run = run_ftp_worker(
                    &tab_id,
                    &profile,
                    &mut command_rx,
                    &app,
                    &mut reconnect_attempt,
                );
                tokio::select! {
                    result = run => result,
                    _ = cancellation.cancelled() => return,
                }
            };
            match result {
                Ok(()) => return,
                Err(error) if reconnect_mode == "auto" => {
                    let Some(attempt) = reconnect_policy.next_attempt(reconnect_attempt) else {
                        crate::services::logging::session(
                            &app,
                            "ERROR",
                            "ftp",
                            &tab_id,
                            format!("auto-reconnect limit reached: {error}"),
                        );
                        set_ftp_state(
                            &app,
                            &tab_id,
                            format!("FTP reconnect limit reached: {error}"),
                            WorkspaceTabStatus::Error,
                            None,
                            None,
                        )
                        .await;
                        return;
                    };
                    reconnect_attempt = attempt;
                    let delay = reconnect_policy.delay_for_attempt(attempt);
                    crate::services::logging::session(
                        &app,
                        "WARN",
                        "ftp",
                        &tab_id,
                        format!(
                            "auto-reconnect scheduled attempt={attempt} delay_ms={}",
                            delay.as_millis()
                        ),
                    );
                    set_ftp_state(
                        &app,
                        &tab_id,
                        format!("FTP reconnecting (attempt {attempt})"),
                        WorkspaceTabStatus::Connecting,
                        None,
                        None,
                    )
                    .await;
                    tokio::select! {
                        _ = sleep(delay) => {}
                        _ = cancellation.cancelled() => return,
                    }
                }
                Err(error) => {
                    crate::services::logging::session(&app, "ERROR", "ftp", &tab_id, &error);
                    set_ftp_state(
                        &app,
                        &tab_id,
                        format!("FTP error: {error}"),
                        WorkspaceTabStatus::Error,
                        None,
                        None,
                    )
                    .await;
                    return;
                }
            }
        }
    });
}

async fn run_ftp_worker(
    tab_id: &str,
    profile: &Value,
    command_rx: &mut mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    reconnect_attempt: &mut u32,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "FTP host is required".to_string())?;
    let port = port_from_profile(profile, 21, "FTP")?;
    let remote_path = profile
        .get("remotePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("/")
        .to_string();
    let mut client = connect_ftp(profile, host, port).await?;
    *reconnect_attempt = 0;
    let mut listing_state = FtpListingState::default();
    let initial_files = ftp_with_timeout(
        profile,
        "list",
        client_list(&mut client, &remote_path, &mut listing_state),
    )
    .await?;
    crate::services::logging::session(
        app,
        "INFO",
        "ftp",
        tab_id,
        format!(
            "connected host={host} port={port} entries={}",
            initial_files.len()
        ),
    );
    set_ftp_state(
        app,
        tab_id,
        format!("FTP {}:{}", host, port),
        WorkspaceTabStatus::Connected,
        Some(remote_path.clone()),
        Some(initial_files),
    )
    .await;
    let capabilities =
        match ftp_with_timeout(profile, "features", client_features(&mut client)).await {
            Ok(features) => ftp_capabilities_from_features(features),
            Err(error) if ftp_error_requires_reconnect(&error) => return Err(error),
            Err(_) => default_ftp_capabilities(),
        };
    set_ftp_capabilities(app, tab_id, capabilities).await;
    let mut transfer_jobs = tokio::task::JoinSet::new();
    let cleanup_app = app.clone();
    let cleanup_tab_id = tab_id.to_string();
    tokio::spawn(async move {
        if let Err(error) =
            crate::services::transfers::retry_pending_cleanup_for_tab(&cleanup_app, &cleanup_tab_id)
                .await
        {
            crate::services::logging::warn(
                &cleanup_app,
                &format!("transfer:{cleanup_tab_id}"),
                format!("pending cleanup retry failed: {error}"),
            );
        }
    });
    let keepalive = KeepalivePolicy::from_profile(profile);
    let mut keepalive_tick =
        tokio::time::interval(keepalive.interval.unwrap_or(Duration::from_secs(86400)));
    keepalive_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    keepalive_tick.tick().await;
    let mut keepalive_misses = 0_usize;

    loop {
        while transfer_jobs.try_join_next().is_some() {}
        let command = tokio::select! {
            command = command_rx.recv() => command,
            _ = keepalive_tick.tick(), if keepalive.interval.is_some() => {
                if keepalive_misses >= keepalive.max_misses {
                    return Err(format!("FTP keepalive failed after {} attempts", keepalive.max_misses));
                }
                match ftp_with_timeout(profile, "keepalive", client_noop(&mut client)).await {
                    Ok(()) => keepalive_misses = 0,
                    Err(error) => {
                        if ftp_error_requires_reconnect(&error) {
                            return Err(error);
                        }
                        keepalive_misses += 1;
                        crate::services::logging::session(
                            app,
                            "WARN",
                            "ftp",
                            tab_id,
                            format!("keepalive failed misses={keepalive_misses}: {error}"),
                        );
                    }
                }
                continue;
            }
        };
        match command {
            Some(WorkerCmd::ListRemoteFiles {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "list",
                    cancellation,
                    client_list(&mut client, &path, &mut listing_state),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ReadRemoteFile {
                path,
                encoding,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "read",
                    cancellation,
                    client_read(&mut client, &path, &encoding),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::WriteRemoteFile {
                path,
                content,
                encoding,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "write",
                    cancellation,
                    client_write(&mut client, &path, &content, &encoding),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CreateRemoteDirectory {
                parent_path,
                name,
                cancellation,
                respond_to,
                ..
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let result = ftp_with_cancellation(
                    profile,
                    "mkdir",
                    cancellation,
                    client_ensure_dir(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CreateRemoteFile {
                parent_path,
                name,
                cancellation,
                respond_to,
                ..
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let result = ftp_with_cancellation(
                    profile,
                    "create file",
                    cancellation,
                    client_write(&mut client, &path, "", "utf-8"),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CopyRemotePath { respond_to, .. }) => {
                let _ =
                    respond_to.send(Err("FTP 不支持服务器内复制，请改用下载后上传".to_string()));
            }
            Some(WorkerCmd::MoveRemotePath {
                target_path,
                destination_path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "rename",
                    cancellation,
                    client_rename(&mut client, &target_path, &destination_path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::RenameRemotePath {
                target_path,
                new_name,
                cancellation,
                respond_to,
                ..
            }) => {
                let destination = join_remote_path(&parent_remote_path(&target_path), &new_name);
                let result = ftp_with_cancellation(
                    profile,
                    "rename",
                    cancellation,
                    client_rename(&mut client, &target_path, &destination),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::DeleteRemotePath {
                target_path,
                target_type,
                target_is_symlink,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "delete",
                    cancellation,
                    client_delete(&mut client, &target_path, &target_type, target_is_symlink),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ChangeRemotePermissions {
                target_path,
                permissions,
                recursive,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = if recursive {
                    Err("FTP 暂不支持递归修改权限".to_string())
                } else {
                    ftp_with_cancellation(
                        profile,
                        "chmod",
                        cancellation,
                        client_chmod(&mut client, &target_path, permissions),
                    )
                    .await
                };
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::SetRemoteFileAccessMode {
                mode, respond_to, ..
            }) => {
                let result = if mode == "root" {
                    Err("FTP 不支持 SSH root 文件模式".to_string())
                } else {
                    Ok(())
                };
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::StatRemoteFile {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "stat",
                    cancellation,
                    client_stat(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::UploadLocalFile {
                local_path,
                remote_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let profile = profile.clone();
                let host = host.to_string();
                let app = app.clone();
                let tab_id = tab_id.to_string();
                transfer_jobs.spawn(async move {
                    let result = async {
                        let mut transfer_client = connect_ftp(&profile, &host, port).await?;
                        crate::services::logging::session(
                            &app,
                            "INFO",
                            "ftp",
                            &tab_id,
                            format!("dedicated upload connection opened transfer={transfer_id}"),
                        );
                        let transfer_timeout = seconds_from_profile(
                            &profile,
                            "operationTimeoutSeconds",
                            DEFAULT_FTP_OPERATION_TIMEOUT,
                            Duration::from_secs(5),
                            Duration::from_secs(3600),
                        );
                        let transfer_cancel = cancel.clone();
                        let mut result = client_upload(
                            &mut transfer_client,
                            &local_path,
                            &remote_path,
                            resume_offset,
                            &transfer_id,
                            cancel,
                            &app,
                            transfer_timeout,
                        )
                        .await;
                        if result.is_ok() && !transfer_cancel.is_cancelled() {
                            result = verify_ftp_transfer_checksum(
                                &mut transfer_client,
                                &local_path,
                                &remote_path,
                                transfer_timeout,
                            )
                            .await;
                        }
                        let _ = client_quit(&mut transfer_client).await;
                        result
                    }
                    .await;
                    let _ = respond_to.send(result);
                });
            }
            Some(WorkerCmd::DownloadRemoteFile {
                remote_path,
                local_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let profile = profile.clone();
                let host = host.to_string();
                let app = app.clone();
                let tab_id = tab_id.to_string();
                transfer_jobs.spawn(async move {
                    let result = async {
                        let mut transfer_client = connect_ftp(&profile, &host, port).await?;
                        crate::services::logging::session(
                            &app,
                            "INFO",
                            "ftp",
                            &tab_id,
                            format!("dedicated download connection opened transfer={transfer_id}"),
                        );
                        let transfer_timeout = seconds_from_profile(
                            &profile,
                            "operationTimeoutSeconds",
                            DEFAULT_FTP_OPERATION_TIMEOUT,
                            Duration::from_secs(5),
                            Duration::from_secs(3600),
                        );
                        let transfer_cancel = cancel.clone();
                        let mut result = client_download(
                            &mut transfer_client,
                            &remote_path,
                            &local_path,
                            resume_offset,
                            &transfer_id,
                            cancel,
                            &app,
                            transfer_timeout,
                        )
                        .await;
                        if result.is_ok() && !transfer_cancel.is_cancelled() {
                            result = verify_ftp_transfer_checksum(
                                &mut transfer_client,
                                &local_path,
                                &remote_path,
                                transfer_timeout,
                            )
                            .await;
                        }
                        let _ = client_quit(&mut transfer_client).await;
                        result
                    }
                    .await;
                    let _ = respond_to.send(result);
                });
            }
            Some(WorkerCmd::ReplaceRemoteFile {
                partial_path,
                destination_path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "replace",
                    cancellation,
                    client_replace(&mut client, &partial_path, &destination_path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CommitRemoteStaging { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不使用 SSH root staging 提交链路".to_string()));
            }
            Some(WorkerCmd::RemoveRemoteFile {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "remove",
                    cancellation,
                    client_remove(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ExecuteRemoteCommand { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持远程命令执行".to_string()));
            }
            Some(WorkerCmd::ListSshTunnels { respond_to })
            | Some(WorkerCmd::CreateSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StartSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StopSshTunnel { respond_to, .. })
            | Some(WorkerCmd::DeleteSshTunnel { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持 SSH 隧道".to_string()));
            }
            Some(WorkerCmd::SerialControl { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持串口控制".to_string()));
            }
            Some(WorkerCmd::SerialTransfer { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持串口文件传输".to_string()));
            }
            Some(WorkerCmd::WriteTerminal(_)) | Some(WorkerCmd::ResizeTerminal { .. }) => {}
            Some(WorkerCmd::Disconnect) | None => {
                crate::services::logging::session(app, "INFO", "ftp", tab_id, "disconnecting");
                transfer_jobs.abort_all();
                while transfer_jobs.join_next().await.is_some() {
                    // Drain aborted and already-completed jobs before the
                    // session worker releases its runtime state.
                }
                let _ = ftp_with_timeout(profile, "quit", client_quit(&mut client)).await;
                set_ftp_state(
                    app,
                    tab_id,
                    "FTP disconnected".to_string(),
                    WorkspaceTabStatus::Closed,
                    None,
                    None,
                )
                .await;
                return Ok(());
            }
        }
    }
}
