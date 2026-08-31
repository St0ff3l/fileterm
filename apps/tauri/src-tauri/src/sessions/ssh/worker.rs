async fn run_worker_loop(
    tab_id: &str,
    profile: &Value,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<(), String> {
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
    let session = match open_session(
        profile,
        app,
        tab_id,
        SSH_INTERACTION_TIMEOUT,
        Some("main".to_string()),
        SshAuthenticationTarget::Direct,
    )
    .await
    {
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
    let (mut shell_reader, shell_writer) = shell_channel.split();
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
    // 加 timeout：probe 内部最多 4 次串行 exec_command，每次都用
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
        match timeout(
            PLATFORM_PROBE_TIMEOUT,
            crate::sessions::system_metrics::probe_remote_platform(&handle),
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
    let mut pending_shell_setup_echo = None;
    let mut shell_setup_waiting_for_prompt = shell_setup_script.is_some();
    let mut shell_prompt_buffer = String::new();
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

    // Emit "connected" notice so the user sees confirmation in the terminal.
    // Mirrors Electron's `appendSystemMessage('连接主机成功\r\n')`.
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;

    // ── Initialize session snapshot ────────────────────────────────────────
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut sessions = state.sessions.write().await;
        let existing_transcript = sessions
            .get(tab_id)
            .map(|s| s.terminal_transcript.clone())
            .unwrap_or_default();
        let existing_reconnect_mode = sessions
            .get(tab_id)
            .and_then(|session| session.reconnect_mode.clone());
        let existing_remote_path = sessions
            .get(tab_id)
            .map(|session| session.remote_path.clone())
            .unwrap_or_else(|| {
                crate::services::workspace::initial_remote_path_for_profile(profile)
            });
        let existing_shell_cwd = if network_device_mode {
            None
        } else {
            sessions
                .get(tab_id)
                .and_then(|session| session.shell_cwd.clone())
        };
        let mut capabilities =
            crate::services::workspace::ConnectionCapabilities::for_profile(profile);
        if !exec_channel_enabled {
            capabilities.resource_monitoring = false;
            capabilities.shell_integration = false;
        }
        sessions.insert(
            tab_id.to_string(),
            crate::services::SessionSnapshot {
                profile_id: profile
                    .get("id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string(),
                ai_session_revision: state.ai_session_revision(tab_id).await.to_string(),
                device_mode: crate::services::workspace::configured_device_mode_for_profile(
                    profile,
                ),
                access_host: format!("{}:{}", host, port),
                summary: format!("{}@{}", username, host),
                terminal_transcript: existing_transcript,
                remote_path: existing_remote_path,
                shell_cwd: existing_shell_cwd,
                follow_shell_cwd: exec_channel_enabled,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                // A saved sudo password is already a reusable credential for
                // the file toolbar. Keep only this non-secret presence bit in
                // the public snapshot; the password itself stays worker-local.
                has_reusable_sudo_auth: !network_device_mode
                    && profile
                        .get("sudoPassword")
                        .and_then(Value::as_str)
                        .is_some_and(|password| !password.is_empty()),
                login_user: profile
                    .get("username")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                shell_user: None,
                connected: true,
                system_metrics: None,
                capabilities,
                remote_capabilities: None,
                reconnect_mode: existing_reconnect_mode
                    .or_else(|| crate::services::workspace::reconnect_mode_for_profile(profile)),
            },
        );
    }

    state
        .connection_operations
        .publish_for_tab(
            tab_id,
            crate::services::connection_operations::ConnectionOperationState::Connected,
        )
        .await;

    // ── SFTP subsystem ─────────────────────────────────────────────────────
    // russh-sftp 2.3 needs an explicit subsystem request before converting
    // the channel into its protocol stream. A failed SFTP negotiation must
    // not tear down an otherwise healthy SSH shell: Electron keeps terminal
    // and tunnel features available while exposing the file-channel error.
    let sftp_enabled = effective_sftp_enabled(profile);
    let (sftp_arc, sftp_unavailable_reason) = if network_device_mode {
        crate::services::logging::session(
            app,
            "INFO",
            "sftp",
            tab_id,
            "network-device mode; skipping SFTP channel",
        );
        (None, None)
    } else if !sftp_enabled {
        let reason = "SFTP disabled for this connection profile".to_string();
        crate::services::logging::session(
            app,
            "INFO",
            "sftp",
            tab_id,
            "disabled by connection profile",
        );
        {
            let mut sessions = state.sessions.write().await;
            if let Some(session) = sessions.get_mut(tab_id) {
                session.sftp_unavailable_reason = Some(reason.clone());
                session.capabilities.files = false;
                session.capabilities.file_access = false;
            }
        }
        emit_terminal_data(app, tab_id, &format!("\r\n[files] {reason}\r\n")).await;
        (None, Some(reason))
    } else {
        match open_sftp_session(&handle, operation_timeout).await {
            Ok(sftp) => {
                crate::services::logging::session(
                    app,
                    "INFO",
                    "sftp",
                    tab_id,
                    "SFTP session ready",
                );
                let sftp_arc = Arc::new(RwLock::new(sftp));
                let configured_initial_remote_path = {
                    let sessions = state.sessions.read().await;
                    sessions
                        .get(tab_id)
                        .map(|session| session.remote_path.clone())
                        .unwrap_or_else(|| {
                            crate::services::workspace::initial_remote_path_for_profile(profile)
                        })
                };
                let initial_remote_path = if is_implicit_ssh_home_path(
                    &configured_initial_remote_path,
                ) {
                    match resolve_initial_sftp_home_path(&sftp_arc, operation_timeout).await {
                        Ok(resolved_path) => {
                            crate::services::logging::ssh_debug(
                                app,
                                tab_id,
                                format!(
                                    "initial SFTP home resolved configured={} resolved={resolved_path}",
                                    configured_initial_remote_path
                                ),
                            );
                            resolved_path
                        }
                        Err(error) => {
                            crate::services::logging::ssh_debug(
                                app,
                                tab_id,
                                format!(
                                    "initial SFTP home resolution failed configured={} error={error}; using configured path",
                                    configured_initial_remote_path
                                ),
                            );
                            configured_initial_remote_path.clone()
                        }
                    }
                } else {
                    configured_initial_remote_path.clone()
                };
                {
                    let mut sessions = state.sessions.write().await;
                    if let Some(session) = sessions.get_mut(tab_id) {
                        if session.remote_path == configured_initial_remote_path {
                            session.remote_path = initial_remote_path.clone();
                        }
                        session.remote_capabilities = Some(default_sftp_capabilities());
                    }
                }
                // A server can accept the SFTP subsystem and then stop replying
                // to read_dir. Do not await the initial directory load before the
                // terminal select loop: otherwise Ctrl+C reaches IPC but cannot be
                // consumed until the SFTP request returns. The bound includes both
                // the lock wait and read_dir; the task publishes its own snapshot.
                {
                    let mut sessions = state.sessions.write().await;
                    if let Some(session) = sessions.get_mut(tab_id) {
                        session.remote_files_loading = true;
                    }
                }
                let initial_sftp = Arc::clone(&sftp_arc);
                let initial_handle = Arc::clone(&handle);
                let initial_app = app.clone();
                let initial_tab_id = tab_id.to_string();
                let initial_cancellation = cancellation.clone();
                let initial_listing_timeout = operation_timeout.min(INITIAL_SFTP_LISTING_TIMEOUT);
                tokio::spawn(async move {
                    crate::services::logging::ssh_debug(
                        &initial_app,
                        &initial_tab_id,
                        format!(
                            "initial directory listing started path={initial_remote_path} timeout_secs={}",
                            initial_listing_timeout.as_secs()
                        ),
                    );
                    let initial_files = tokio::select! {
                        _ = initial_cancellation.cancelled() => {
                            let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                            if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                                session.remote_files_loading = false;
                            }
                            if let Ok(snapshot) =
                                crate::commands::get_workspace_snapshot(initial_app.clone()).await
                            {
                                let _ = initial_app.emit("workspace:snapshot", snapshot);
                            }
                            crate::services::logging::ssh_debug(
                                &initial_app,
                                &initial_tab_id,
                                "initial directory listing cancelled",
                            );
                            return;
                        },
                        result = timeout(initial_listing_timeout, async {
                            let sftp = initial_sftp.write().await;
                            list_dir(&sftp, &initial_remote_path).await
                        }) => match result {
                            Ok(result) => result,
                            Err(_) => Err(format!("列出远程目录 {initial_remote_path} 超时")),
                        },
                    };

                    let initial_listing_error = initial_files.as_ref().err().cloned();
                    let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                    let mut initial_listing_is_current = false;
                    let mut initial_listing_fallback_used = false;
                    if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                        initial_listing_is_current = initial_remote_listing_matches_current_session(
                            &initial_remote_path,
                            &session.remote_path,
                            session.shell_cwd.as_deref(),
                            session.follow_shell_cwd,
                        );
                        if initial_listing_is_current {
                            session.remote_files_loading = false;
                            if let Ok(files) = &initial_files {
                                session.remote_files = files.clone();
                            }
                        } else {
                            if initial_remote_listing_can_be_fallback(
                                initial_listing_is_current,
                                &initial_remote_path,
                                &session.remote_path,
                                session.remote_files.is_empty(),
                            ) {
                                // A shell CWD may be outside the SFTP user's
                                // namespace (Synology commonly chroots SFTP
                                // at the user's home). Keep a successful
                                // listing of the visible SFTP path instead of
                                // leaving the pane empty after CWD follow fails.
                                if let Ok(files) = &initial_files {
                                    session.remote_files = files.clone();
                                    initial_listing_fallback_used = true;
                                }
                            }
                            // A manual navigation, a completed CWD follow, or
                            // an unmapped shell CWD owns the final state. The
                            // detached startup request must never leave the
                            // file pane loading forever.
                            session.remote_files_loading = false;
                        }
                    }

                    match &initial_files {
                        Ok(files) => crate::services::logging::ssh_debug(
                            &initial_app,
                            &initial_tab_id,
                            format!(
                                "initial directory listing completed path={initial_remote_path} entries={} current={initial_listing_is_current} fallback={initial_listing_fallback_used}",
                                files.len()
                            ),
                        ),
                        Err(error) => crate::services::logging::session(
                            &initial_app,
                            "WARN",
                            "sftp",
                            &initial_tab_id,
                            format!(
                                "initial directory listing failed path={initial_remote_path} current={initial_listing_is_current}: {error}"
                            ),
                        ),
                    }

                    if initial_listing_is_current {
                        if let Some(error) = initial_listing_error {
                            // A usable SFTP channel can still lack access to the
                            // profile's configured starting directory.
                            emit_terminal_data(
                                &initial_app,
                                &initial_tab_id,
                                &format!(
                                    "\r\n[files] 列出目录 {initial_remote_path} 失败: {error}\r\n"
                                ),
                            )
                            .await;
                        }
                    }

                    // Publish the directory result before running optional
                    // capability probes. fs_info/readlink/hardlink and the
                    // SSH exec probe are best-effort metadata; a slow or
                    // restricted server must not keep usable file rows behind
                    // the loading spinner.
                    if let Ok(snapshot) =
                        crate::commands::get_workspace_snapshot(initial_app.clone()).await
                    {
                        let _ = initial_app.emit("workspace:snapshot", snapshot);
                    }

                    if initial_cancellation.is_cancelled() {
                        return;
                    }

                    crate::services::logging::ssh_debug(
                        &initial_app,
                        &initial_tab_id,
                        format!(
                            "initial capability probes started path={initial_remote_path} exec_enabled={exec_channel_enabled} timeout_secs={}",
                            operation_timeout
                                .min(INITIAL_CAPABILITY_PROBE_TIMEOUT)
                                .as_secs()
                        ),
                    );
                    let capability_timeout =
                        operation_timeout.min(INITIAL_CAPABILITY_PROBE_TIMEOUT);
                    let mut remote_capabilities = tokio::select! {
                        _ = initial_cancellation.cancelled() => return,
                        result = timeout(capability_timeout, async {
                            let sftp = initial_sftp.write().await;
                            inspect_sftp_capabilities(&sftp, &initial_remote_path).await
                        }) => match result {
                            Ok(capabilities) => capabilities,
                            Err(_) => {
                                crate::services::logging::session(
                                    &initial_app,
                                    "WARN",
                                    "sftp",
                                    &initial_tab_id,
                                    format!(
                                        "initial SFTP capability probe timed out path={initial_remote_path} timeout_secs={}",
                                        capability_timeout.as_secs()
                                    ),
                                );
                                default_sftp_capabilities()
                            },
                        },
                    };
                    let (server_copy, checksum_algorithms) = if exec_channel_enabled {
                        tokio::select! {
                            _ = initial_cancellation.cancelled() => return,
                            result = inspect_ssh_exec_capabilities(&initial_handle, capability_timeout) => result,
                        }
                    } else {
                        (false, Vec::new())
                    };
                    remote_capabilities.server_copy = server_copy;
                    remote_capabilities.checksum_algorithms = checksum_algorithms;

                    // The initial probe is deliberately detached from the
                    // terminal select loop, but it must not publish results
                    // after this worker has been stopped for a reconnect or
                    // tab close. Otherwise a slow old SFTP probe can overwrite
                    // the snapshot of the replacement session.
                    if initial_cancellation.is_cancelled() {
                        return;
                    }

                    let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                    if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                        session.remote_capabilities = Some(remote_capabilities.clone());
                    }
                    crate::services::logging::ssh_debug(
                        &initial_app,
                        &initial_tab_id,
                        format!(
                            "initial capability probes completed path={initial_remote_path} server_copy={} checksums={}",
                            remote_capabilities.server_copy,
                            remote_capabilities.checksum_algorithms.len()
                        ),
                    );
                    if let Ok(snapshot) =
                        crate::commands::get_workspace_snapshot(initial_app.clone()).await
                    {
                        let _ = initial_app.emit("workspace:snapshot", snapshot);
                    }
                });
                (Some(sftp_arc), None)
            }
            Err(error) => {
                let reason = format_sftp_unavailable_reason(&error);
                crate::services::logging::session(
                    app,
                    "WARN",
                    "sftp",
                    tab_id,
                    format!("unavailable: {reason}"),
                );
                {
                    let mut sessions = state.sessions.write().await;
                    if let Some(session) = sessions.get_mut(tab_id) {
                        session.sftp_unavailable_reason = Some(reason.clone());
                        // The interactive SSH shell is still usable, but the
                        // file capability must reflect the failed subsystem
                        // handshake. Leaving it enabled makes the renderer
                        // offer file actions that can only return the cached
                        // SFTP error.
                        session.capabilities.files = false;
                        session.capabilities.file_access = false;
                    }
                }
                emit_terminal_data(app, tab_id, &format!("\r\n[files] {reason}\r\n")).await;
                (None, Some(reason))
            }
        }
    };
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
    // Instead of opening a new exec channel every second (which adds variable
    // SSH overhead and makes the refresh cadence jittery), we open one
    // long-lived shell channel and pipe an infinite-loop script into it.
    // The remote side controls the 1s cadence via `sleep 1`, so data arrives
    // at a rock-steady interval regardless of SSH RTT.
    let metrics_shutdown = Arc::new(tokio::sync::Notify::new());
    if effective_resource_monitoring_enabled(profile) {
        let metrics_shutdown_clone = metrics_shutdown.clone();
        let metrics_handle = Arc::clone(&handle);
        let metrics_app = app.clone();
        let metrics_tid = tab_id.to_string();
        let metrics_plat = platform.clone();
        let metrics_interval_seconds = resource_monitoring_interval_seconds(profile);
        let metrics_cancellation = cancellation.clone();
        tokio::spawn(async move {
            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                format!("collector starting platform={metrics_plat} interval_seconds={metrics_interval_seconds}"),
            );

            // Build the infinite-loop script. Each iteration emits a
            // delimited metrics block and sleeps for 1 second. We use a
            // unique marker so the stream parser can reliably slice blocks.
            let marker = "__FILETERM_METRICS_BLOCK__";
            let (windows_command, script_body) = if metrics_plat == "windows" {
                let command =
                    match crate::sessions::system_metrics::build_windows_streaming_metrics_exec_command(
                        metrics_interval_seconds,
                    ) {
                        Ok(command) => command,
                        Err(error) => {
                            disable_resource_monitoring_capability(
                                &metrics_app,
                                &metrics_tid,
                                format!("Windows streaming command build failed: {error}"),
                            )
                            .await;
                            return;
                        }
                    };
                (Some(command), None)
            } else {
                // POSIX: wrap the metrics script in a while-true loop
                let metrics = if metrics_plat == "freebsd" {
                    crate::sessions::system_metrics::build_freebsd_metrics_command()
                } else {
                    let raw = if metrics_plat == "busybox" {
                        "busybox"
                    } else {
                        "linux"
                    };
                    crate::sessions::system_metrics::build_posix_metrics_command(raw)
                };
                let script = format!(
                    "{}\nwhile true; do\n{}\necho '{}'\nsleep {}\ndone\n",
                    "cd / >/dev/null 2>&1 || true", metrics, marker, metrics_interval_seconds
                );
                (None, Some(script))
            };

            // Open one persistent shell channel for the entire session.
            // 加 timeout：服务器 MaxSessions 满或网络抖动时这一步会卡住，
            // 不加超时 metrics task 会永久 await，虽然不阻塞主循环，但
            // 用户看不到系统监控数据且 worker 不会自动重试。
            let mut channel = match timeout(
                SHELL_INIT_STEP_TIMEOUT,
                metrics_handle.channel_open_session(),
            )
            .await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    disable_resource_monitoring_capability(
                        &metrics_app,
                        &metrics_tid,
                        format!("open channel failed: {e}"),
                    )
                    .await;
                    return;
                }
                Err(_) => {
                    disable_resource_monitoring_capability(
                        &metrics_app,
                        &metrics_tid,
                        "open channel timed out",
                    )
                    .await;
                    return;
                }
            };

            // Windows OpenSSH on this host stalls when a large script is sent
            // through stdin. Match Electron's transport: gzip + base64 keeps
            // the loader below cmd.exe's safe command-line budget, while the
            // decoded script runs as one persistent PowerShell process.
            let collector_start = if let Some(command) = windows_command.as_deref() {
                timeout(SHELL_INIT_STEP_TIMEOUT, channel.exec(true, command)).await
            } else {
                timeout(SHELL_INIT_STEP_TIMEOUT, channel.request_shell(true)).await
            };
            let collector_start = match collector_start {
                Ok(inner) => inner,
                Err(_) => {
                    disable_resource_monitoring_capability(
                        &metrics_app,
                        &metrics_tid,
                        "start collector timed out",
                    )
                    .await;
                    return;
                }
            };
            if let Err(e) = collector_start {
                disable_resource_monitoring_capability(
                    &metrics_app,
                    &metrics_tid,
                    format!("start collector failed: {e}"),
                )
                .await;
                return;
            }

            if let Some(script) = script_body.as_deref() {
                // 写脚本也加 timeout：Windows OpenSSH 在大脚本场景偶发 stall，
                // 不加超时会让 metrics task 永久卡在 data() 调用上。
                match timeout(SHELL_INIT_STEP_TIMEOUT, channel.data(script.as_bytes())).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        disable_resource_monitoring_capability(
                            &metrics_app,
                            &metrics_tid,
                            format!("write collector script failed: {e}"),
                        )
                        .await;
                        return;
                    }
                    Err(_) => {
                        disable_resource_monitoring_capability(
                            &metrics_app,
                            &metrics_tid,
                            "write collector script timed out",
                        )
                        .await;
                        return;
                    }
                }
            }

            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                "collector started; waiting for first sample",
            );

            // Stream reader: accumulate data, split on the marker, parse
            // each complete block and emit it to the renderer.
            let mut buffer: Vec<u8> = Vec::new();
            let marker_bytes = marker.as_bytes();
            let mut sample_count = 0_u64;

            loop {
                tokio::select! {
                    biased;
                    _ = metrics_shutdown_clone.notified() => {
                        let _ = channel.close().await;
                        break;
                    }
                    _ = metrics_cancellation.cancelled() => {
                        let _ = channel.close().await;
                        break;
                    }
                    msg = channel.wait() => {
                        match msg {
                            Some(ChannelMsg::Data { data }) => {
                                buffer.extend_from_slice(data.as_ref());
                                // Drain all complete blocks from the buffer.
                                while let Some(idx) = find_subsequence(&buffer, marker_bytes) {
                                    // A malformed or unexpectedly large process list must not
                                    // monopolize the Tokio worker and freeze the native webview.
                                    // Keep one bounded metrics sample; the next marker resumes
                                    // normal streaming collection.
                                    if idx > 256 * 1024 {
                                        buffer.drain(..idx + marker_bytes.len());
                                        continue;
                                    }
                                    let block = String::from_utf8_lossy(&buffer[..idx]).into_owned();
                                    buffer.drain(..idx + marker_bytes.len());
                                    // Parse and emit this block
                                    let val = crate::sessions::system_metrics::parse_system_metrics(
                                        &block,
                                        &metrics_plat,
                                    );
                                    let cpu_pct = val.get("cpuPercent").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                                    let mem_pct = val.get("memoryPercent").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                                    if cpu_pct < 0.0 && mem_pct < 0.0 {
                                        // Probably garbage / incomplete block
                                        continue;
                                    }
                                    sample_count += 1;
                                    if sample_count == 1 {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "INFO",
                                            "metrics",
                                            &metrics_tid,
                                            format!("first sample cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1}"),
                                        );
                                    }
                                    {
                                        let state = metrics_app
                                            .state::<crate::services::workspace::WorkspaceState>();
                                        let mut sessions = state.sessions.write().await;
                                        if let Some(s) = sessions.get_mut(&metrics_tid) {
                                            s.system_metrics = Some(merge_system_metrics_history(
                                                s.system_metrics.as_ref(),
                                                val.clone(),
                                                600,
                                            ));
                                        }
                                    }
                                    let payload = serde_json::json!({
                                        "tabId": metrics_tid,
                                        "systemMetrics": val,
                                        "mode": "append",
                                    });
                                    let _ = metrics_app.emit("workspace:sessionMetrics", payload);
                                }
                                // Cap buffer to prevent unbounded growth
                                if buffer.len() > 1_000_000 {
                                    buffer.drain(..buffer.len() - 500_000);
                                }
                            }
                            Some(ChannelMsg::ExtendedData { data, .. }) => {
                                buffer.extend_from_slice(data.as_ref());
                            }
                            Some(ChannelMsg::ExitStatus { .. }) | None => {
                                if !metrics_cancellation.is_cancelled() {
                                    disable_resource_monitoring_capability(
                                        &metrics_app,
                                        &metrics_tid,
                                        "collector channel closed",
                                    )
                                    .await;
                                }
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            let _ = channel.close().await;
            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                "collector stopped",
            );
        });
    } else {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.system_metrics = None;
        }
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "collection disabled by profile",
        );
    }

    // ── Main event loop: terminal reads + command dispatch ─────────────────
    let mut cwd_buffer = String::new();
    // `__tdcwd` prints OSC7 (CWD) immediately before OSC1337 (user). SSH can
    // split those two markers into separate packets; defer a CWD-only event
    // until its matching user marker arrives so a root transition cannot
    // briefly browse through the stale sudo method.
    let mut pending_cwd_marker_without_user: Option<String> = None;
    let mut batch_buffer: Vec<u8> = Vec::new();
    let mut last_emit = Instant::now();
    // User input must not enter the PTY while the first prompt is being
    // identified or while the internal setup command is executing. Otherwise
    // the shell echoes a literal `#` into the same stream that the prompt
    // heuristic inspects and the setup command races with that input.
    let mut deferred_terminal_input: Vec<Vec<u8>> = Vec::new();

    // Terminal output pump: 解耦 worker 主循环与 renderer IPC 推送。
    // flush_batch 用 try_send 把 chunk 推到这个 bounded channel，独立的
    // pump task 异步消费并调 emit_terminal_data（含 channel.send + RwLock
    // 写）。这样高吞吐输出（pacman-key --populate）时 worker 主循环的
    // select! 永远不会被 IPC 推送或 RwLock 竞争阻塞，Ctrl+C 路径始终
    // 畅通。通道满时丢弃旧 chunk（终端输出是尽力而为的，丢几帧不影响
    // 功能，但 Ctrl+C 必须响应）。容量 128 覆盖 16ms × 8MB/s 的峰值。
    let (terminal_output_tx, mut terminal_output_rx) = tokio::sync::mpsc::channel::<String>(128);
    let pump_app = app.clone();
    let pump_tab_id = tab_id.to_string();
    let _pump_handle = tokio::spawn(async move {
        while let Some(chunk) = terminal_output_rx.recv().await {
            emit_terminal_data(&pump_app, &pump_tab_id, &chunk).await;
        }
    });

    // sudo / root-mode credentials — kept in worker-local state so they
    // never leak into SessionSnapshot (which is serialized to the renderer).
    let mut file_access_mode = "user".to_string();
    let mut sudo_user: Option<String> = None;
    let mut sudo_password = profile
        .get("sudoPassword")
        .and_then(Value::as_str)
        .filter(|password| !password.is_empty())
        .map(str::to_string);
    let mut su_password = profile
        .get("suPassword")
        .and_then(Value::as_str)
        .filter(|password| !password.is_empty())
        .map(str::to_string);
    // File operations receive the credential matching the currently selected
    // root method. Keep this active value separate from the two profile
    // caches so switching sudo ↔ su cannot reuse the wrong password.
    let mut root_password = sudo_password.clone();
    let mut sudo_prompt_buffer = String::new();
    let mut awaiting_root_access_auth: Option<PendingRootAccessAuth> = None;
    let mut pending_sudo_password = String::new();
    let mut recent_terminal_input = String::new();
    let mut pending_root_access_command: Option<PendingRootAccessAuth> = None;
    let mut last_authenticated_root_access: Option<PendingRootAccessAuth> = None;
    let mut root_file_access_method = RootFileAccessMethod::Sudo;
    // A new `sudo -i` shell discards the login shell's PROMPT_COMMAND.  Keep
    // Electron's two-second guard so a root prompt causes one safe reinject
    // of the OSC CWD/RemoteUser hook, not an injection loop.
    let mut last_shell_setup_injection = Instant::now() - Duration::from_secs(3);

    let mut tunnel_manager = TunnelManager::new(tab_id, app, Arc::clone(&handle));
    let mut auto_start_tunnel_ids = Vec::new();
    if let Some(rules) = profile.get("forwards").and_then(Value::as_array) {
        for raw_rule in rules {
            match serde_json::from_value::<SshTunnelRule>(raw_rule.clone()) {
                Ok(rule) => {
                    let should_start = rule.auto_start;
                    if let Err(error) = tunnel_manager.register(rule.clone(), false) {
                        emit_terminal_data(
                            app,
                            tab_id,
                            &format!("[tunnel] 忽略无效规则: {error}\r\n"),
                        )
                        .await;
                    } else if should_start {
                        auto_start_tunnel_ids.push(rule.id);
                    }
                }
                Err(error) => {
                    emit_terminal_data(app, tab_id, &format!("[tunnel] 解析规则失败: {error}\r\n"))
                        .await
                }
            }
        }
    }
    // Keep potentially slow tunnel control operations out of the terminal
    // worker. The queue preserves command order (for example Start → Stop)
    // while its own task absorbs server-side request/cancel waits.
    let (tunnel_command_tx, tunnel_command_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_tunnel_command_loop(tunnel_manager, tunnel_command_rx));
    for rule_id in auto_start_tunnel_ids {
        let (respond_to, response_rx) = oneshot::channel();
        enqueue_tunnel_command(
            &tunnel_command_tx,
            TunnelCommand::Start {
                rule_id: rule_id.clone(),
                respond_to,
            },
        );
        let auto_tunnel_app = app.clone();
        let auto_tunnel_tab_id = tab_id.to_string();
        tokio::spawn(async move {
            match response_rx.await {
                Ok(Err(error)) => {
                    emit_terminal_data(
                        &auto_tunnel_app,
                        &auto_tunnel_tab_id,
                        &format!("[tunnel] 自动启动 {rule_id} 失败: {error}\r\n"),
                    )
                    .await;
                }
                Err(_) => {
                    crate::services::logging::session(
                        &auto_tunnel_app,
                        "WARN",
                        "tunnel",
                        &auto_tunnel_tab_id,
                        format!("auto-start response dropped id={rule_id}"),
                    );
                }
                Ok(Ok(_)) => {}
            }
        });
    }

    // Start the prompt wait only after the worker has finished opening its
    // auxiliary channels and is about to enter the fair terminal loop.
    let mut shell_setup_prompt_deadline =
        shell_setup_script.map(|_| Instant::now() + SHELL_SETUP_PROMPT_TIMEOUT);

    loop {
        // 16ms batch window for terminal output.
        let next_batch_deadline =
            tokio::time::Instant::from_std(last_emit + Duration::from_millis(16));

        tokio::select! {
            _ = cancellation.cancelled() => {
                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                metrics_shutdown.notify_waiters();
                return Ok(());
            }
            input = terminal_input_rx.recv() => {
                let Some(data) = input else {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    metrics_shutdown.notify_waiters();
                    return Ok(());
                };
                let data = coalesce_terminal_input(data, terminal_input_rx);
                if should_buffer_terminal_input_during_shell_setup(
                    shell_setup_waiting_for_prompt,
                    pending_shell_setup_echo.is_some(),
                    &data,
                ) {
                    deferred_terminal_input.push(data.into_bytes());
                    continue;
                }
                if contains_interrupt_byte(&data) {
                    // Ctrl+C is the escape hatch for a setup command that is
                    // taking too long. Discard text typed before it rather
                    // than replaying a stale partial command after recovery.
                    deferred_terminal_input.clear();
                }
                if !network_device_mode {
                    let previous_pending_command = pending_root_access_command.clone();
                    if capture_root_access_password_input(
                        &data,
                        &mut awaiting_root_access_auth,
                        &mut pending_sudo_password,
                        &mut recent_terminal_input,
                        &mut root_password,
                        &mut last_authenticated_root_access,
                        &mut pending_root_access_command,
                    ) {
                        cache_root_password_for_auth(
                            last_authenticated_root_access.as_ref(),
                            &root_password,
                            &mut sudo_password,
                            &mut su_password,
                        );
                        let mut sessions = state.sessions.write().await;
                        if let Some(session) = sessions.get_mut(tab_id) {
                            session.has_reusable_sudo_auth = matches!(
                                last_authenticated_root_access.as_ref(),
                                Some(auth) if auth.method == RootFileAccessMethod::Sudo
                            ) && root_password.is_some();
                        }
                    }
                    if pending_root_access_command != previous_pending_command {
                        if let Some(auth) = pending_root_access_command.as_ref() {
                            crate::services::logging::ssh_debug(
                                app,
                                tab_id,
                                format!(
                                    "interactive privilege command tracked method={:?} target_user={} interactive_shell={}",
                                    auth.method, auth.target_user, auth.interactive_shell
                                ),
                            );
                        }
                    }
                }
                if contains_interrupt_byte(&data) {
                    // Fire-and-forget: the SIGINT request used to be awaited
                    // inline for up to TERMINAL_INTERRUPT_TIMEOUT (500ms).
                    // Under high-throughput shell output that 500ms stalled
                    // the next `select!` iteration, so a second Ctrl+C press
                    // was effectively swallowed. Spinning the signal off to
                    // its own task lets the main loop immediately poll
                    // `terminal_input_rx` again for follow-up interrupts.
                    let sigint_writer = Arc::clone(&shell_writer);
                    let sigint_app = app.clone();
                    let sigint_tab_id = tab_id.to_string();
                    tokio::spawn(async move {
                        match timeout(
                            TERMINAL_INTERRUPT_TIMEOUT,
                            sigint_writer.signal(Sig::INT),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                crate::services::logging::session(
                                    &sigint_app,
                                    "WARN",
                                    "ssh",
                                    &sigint_tab_id,
                                    format!("terminal SIGINT request failed: {error}"),
                                );
                            }
                            Err(_) => {
                                crate::services::logging::session(
                                    &sigint_app,
                                    "WARN",
                                    "ssh",
                                    &sigint_tab_id,
                                    "terminal SIGINT request timed out",
                                );
                            }
                        }
                    });
                }
                terminal_write_tx
                    .send(data.into_bytes())
                    .map_err(|_| "Terminal writer stopped".to_string())?;
            }
            // Commands and shell output intentionally share Tokio's fair
            // selection. Making this branch unconditionally preferred lets a
            // stream of Enter keypresses starve both shell reads and the 16ms
            // output flush, so the terminal appears to freeze and then jumps.
            // When the sender is dropped (reconnect / disconnect / close),
            // `recv()` returns None and we must exit — otherwise the old
            // worker keeps publishing terminal output alongside the new worker.
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(cmd) => {
                        if !network_device_mode {
                            if let WorkerCmd::WriteTerminal(data) = &cmd {
                                let previous_pending_command = pending_root_access_command.clone();
                                if capture_root_access_password_input(
                                    data,
                                    &mut awaiting_root_access_auth,
                                    &mut pending_sudo_password,
                                    &mut recent_terminal_input,
                                    &mut root_password,
                                    &mut last_authenticated_root_access,
                                    &mut pending_root_access_command,
                                ) {
                                    cache_root_password_for_auth(
                                        last_authenticated_root_access.as_ref(),
                                        &root_password,
                                        &mut sudo_password,
                                        &mut su_password,
                                    );
                                    let mut sessions = state.sessions.write().await;
                                    if let Some(session) = sessions.get_mut(tab_id) {
                                        session.has_reusable_sudo_auth = matches!(
                                            last_authenticated_root_access.as_ref(),
                                            Some(auth) if auth.method == RootFileAccessMethod::Sudo
                                        ) && root_password.is_some();
                                    }
                                }
                                if pending_root_access_command != previous_pending_command {
                                    if let Some(auth) = pending_root_access_command.as_ref() {
                                        crate::services::logging::ssh_debug(
                                            app,
                                            tab_id,
                                            format!(
                                                "interactive privilege command tracked method={:?} target_user={} interactive_shell={}",
                                                auth.method, auth.target_user, auth.interactive_shell
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        let result = if let Some(sftp) = sftp_arc.as_ref() {
                            handle_worker_cmd(
                                cmd,
                                &handle,
                                &shell_writer,
                                sftp,
                                &transfer_sftp_slot,
                                operation_timeout,
                                &mut file_access_mode,
                                &mut root_file_access_method,
                                &mut sudo_user,
                                &mut root_password,
                                &mut sudo_password,
                                &mut su_password,
                                tab_id,
                                app,
                                &state,
                                &tunnel_command_tx,
                                exec_channel_enabled,
                            ).await
                        } else {
                            handle_worker_cmd_without_sftp(
                                cmd,
                                &handle,
                                &shell_writer,
                                &mut file_access_mode,
                                &mut root_file_access_method,
                                &mut sudo_user,
                                &mut root_password,
                                &mut sudo_password,
                                &mut su_password,
                                tab_id,
                                &state,
                                &tunnel_command_tx,
                                sftp_unavailable_reason.as_deref().unwrap_or(SFTP_UNAVAILABLE_FALLBACK),
                                exec_channel_enabled,
                            ).await
                        };
                        match result {
                            Ok(true) => {
                                // WorkerCmd::Disconnect requested — flush and exit.
                                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                                metrics_shutdown.notify_waiters();
                                return Ok(());
                            }
                            Ok(false) => {}
                            Err(e) => {
                                crate::services::logging::session(app, "WARN", "ssh", tab_id, format!("command failed: {e}"));
                            }
                        }
                    }
                    None => {
                        // Sender dropped — flush and exit cleanly.
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
                        return Ok(());
                    }
                }
            }
            _ = async {
                if let Some(deadline) = shell_setup_prompt_deadline {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }, if shell_setup_waiting_for_prompt => {
                // A server may expose a non-standard prompt or never emit one
                // at all (for example a login shell that starts a full-screen
                // program). Do not hold user keystrokes forever; abandon the
                // optional integration and leave the PTY untouched.
                shell_setup_waiting_for_prompt = false;
                shell_setup_prompt_deadline = None;
                shell_prompt_buffer.clear();
                flush_deferred_terminal_input(
                    &mut deferred_terminal_input,
                    &terminal_write_tx,
                )?;
                crate::services::logging::session(
                    app,
                    "DEBUG",
                    "ssh",
                    tab_id,
                    "shell setup prompt wait timed out; continuing without injection",
                );
            }
            _ = async {
                if let Some(deadline) = shell_setup_release_deadline(&pending_shell_setup_echo) {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }, if pending_shell_setup_echo.is_some() => {
                let visible = finish_shell_setup_suppression(&mut pending_shell_setup_echo);
                if !visible.is_empty() {
                    batch_buffer.extend_from_slice(visible.as_bytes());
                }
                flush_deferred_terminal_input(
                    &mut deferred_terminal_input,
                    &terminal_write_tx,
                )?;
            }
            // 2. Drain shell channel output.
            msg = shell_reader.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let bytes = data.as_ref();
                        let text = String::from_utf8_lossy(bytes);
                        let (new_cwd, new_user) = if network_device_mode {
                            (None, None)
                        } else {
                            let previous_awaiting_auth = awaiting_root_access_auth.clone();
                            if track_root_access_prompt_from_terminal(
                                &text,
                                &mut sudo_prompt_buffer,
                                &mut awaiting_root_access_auth,
                                &mut pending_sudo_password,
                                &mut root_password,
                                &mut last_authenticated_root_access,
                                &mut pending_root_access_command,
                            ) {
                                cache_root_password_for_auth(
                                    last_authenticated_root_access.as_ref(),
                                    &root_password,
                                    &mut sudo_password,
                                    &mut su_password,
                                );
                                let mut sessions = state.sessions.write().await;
                                if let Some(session) = sessions.get_mut(tab_id) {
                                    session.has_reusable_sudo_auth = false;
                                }
                            }
                            if autofill_root_access_password(
                                &shell_writer,
                                &mut awaiting_root_access_auth,
                                &mut pending_sudo_password,
                                &mut root_password,
                                &sudo_password,
                                &su_password,
                            )
                            .await?
                            {
                                crate::services::logging::ssh_debug(
                                    app,
                                    tab_id,
                                    "interactive privilege password filled from connection profile",
                                );
                            }
                            if awaiting_root_access_auth != previous_awaiting_auth {
                                if let Some(auth) = awaiting_root_access_auth.as_ref() {
                                    crate::services::logging::ssh_debug(
                                        app,
                                        tab_id,
                                        format!(
                                            "root auth prompt tracked method={:?} target_user={} pending_command={:?}",
                                            auth.method,
                                            auth.target_user,
                                            pending_root_access_command
                                                .as_ref()
                                                .map(|pending| pending.method)
                                        ),
                                    );
                                }
                            }
                            track_cwd_and_user(&text, &mut cwd_buffer)
                        };
                        let deferred_cwd_to_follow = if new_user.is_some() {
                            pending_cwd_marker_without_user.take()
                        } else {
                            if let Some(cwd) = new_cwd.as_ref() {
                                pending_cwd_marker_without_user = Some(cwd.clone());
                            }
                            None
                        };
                        let mut cwd_to_follow = None;
                        let mut file_mode_switch: Option<(
                            String,
                            Option<String>,
                            RootFileAccessMethod,
                        )> = None;
                        let mut session_state_changed = false;
                        let mut ai_target_changed = false;
                        if new_cwd.is_some() || new_user.is_some() {
                            let mut sessions = state.sessions.write().await;
                            if let Some(s) = sessions.get_mut(tab_id) {
                                if let Some(cwd) = new_cwd.as_ref() {
                                    if s.shell_cwd.as_deref() != Some(cwd.as_str()) {
                                        crate::services::logging::ssh_debug(
                                            app,
                                            tab_id,
                                            format!("Shell CWD reported: {cwd}"),
                                        );
                                        s.shell_cwd = Some(cwd.clone());
                                        session_state_changed = true;
                                        ai_target_changed = true;
                                        // When the user marker is in a later
                                        // packet, wait for that packet before
                                        // opening the root exec channel.
                                        if s.follow_shell_cwd && new_user.is_some() {
                                            cwd_to_follow = Some(cwd.clone());
                                        }
                                    }
                                }
                                if let Some(user) = &new_user {
                                    // 首次观察到 RemoteUser 时记录为 login_user
                                    // （若 profile.username 不可用则用观察值）。
                                    if s.login_user.is_none() {
                                        s.login_user = Some(user.clone());
                                        ai_target_changed = true;
                                    }
                                    let shell_user_changed =
                                        s.shell_user.as_deref() != Some(user.as_str());
                                    if shell_user_changed {
                                        s.shell_user = Some(user.clone());
                                        session_state_changed = true;
                                        ai_target_changed = true;
                                    }
                                    // 对照 Electron resolveShellFileAccess：
                                    // shell user != login user ⇒ 自动切 root 视角
                                    // shell user == login user ⇒ 切回 user 视角。
                                    // 即使 RemoteUser 没变化也要重新同步提权方式：连续
                                    // sudo/su 都可能上报 root，但独立 exec 必须采用本次方法。
                                    let login = s.login_user.clone();
                                    if let Some(login_user) = login {
                                        let (target_mode, observed_sudo_user) =
                                            resolve_shell_file_access(&login_user, user);
                                        if new_cwd.is_none() {
                                            if let Some(deferred_cwd) =
                                                deferred_cwd_to_follow.as_ref()
                                            {
                                                if s.follow_shell_cwd {
                                                    cwd_to_follow = Some(deferred_cwd.clone());
                                                }
                                            }
                                        }
                                        // Only a shell identity transition may auto-switch the
                                        // visible file mode. Repeated RemoteUser markers from the
                                        // same root shell must not undo a manual user/root choice.
                                        let mode_changed =
                                            shell_user_changed && s.file_access_mode != target_mode;
                                        if mode_changed {
                                            s.file_access_mode = target_mode.to_string();
                                        }
                                        if let Some(observed_sudo_user) = observed_sudo_user {
                                            let access_method = root_access_method_for_shell_user(
                                                &observed_sudo_user,
                                                last_authenticated_root_access.as_ref(),
                                                pending_root_access_command.as_ref(),
                                            );
                                            crate::services::logging::ssh_debug(
                                                app,
                                                tab_id,
                                                format!(
                                                    "RemoteUser sync login_user={} shell_user={} target_mode={} method={:?} pending_method={:?} authenticated_method={:?} password_cached={}",
                                                    login_user,
                                                    user,
                                                    target_mode,
                                                    access_method,
                                                    pending_root_access_command
                                                        .as_ref()
                                                        .map(|auth| auth.method),
                                                    last_authenticated_root_access
                                                        .as_ref()
                                                        .map(|auth| auth.method),
                                                    root_password_for_method(
                                                        access_method,
                                                        &sudo_password,
                                                        &su_password,
                                                    )
                                                    .is_some(),
                                                ),
                                            );
                                            let access_changed =
                                                root_file_access_method != access_method
                                                    || sudo_user.as_deref()
                                                        != Some(observed_sudo_user.as_str());
                                            s.sudo_user = Some(observed_sudo_user.clone());
                                            s.has_reusable_sudo_auth =
                                                access_method == RootFileAccessMethod::Sudo
                                                    && root_password_for_method(
                                                        access_method,
                                                        &sudo_password,
                                                        &su_password,
                                                    )
                                                    .is_some();
                                            if access_changed {
                                                session_state_changed = true;
                                            }
                                            if mode_changed || access_changed {
                                                let next_mode = if mode_changed {
                                                    target_mode.to_string()
                                                } else {
                                                    file_access_mode.clone()
                                                };
                                                file_mode_switch = Some((
                                                    next_mode,
                                                    Some(observed_sudo_user),
                                                    access_method,
                                                ));
                                            }
                                        } else if mode_changed {
                                            // `exit` 回到登录用户时必须立即恢复 user
                                            // 视角。保留 sudo_user / 密码缓存只用于下次
                                            // 手动切 root，不得让工具栏继续显示 root。
                                            file_mode_switch = Some((
                                                target_mode.to_string(),
                                                s.sudo_user.clone(),
                                                RootFileAccessMethod::Sudo,
                                            ));
                                        }
                                        if shell_user_changed && target_mode == "user" {
                                            // The interactive root shell ended. Do not let the
                                            // previous `su -` command influence a later root
                                            // marker that belongs to a new transition.
                                            pending_root_access_command = None;
                                            pending_sudo_password.clear();
                                        }
                                        if mode_changed || file_mode_switch.is_some() {
                                            // 身份或提权方式变化即使没有伴随 CWD 变化也要刷新
                                            // 当前目录，确保列表内容和访问模型同步切换。
                                            cwd_to_follow = s.shell_cwd.clone();
                                        }
                                    }
                                }
                            }
                            drop(sessions);
                            if ai_target_changed {
                                state.touch_ai_session_revision(tab_id).await;
                            }
                            // Keep worker-local auth/access state in lockstep
                            // before dispatching the follow task below.
                            if let Some((mode, su_user, access_method)) = file_mode_switch {
                                file_access_mode = mode;
                                sudo_user = su_user;
                                root_file_access_method = access_method;
                                root_password =
                                    root_password_for_method(access_method, &sudo_password, &su_password);
                            }
                            if let (Some(cwd), Some(sftp)) = (cwd_to_follow, sftp_arc.as_ref()) {
                                tokio::spawn(follow_shell_cwd(
                                    app.clone(),
                                    tab_id.to_string(),
                                    cwd,
                                    Arc::clone(sftp),
                                    Arc::clone(&handle),
                                    operation_timeout,
                                    file_access_mode.clone(),
                                    root_file_access_method,
                                    sudo_user.clone(),
                                    root_password.clone(),
                                ));
                            } else if session_state_changed {
                                // 解耦：get_workspace_snapshot 会读整个 sessions
                                // RwLock + 序列化所有 tab 数据，在 shell output 分支
                                // 内同步 await 会阻塞 select! 轮询 terminal_input_rx。
                                // CWD/user 变化频率有限，spawn 到后台不阻塞主循环。
                                let snap_app = app.clone();
                                tokio::spawn(async move {
                                    if let Ok(snap) =
                                        crate::commands::get_workspace_snapshot(snap_app.clone())
                                            .await
                                    {
                                        let _ = snap_app.emit("workspace:snapshot", snap);
                                    }
                                });
                            }
                        }

                        let setup_echo_was_pending = pending_shell_setup_echo.is_some();
                        let mut visible = suppress_shell_setup_echo(&mut pending_shell_setup_echo, &text);
                        if setup_echo_was_pending && pending_shell_setup_echo.is_none() {
                            flush_deferred_terminal_input(
                                &mut deferred_terminal_input,
                                &terminal_write_tx,
                            )?;
                        }
                        // A newly-created root login shell prints its first
                        // prompt before FileTerm can inject the CWD hook. Do
                        // not forward that prompt yet: the hook intentionally
                        // causes the shell to print a replacement prompt, so
                        // forwarding both would render `root# root#` on one
                        // line. Keep the original as a fail-open fallback in
                        // case the injection cannot be completed.
                        //
                        // This path is deliberately tied to an explicit
                        // interactive sudo/su transition. A normal user's
                        // literal `#` is also echoed by the PTY and must not
                        // be mistaken for a root prompt.
                        if pending_root_access_command
                            .as_ref()
                            .is_some_and(|auth| auth.interactive_shell)
                            && looks_like_shell_prompt(&visible)
                            && !looks_like_root_prompt(&visible)
                        {
                            // The privilege command may fail without printing
                            // one of the localized authentication errors we
                            // recognize. Once the ordinary user prompt is back,
                            // discard the stale transition so a later literal
                            // `#` cannot trigger setup injection.
                            pending_root_access_command = None;
                        }
                        if last_shell_setup_injection.elapsed() > Duration::from_secs(2)
                            && pending_root_access_command
                                .as_ref()
                                .is_some_and(|auth| auth.interactive_shell)
                        {
                            let shell_is_root = state
                                .sessions
                                .read()
                                .await
                                .get(tab_id)
                                .and_then(|session| session.shell_user.as_deref())
                                == Some("root");
                            if should_reinject_root_shell_setup(
                                shell_setup_script.is_some(),
                                pending_shell_setup_echo.is_some(),
                                shell_setup_waiting_for_prompt,
                                pending_root_access_command
                                    .as_ref()
                                    .is_some_and(|auth| auth.interactive_shell),
                                shell_is_root,
                                &visible,
                            ) {
                                if let Some(setup) = shell_setup_script {
                                    let (banner, prompt_tail) =
                                        split_prompt_tail_for_setup_wait(&visible);
                                    last_shell_setup_injection = Instant::now();
                                    match write_shell_data(
                                        &shell_writer,
                                        format!(" {setup}\r").into_bytes(),
                                    )
                                    .await
                                    {
                                        Ok(()) => {
                                            visible = banner;
                                            pending_shell_setup_echo = Some(
                                                ShellSetupEchoSuppression::with_fallback(
                                                    prompt_tail,
                                                ),
                                            );
                                        }
                                        Err(error) => {
                                            // Fail open: retain the original
                                            // prompt if the hook cannot be
                                            // written to the shell channel.
                                            visible = format!("{banner}{prompt_tail}");
                                            crate::services::logging::session(
                                                app,
                                                "WARN",
                                                "ssh",
                                                tab_id,
                                                format!("root shell setup write failed: {error}"),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        // shell_setup_waiting_for_prompt 期间，shell 启动输出的 prompt
                        // 尾部不能立即 forward——否则群晖等设备会显示多个重复 prompt
                        // （shell 启动脚本可能执行命令后再次输出 prompt）。把 prompt 尾部
                        // 剥离暂存到 shell_prompt_buffer，只 forward banner 部分；setup
                        // 注入成功后由 suppress 接管，新 prompt 统一释放。
                        let (forward_text, prompt_tail) = if shell_setup_waiting_for_prompt {
                            split_prompt_tail_for_setup_wait(&visible)
                        } else {
                            (visible, String::new())
                        };
                        if !forward_text.is_empty() {
                            batch_buffer.extend_from_slice(forward_text.as_bytes());
                            // Hard ceiling: under sustained high-throughput output the
                            // 16ms flush timer can lose fairness to this branch; force
                            // a flush so memory stays bounded and the next emit does
                            // not grow a multi-MB chunk in one shot.
                            if batch_buffer.len() >= TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD {
                                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                                last_emit = Instant::now();
                            }
                        }

                        if shell_setup_waiting_for_prompt {
                            shell_prompt_buffer.push_str(&visible_shell_text(&prompt_tail));
                            if shell_prompt_buffer.len() > 4096 {
                                // char 边界安全裁剪，避免 panic 杀死 worker。
                                trim_string_front(&mut shell_prompt_buffer, 2048);
                            }
                        }

                        if shell_setup_waiting_for_prompt
                            && looks_like_shell_prompt(&shell_prompt_buffer)
                        {
                            shell_setup_waiting_for_prompt = false;
                            shell_setup_prompt_deadline = None;
                            shell_prompt_buffer.clear();
                            if let Some(setup) = shell_setup_script {
                                last_shell_setup_injection = Instant::now();
                                let setup_command = format!(" {setup}\r");
                                match write_shell_data(&shell_writer, setup_command.into_bytes()).await {
                                    Ok(()) => {
                                        // setup 注入成功，suppress 接管后续 echo 和新 prompt。
                                        pending_shell_setup_echo =
                                            Some(ShellSetupEchoSuppression::new(false));
                                    }
                                    Err(error) => {
                                        // setup 写入失败：fail-open，把暂存的 prompt 尾部
                                        // forward 出去，避免用户看不到任何 prompt。
                                        if !prompt_tail.is_empty() {
                                            batch_buffer.extend_from_slice(prompt_tail.as_bytes());
                                        }
                                        flush_deferred_terminal_input(
                                            &mut deferred_terminal_input,
                                            &terminal_write_tx,
                                        )?;
                                        shell_setup_prompt_deadline = None;
                                        crate::services::logging::session(app, "WARN", "ssh", tab_id, format!("shell setup write failed: {error}"));
                                    }
                                }
                            }
                        }

                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        // PTY implementations normally merge stderr into Data,
                        // but some SSH servers still deliver the password
                        // prompt as ExtendedData. Feed both streams through
                        // the auth detector so `su -` credentials are captured
                        // before the file exec channel is opened.
                        let text = String::from_utf8_lossy(data.as_ref());
                        if !network_device_mode {
                            if track_root_access_prompt_from_terminal(
                                &text,
                                &mut sudo_prompt_buffer,
                                &mut awaiting_root_access_auth,
                                &mut pending_sudo_password,
                                &mut root_password,
                                &mut last_authenticated_root_access,
                                &mut pending_root_access_command,
                            ) {
                                cache_root_password_for_auth(
                                    last_authenticated_root_access.as_ref(),
                                    &root_password,
                                    &mut sudo_password,
                                    &mut su_password,
                                );
                                let mut sessions = state.sessions.write().await;
                                if let Some(session) = sessions.get_mut(tab_id) {
                                    session.has_reusable_sudo_auth = false;
                                }
                            }
                            if autofill_root_access_password(
                                &shell_writer,
                                &mut awaiting_root_access_auth,
                                &mut pending_sudo_password,
                                &mut root_password,
                                &sudo_password,
                                &su_password,
                            )
                            .await?
                            {
                                crate::services::logging::ssh_debug(
                                    app,
                                    tab_id,
                                    "interactive privilege password filled from connection profile",
                                );
                            }
                        }
                        batch_buffer.extend_from_slice(data.as_ref());
                        if batch_buffer.len() >= TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD {
                            flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                            last_emit = Instant::now();
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        // Shell closed → flush and disconnect.
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
                        return Ok(());
                    }
                    _ => {}
                }
            }
            // 3. Periodic flush if there is buffered output.
            _ = tokio::time::sleep_until(next_batch_deadline) => {
                if !batch_buffer.is_empty() {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    last_emit = Instant::now();
                } else {
                    last_emit = Instant::now();
                }
            }
        }
    }
}

/// Execute one explicit remote command on an independent SSH exec channel.
/// The interactive PTY remains owned by the terminal, so an external CLI/MCP
/// call cannot steal terminal input or mix its output into the user's shell.
#[allow(clippy::too_many_arguments)]
fn spawn_remote_command(
    handle: &Arc<Handle<ClientHandler>>,
    command: String,
    cwd: Option<String>,
    timeout_ms: u64,
    stdin: Option<String>,
    request_pty: bool,
    cancellation: Option<tokio_util::sync::CancellationToken>,
    respond_to: oneshot::Sender<Result<Value, String>>,
) {
    let handle = Arc::clone(handle);
    let command = cwd
        .filter(|path| !path.trim().is_empty())
        .map(|path| format!("cd -- {} && {command}", shell_quote(path.trim())))
        .unwrap_or(command);
    let timeout_duration = Duration::from_millis(timeout_ms);
    tokio::spawn(async move {
        let exec = crate::sessions::system_metrics::exec_command_with_stdin_status_timeout_detailed(
            &handle,
            &command,
            stdin.as_deref().unwrap_or(""),
            request_pty,
            timeout_duration,
        );
        let result = match cancellation {
            Some(cancellation) if cancellation.is_cancelled() => {
                Err("AI_REQUEST_CANCELLED".to_string())
            }
            Some(cancellation) => tokio::select! {
                _ = cancellation.cancelled() => Err("AI_REQUEST_CANCELLED".to_string()),
                result = exec => result,
            },
            None => exec.await,
        };
        let result = match result {
            Ok(result) => {
                let input_kind =
                    detect_remote_exec_input_kind(&result.output).map(ToOwned::to_owned);
                let input_required = stdin.is_none() && input_kind.is_some();
                // This is only a redacted routing hint. A privileged exec
                // has already received its one-shot stdin and must not route
                // the prompt to a second input surface.
                Ok(serde_json::json!({
                    "output": result.output,
                    "exitCode": result.exit_code,
                    "timedOut": result.timed_out,
                    "outputTruncated": result.output_truncated,
                    "rawTerminal": false,
                    "inputRequired": input_required,
                    "inputKind": input_kind,
                }))
            }
            Err(error) => Err(error),
        };
        let _ = respond_to.send(result);
    });
}

fn remote_exec_input_kind(prompt: &str) -> &'static str {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("password")
        || prompt.contains("密码")
        || lower.contains("passphrase")
        || lower.contains("verification code")
        || lower.contains("one-time")
        || lower.contains("otp")
    {
        "secret"
    } else {
        "text"
    }
}

fn detect_remote_exec_input_kind(output: &str) -> Option<&'static str> {
    let visible = visible_shell_text(output).replace('\r', "\n");
    let candidate = visible
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .trim();
    let lower = candidate.to_ascii_lowercase();
    let needs_input = lower.contains("password")
        || candidate.contains("密码")
        || lower.contains("passphrase")
        || lower.contains("verification code")
        || lower.contains("one-time")
        || lower.contains("otp")
        || lower.contains("[y/n]")
        || lower.contains("[yes/no]")
        || lower.contains("(y/n)")
        || lower.contains("confirm")
        || candidate.contains("确认");
    needs_input.then(|| remote_exec_input_kind(candidate))
}

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
