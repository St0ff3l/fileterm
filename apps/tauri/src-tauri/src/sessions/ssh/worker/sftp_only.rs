async fn run_sftp_only_worker_loop(
    tab_id: &str,
    profile: &Value,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<SshWorkerExit, String> {
    let cancellation = cancellation.child_token();
    let _connection_lifetime = cancellation.clone().drop_guard();
    let mut session_profile = profile.clone();
    session_profile["sftpEnabled"] = Value::Bool(true);
    session_profile["enableExecChannel"] = Value::Bool(false);
    session_profile["enableResourceMonitoring"] = Value::Bool(false);
    let mut transport_profile = session_profile.clone();
    transport_profile["type"] = Value::String("ssh".to_string());
    let host = session_profile
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = port_from_profile(&session_profile, 22, "SSH")?;
    let username = session_profile
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("root")
        .to_string();
    let started_at = Instant::now();
    let session = tokio::select! {
        biased;
        _ = cancellation.cancelled() => return Ok(SshWorkerExit::cancelled()),
        result = open_session(
            &transport_profile,
            app,
            tab_id,
            SSH_INTERACTION_TIMEOUT,
            Some("main".to_string()),
            SshInteractionFlow::new(),
            cancellation.clone(),
        ) => result,
    }?;
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        format!("SSH transport authenticated; opening SFTP subsystem host={host}:{port}"),
    );
    let handle = session.handle;
    let transfer_endpoint = session.transfer_endpoint;
    let disconnect_reason = session.disconnect_reason;
    let _session_cleanup = session.cleanup;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let operation_timeout = file_operation_timeout(&session_profile);
    let startup = SshWorkerStartupContext {
        app,
        tab_id,
        profile: &session_profile,
        handle: &handle,
        host: &host,
        port,
        username: &username,
        platform: "unknown",
        operation_timeout,
        network_device_mode: false,
        exec_channel_enabled: false,
        metrics_request_pty: false,
        interactive_gateway: false,
        route_hint: "sftp-only",
        cancellation: &cancellation,
        state: &state,
    };
    initialize_ssh_session_snapshot(&startup).await;
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.transfer_endpoint = Some(transfer_endpoint);
    }
    let (sftp, unavailable_reason) = initialize_sftp_session(&startup).await;
    let Some(sftp) = sftp else {
        return Err(unavailable_reason.unwrap_or_else(|| "SFTP subsystem is unavailable".to_string()));
    };
    update_tab_status_and_emit_without_snapshot(app, tab_id, WorkspaceTabStatus::Connected).await;
    schedule_workspace_snapshot_emit(app);
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        format!("SFTP-only session ready startup_elapsed_ms={}", started_at.elapsed().as_millis()),
    );

    let transfer_sftp_slot: TransferSftpSlot = Arc::new(Mutex::new(None));
    let (tunnel_command_tx, _tunnel_command_rx) = mpsc::unbounded_channel();
    let mut file_access_mode = "user".to_string();
    let mut root_file_access_method = RootFileAccessMethod::Sudo;
    let mut sudo_user = None;
    let mut sudo_password = None;
    let mut saved_sudo_password = None;
    let mut saved_su_password = None;
    let mut transport_check = tokio::time::interval(Duration::from_millis(250));
    transport_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;
            _ = cancellation.cancelled() => return Ok(SshWorkerExit::cancelled()),
            _ = transport_check.tick() => {
                if handle.is_closed() {
                    let disconnect = disconnect_reason
                        .lock()
                        .ok()
                        .and_then(|reason| reason.clone())
                        .unwrap_or_else(|| SshDisconnectInfo {
                            kind: SshDisconnectKind::Transport,
                            message: "transport closed without a disconnect callback".to_string(),
                        });
                    return Ok(SshWorkerExit::transport_closed(
                        disconnect,
                        started_at.elapsed() >= SSH_CONNECTION_STABILITY_WINDOW,
                    ));
                }
            }
            command = cmd_rx.recv() => {
                let Some(command) = command else {
                    return Ok(SshWorkerExit::input_closed());
                };
                match command {
                    WorkerCmd::Disconnect => return Ok(SshWorkerExit::explicit_disconnect()),
                    WorkerCmd::WriteTerminal(_) | WorkerCmd::ResizeTerminal { .. } => continue,
                    WorkerCmd::ExecuteRemoteCommand { respond_to, .. } => {
                        let _ = respond_to.send(Err("独立 SFTP 会话不提供终端命令".to_string()));
                        continue;
                    }
                    WorkerCmd::StartBackgroundRemoteCommand { respond_to, .. } => {
                        let _ = respond_to.send(Err("独立 SFTP 会话不提供后台命令".to_string()));
                        continue;
                    }
                    WorkerCmd::SerialControl { respond_to, .. } => {
                        let _ = respond_to.send(Err("SFTP 不支持串口控制".to_string()));
                        continue;
                    }
                    WorkerCmd::SerialTransfer { respond_to, .. } => {
                        let _ = respond_to.send(Err("SFTP 不支持串口文件传输".to_string()));
                        continue;
                    }
                    WorkerCmd::ListSshTunnels { respond_to }
                    | WorkerCmd::CreateSshTunnel { respond_to, .. }
                    | WorkerCmd::StartSshTunnel { respond_to, .. }
                    | WorkerCmd::StopSshTunnel { respond_to, .. }
                    | WorkerCmd::DeleteSshTunnel { respond_to, .. } => {
                        let _ = respond_to.send(Err("独立 SFTP 会话不支持 SSH 隧道".to_string()));
                        continue;
                    }
                    command => {
                        if dispatch_transfer_cmd(
                            command,
                            &handle,
                            &sftp,
                            &transfer_sftp_slot,
                            operation_timeout,
                            &mut file_access_mode,
                            &mut root_file_access_method,
                            &mut sudo_user,
                            &mut sudo_password,
                            &mut saved_sudo_password,
                            &mut saved_su_password,
                            tab_id,
                            app,
                            &state,
                            &tunnel_command_tx,
                            false,
                        ).await? {
                            return Ok(SshWorkerExit::explicit_disconnect());
                        }
                    }
                }
            }
        }
    }
}
