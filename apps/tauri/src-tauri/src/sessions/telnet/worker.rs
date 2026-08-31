pub fn start_telnet_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    crate::services::logging::session(&app, "INFO", "telnet", &tab_id, "worker starting");
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
                let run = run_telnet_worker(
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
                            "telnet",
                            &tab_id,
                            format!("auto-reconnect limit reached: {error}"),
                        );
                        emit_terminal_data(
                            &app,
                            &tab_id,
                            &format!("\r\n[Telnet] reconnect-limit: {error}\r\n"),
                        )
                        .await;
                        set_terminal_state(
                            &app,
                            &tab_id,
                            format!("Telnet reconnect limit reached: {error}"),
                            WorkspaceTabStatus::Error,
                        )
                        .await;
                        return;
                    };
                    reconnect_attempt = attempt;
                    let delay = reconnect_policy.delay_for_attempt(attempt);
                    crate::services::logging::session(
                        &app,
                        "WARN",
                        "telnet",
                        &tab_id,
                        format!(
                            "auto-reconnect scheduled attempt={attempt} delay_ms={}",
                            delay.as_millis()
                        ),
                    );
                    set_terminal_state(
                        &app,
                        &tab_id,
                        format!("Telnet reconnecting (attempt {attempt})"),
                        WorkspaceTabStatus::Connecting,
                    )
                    .await;
                    emit_terminal_data(
                        &app,
                        &tab_id,
                        &format!(
                            "\r\n[Telnet] reconnect-scheduled: {} {attempt}\r\n",
                            delay.as_secs(),
                        ),
                    )
                    .await;
                    tokio::select! {
                        _ = sleep(delay) => {}
                        _ = cancellation.cancelled() => return,
                    }
                }
                Err(error) => {
                    crate::services::logging::session(&app, "ERROR", "telnet", &tab_id, &error);
                    emit_terminal_data(&app, &tab_id, &format!("\r\n[Telnet] {error}\r\n")).await;
                    set_terminal_state(
                        &app,
                        &tab_id,
                        format!("Telnet error: {error}"),
                        WorkspaceTabStatus::Error,
                    )
                    .await;
                    return;
                }
            }
        }
    });
}

async fn run_telnet_worker(
    tab_id: &str,
    profile: &Value,
    command_rx: &mut mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    reconnect_attempt: &mut u32,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1");
    let port = port_from_profile(profile, 23, "Telnet")?;
    let encoding = profile
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-8")
        .to_string();
    let terminal_type = profile
        .get("terminalType")
        .and_then(Value::as_str)
        .unwrap_or("xterm-256color");
    let newline_mode = profile
        .get("newlineMode")
        .and_then(Value::as_str)
        .unwrap_or("crlf");
    let cr_nul = profile
        .get("crNul")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let login_script = profile
        .get("loginScript")
        .and_then(Value::as_str)
        .map(parse_login_script)
        .unwrap_or_default();
    let keepalive = KeepalivePolicy::from_profile(profile);
    let stream = connect_transport(profile, host, port).await?;
    crate::services::logging::session(
        app,
        "INFO",
        "telnet",
        tab_id,
        format!("connected host={host} port={port}"),
    );
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut parser = TelnetParser::new(terminal_type);
    set_terminal_state(
        app,
        tab_id,
        format!("Telnet {host}:{port}"),
        WorkspaceTabStatus::Connected,
    )
    .await;
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut keepalive_tick =
        tokio::time::interval(keepalive.interval.unwrap_or(Duration::from_secs(86400)));
    keepalive_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    keepalive_tick.tick().await;
    let mut keepalive_misses = 0_usize;
    let mut login_timer = Box::pin(sleep(Duration::from_millis(250)));
    let mut login_pending = !login_script.is_empty();
    let connected_at = Instant::now();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCmd::WriteTerminal(data)) => {
                        let bytes = encode_terminal(&data, &encoding);
                        let encoded = encode_telnet_input(&bytes, newline_mode, cr_nul, parser.transmit_binary);
                        write_telnet(&mut writer, &encoded).await?;
                    }
                    Some(WorkerCmd::ResizeTerminal { cols, rows, .. }) => {
                        let packet = parser.set_size(cols, rows);
                        if !packet.is_empty() {
                            write_telnet(&mut writer, &packet).await?;
                        }
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        crate::services::logging::session(app, "INFO", "telnet", tab_id, "disconnecting");
                        let _ = timeout(TELNET_WRITE_TIMEOUT, writer.shutdown()).await;
                        set_terminal_state(app, tab_id, "Telnet disconnected".to_string(), WorkspaceTabStatus::Closed).await;
                        return Ok(());
                    }
                    Some(command) => reject_unsupported(command, "Telnet 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                let count = read.map_err(|error| error.to_string())?;
                if count == 0 {
                    crate::services::logging::session(app, "WARN", "telnet", tab_id, "remote closed connection");
                    if connected_at.elapsed() >= Duration::from_secs(10) {
                        *reconnect_attempt = 0;
                    }
                    return Err("Telnet remote closed the connection".to_string());
                }
                keepalive_misses = 0;
                let (visible, writes) = parser.feed(&buffer[..count]);
                for write in writes {
                    write_telnet(&mut writer, &write).await?;
                }
                if !visible.is_empty() {
                    emit_terminal_data(app, tab_id, &decode_terminal(&visible, &encoding)).await;
                }
            }
            _ = keepalive_tick.tick(), if keepalive.interval.is_some() => {
                if keepalive_misses >= keepalive.max_misses {
                    return Err(format!("Telnet keepalive failed after {} attempts", keepalive.max_misses));
                }
                write_telnet(&mut writer, &[IAC, AYT]).await?;
                keepalive_misses += 1;
            }
            _ = &mut login_timer, if login_pending => {
                for line in &login_script {
                    // A script entry is a command line, not a raw fragment.
                    // Append one logical LF and let the selected Telnet
                    // newline policy encode it for the server.
                    let mut command = line.as_bytes().to_vec();
                    command.push(b'\n');
                    let encoded = encode_telnet_input(&command, newline_mode, cr_nul, parser.transmit_binary);
                    write_telnet(&mut writer, &encoded).await?;
                }
                login_pending = false;
            }
        }
    }
}
