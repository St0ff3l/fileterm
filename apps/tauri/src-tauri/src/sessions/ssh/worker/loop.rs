async fn run_worker_loop(
    tab_id: &str,
    profile: &Value,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<SshWorkerExit, String> {
    // Every detached channel task belongs to this connection attempt. Drop
    // cancels pending listings/writers even when the remote shell exits first.
    let cancellation = cancellation.child_token();
    let _connection_lifetime = cancellation.clone().drop_guard();
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = port_from_profile(profile, 22, "SSH")?;
    let configured_username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root")
        .to_string();
    let normalized_username = crate::sessions::system_metrics::normalize_jumpserver_cli_username(
        &configured_username,
        &host,
    );
    let username_normalized = normalized_username.is_some();
    let configured_user_segments = configured_username
        .split(['@', '#'])
        .filter(|part| !part.trim().is_empty())
        .count();
    let mut routed_profile = profile.clone();
    if let Some(normalized_username) = normalized_username.as_ref() {
        routed_profile["username"] = Value::String(normalized_username.clone());
        crate::services::logging::session(
            app,
            "INFO",
            "ssh",
            tab_id,
            format!(
                "JumpServer username normalized source_shape=full-cli-destination target_shape=direct-asset configured_user_segments={} normalized_user_segments=3 host_match=true",
                configured_user_segments,
            ),
        );
    }
    let profile = &routed_profile;
    let username = normalized_username.unwrap_or(configured_username);
    let jump_host_configured = profile
        .get("jumpProfileId")
        .and_then(Value::as_str)
        .is_some();
    let direct_login_hint =
        crate::sessions::system_metrics::jumpserver_direct_login_hint(&username);
    let route_hint = if jump_host_configured {
        "transparent-jump-host"
    } else if direct_login_hint.is_some() {
        "jumpserver-direct-asset"
    } else {
        "direct-or-interactive"
    };
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "SSH route classified route_hint={route_hint} jump_profile_configured={jump_host_configured} direct_login_hint={} username_normalized={username_normalized} configured_user_segments={} direct_login_user_segments={}",
            direct_login_hint.unwrap_or("none"),
            configured_user_segments,
            username
                .split(['@', '#'])
                .filter(|part| !part.trim().is_empty())
                .count()
        ),
    );

    // ── Main session (one authenticated handle for shell + auxiliary channels)
    // Servers with strict MaxSessions reject parallel sessions, so we reuse
    // one authenticated handle for every channel when the target route is
    // known. Menu-driven gateways are the exception: their auxiliary channels
    // start a new unselected menu and are deferred after platform probing.
    let session = match tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err("SSH connection canceled".to_string()),
        result = open_session(
            profile,
            app,
            tab_id,
            SSH_INTERACTION_TIMEOUT,
            Some("main".to_string()),
            SshAuthenticationTarget::Direct,
            SshInteractionFlow::new(),
            cancellation.clone(),
        ) => result,
    } {
        Ok(h) => h,
        Err(error) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "ssh",
                tab_id,
                format!("open_session failed: {error}"),
            );
            return Err(error);
        }
    };
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!("SSH session established route_hint={route_hint}"),
    );
    let remote_sshid = session.remote_sshid;
    let disconnect_reason = session.disconnect_reason;
    let handle: Arc<Handle<ClientHandler>> = Arc::new(session.handle);
    let resolution = resolve_ssh_device_mode(profile, &remote_sshid);
    log_ssh_device_mode_resolution(app, tab_id, profile, &remote_sshid, resolution);
    let effective_profile = profile_with_resolved_device_mode(profile, resolution);
    let profile = &effective_profile;
    apply_resolved_device_mode_to_workspace(app, tab_id, profile).await;
    let network_device_mode = is_network_device_profile(profile);
    let exec_channel_enabled = effective_exec_channel_enabled(profile);
    let terminal_type = ssh_terminal_type(profile);
    let operation_timeout = file_operation_timeout(profile);
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "effective SSH session mode={} terminal_type={terminal_type}",
            resolution.mode.as_log_value()
        ),
    );

    // ── Shell channel ──────────────────────────────────────────────────────
    // 三步都加 timeout：服务器在 PTY 协商阶段卡住（嵌入式 dropbear /
    // 网络设备偶发）时 russh 默认无超时，会永久 await，worker 永远起
    // 不来，所有后续命令（含 Ctrl+C）都进不了 cmd_rx。
    let shell_channel = match timeout(SHELL_INIT_STEP_TIMEOUT, handle.channel_open_session()).await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            let msg = format!("无法打开 shell channel: {e}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 channel_open_session".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    };
    match timeout(
        SHELL_INIT_STEP_TIMEOUT,
        shell_channel.request_pty(
            true,
            terminal_type,
            80,
            24,
            0,
            0,
            &[
                (russh::Pty::TTY_OP_ISPEED, 115200),
                (russh::Pty::TTY_OP_OSPEED, 115200),
            ],
        ),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let msg = format!("request_pty failed: {err}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 request_pty".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    }
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!("pty requested terminal_type={terminal_type}"),
    );
    match timeout(SHELL_INIT_STEP_TIMEOUT, shell_channel.request_shell(true)).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let msg = format!("request_shell failed: {err}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 request_shell".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    }
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "shell channel ready");
    let (shell_reader, shell_writer) = shell_channel.split();
    let shell_writer = Arc::new(shell_writer);

    // Normal terminal bytes are serialized here so a slow SSH channel cannot
    // block the session event loop. Ctrl+C bypasses this queue below via the
    // SSH SIGINT request and also keeps its raw 0x03 byte as a fallback.
    let (terminal_write_tx, mut terminal_write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let terminal_writer = Arc::clone(&shell_writer);
    let terminal_writer_cancellation = cancellation.clone();
    let terminal_writer_app = app.clone();
    let terminal_writer_tab_id = tab_id.to_string();
    let _terminal_writer_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = terminal_writer_cancellation.cancelled() => break,
                data = terminal_write_rx.recv() => {
                    let Some(data) = data else { break };
                    if let Err(error) = write_shell_data(&terminal_writer, data).await {
                        crate::services::logging::session(
                            &terminal_writer_app,
                            "WARN",
                            "ssh",
                            &terminal_writer_tab_id,
                            format!("terminal write failed: {error}"),
                        );
                    }
                }
            }
        }
    });

    // Publish shell readiness immediately. Platform, SFTP and metrics are
    // optional and run concurrently with the terminal event loop.
    let connected_at = Instant::now();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let startup = SshWorkerStartupContext {
        app,
        tab_id,
        profile,
        handle: &handle,
        host: &host,
        port,
        username: &username,
        platform: "unknown",
        operation_timeout,
        network_device_mode,
        exec_channel_enabled,
        metrics_request_pty: false,
        interactive_gateway: false,
        route_hint,
        cancellation: &cancellation,
        state: &state,
    };
    initialize_ssh_session_snapshot(&startup).await;
    update_tab_status_and_emit(app, tab_id, WorkspaceTabStatus::Connected).await;
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;
    let transfer_sftp_slot: TransferSftpSlot = Arc::new(Mutex::new(None));
    let metrics_shutdown = Arc::new(tokio::sync::Notify::new());
    let (auxiliary_tx, auxiliary_rx) = oneshot::channel();
    let auxiliary_startup = async {
        let ready = include!("auxiliary_startup.rs");
        let _ = auxiliary_tx.send(ready);
    };
    let context = SshSessionContext {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile: (*profile).clone(),
        handle: Arc::clone(&handle),
        shell_writer,
        sftp: None,
        transfer_sftp_slot,
        operation_timeout,
        network_device_mode,
        exec_channel_enabled,
        sftp_unavailable_reason: Some("SFTP initialization in progress".to_string()),
        cancellation: cancellation.clone(),
        disconnect_reason,
        connected_at,
        metrics_shutdown: metrics_shutdown.clone(),
        shell_setup_script: None,
        terminal_write_tx,
    };
    run_with_auxiliary_startup(
        run_worker_event_loop(
            context,
            shell_reader,
            cmd_rx,
            terminal_input_rx,
            auxiliary_rx,
        ),
        auxiliary_startup,
    )
    .await
}
