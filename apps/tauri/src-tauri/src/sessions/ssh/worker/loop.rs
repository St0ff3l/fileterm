async fn run_worker_loop(
    tab_id: &str,
    profile: &Value,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<SshWorkerExit, String> {
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = port_from_profile(profile, 22, "SSH")?;
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root")
        .to_string();

    // ── Main session (single SSH session multiplexes shell + SFTP + metrics) ─
    // Servers with strict MaxSessions reject parallel sessions, so we reuse
    // one authenticated handle for every channel. The handle is wrapped in
    // `Arc` so the background metrics task can share it with the main loop.
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
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "SSH session established");
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

    // ── Probe platform ─────────────────────────────────────────────────────
    // 加 timeout：probe 内部最多 6 次串行 exec_command，每次都用
    // channel.wait() 循环读取且无内层 timeout。服务器在 exec 模式下卡住
    // 时整个 probe 会永久 await，worker 永远起不来。超时后回落到
    // "unknown"，shell CWD 注入会被 fail-closed 门控跳过，终端仍可用。
    let platform = if network_device_mode {
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "network-device mode; skipping platform probe",
        );
        "unknown".to_string()
    } else if exec_channel_enabled {
        crate::services::logging::session(
            app,
            "DEBUG",
            "metrics",
            tab_id,
            format!(
                "platform probe started exec_enabled=true timeout_secs={}",
                PLATFORM_PROBE_TIMEOUT.as_secs()
            ),
        );
        match timeout(
            PLATFORM_PROBE_TIMEOUT,
            crate::sessions::system_metrics::probe_remote_platform_for_session(
                &handle,
                Some(tab_id),
            ),
        )
        .await
        {
            Ok(p) => p,
            Err(_) => {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "metrics",
                    tab_id,
                    "platform probe timed out, falling back to unknown",
                );
                "unknown".to_string()
            }
        }
    } else {
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "exec channel disabled; skipping platform probe",
        );
        "unknown".to_string()
    };
    crate::services::logging::session(
        app,
        "INFO",
        "metrics",
        tab_id,
        format!("platform probe completed platform={platform}"),
    );

    // ── Inject shell CWD setup (POSIX only, fail-closed) ───────────────────
    // Mirrors Electron's `supportsPosixShellSetup()` + `injectShellSetup()`
    // double gate. Only `linux` / `busybox` get the OSC7/RemoteUser hook
    // injected; Windows / unknown are left untouched so we never push a
    // POSIX script into a non-POSIX shell.
    let shell_setup_script = if exec_channel_enabled {
        shell_cwd_setup_for_platform(&platform)
    } else {
        None
    };
    if let Some(setup) = shell_setup_script {
        crate::services::logging::session(
            app,
            "DEBUG",
            "ssh",
            tab_id,
            format!(
                "shell setup waiting for prompt platform={platform} bytes={}",
                setup.len()
            ),
        );
    } else {
        crate::services::logging::session(
            app,
            "DEBUG",
            "ssh",
            tab_id,
            format!("shell setup skipped platform={platform}"),
        );
    }

    update_tab_status_and_emit(app, tab_id, WorkspaceTabStatus::Connected).await;
    let connected_at = Instant::now();

    // Emit "connected" notice so the user sees confirmation in the terminal.
    // Mirrors Electron's `appendSystemMessage('连接主机成功\r\n')`.
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let startup = SshWorkerStartupContext {
        app,
        tab_id,
        profile,
        handle: &handle,
        host: &host,
        port,
        username: &username,
        platform: &platform,
        operation_timeout,
        network_device_mode,
        exec_channel_enabled,
        cancellation: &cancellation,
        state: &state,
    };
    initialize_ssh_session_snapshot(&startup).await;
    let (sftp_arc, sftp_unavailable_reason) = initialize_sftp_session(&startup).await;
    let transfer_sftp_slot: TransferSftpSlot = Arc::new(Mutex::new(None));

    // Push the full snapshot (with files) to the renderer
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
    if sftp_arc.is_some() {
        let cleanup_app = app.clone();
        let cleanup_tab_id = tab_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = crate::services::transfers::retry_pending_cleanup_for_tab(
                &cleanup_app,
                &cleanup_tab_id,
            )
            .await
            {
                crate::services::logging::warn(
                    &cleanup_app,
                    &format!("transfer:{cleanup_tab_id}"),
                    format!("pending cleanup retry failed: {error}"),
                );
            }
        });
    }

    // ── Spawn metrics collection task (single persistent channel) ─────────
    let metrics_shutdown = Arc::new(tokio::sync::Notify::new());
    spawn_metrics_collector(&startup, metrics_shutdown.clone()).await;
    let context = SshSessionContext {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile: (*profile).clone(),
        handle: Arc::clone(&handle),
        shell_writer,
        sftp: sftp_arc,
        transfer_sftp_slot,
        operation_timeout,
        network_device_mode,
        exec_channel_enabled,
        sftp_unavailable_reason,
        cancellation: cancellation.clone(),
        disconnect_reason,
        connected_at,
        metrics_shutdown,
        shell_setup_script,
        terminal_write_tx,
    };
    run_worker_event_loop(context, shell_reader, cmd_rx, terminal_input_rx).await
}
