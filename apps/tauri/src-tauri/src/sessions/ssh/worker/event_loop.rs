/// Run the fair terminal/input/command select loop for one SSH session.
///
/// The context owns connection resources; this function owns only the mutable
/// event-loop state and exits cleanly on cancellation or channel closure.
async fn run_worker_event_loop(
    context: SshSessionContext,
    mut shell_reader: russh::ChannelReadHalf,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
    mut auxiliary_rx: oneshot::Receiver<SshAuxiliaryReady>,
) -> Result<SshWorkerExit, String> {
    let app = &context.app;
    let tab_id = context.tab_id.as_str();
    let profile = &context.profile;
    let handle = Arc::clone(&context.handle);
    let shell_writer = Arc::clone(&context.shell_writer);
    let mut sftp_arc = context.sftp.clone();
    let transfer_sftp_slot = Arc::clone(&context.transfer_sftp_slot);
    let operation_timeout = context.operation_timeout;
    let network_device_mode = context.network_device_mode;
    let exec_channel_enabled = context.exec_channel_enabled;
    let mut sftp_unavailable_reason = context.sftp_unavailable_reason.clone();
    let cancellation = context.cancellation.clone();
    let disconnect_reason = Arc::clone(&context.disconnect_reason);
    let connected_at = context.connected_at;
    let metrics_shutdown = Arc::clone(&context.metrics_shutdown);
    let mut shell_setup_script = context.shell_setup_script;
    let mut auxiliary_pending = true;
    let mut startup_prompt = String::new();
    let mut terminal_input_started = false;
    let terminal_write_tx = context.terminal_write_tx.clone();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut pending_shell_setup_echo: Option<ShellSetupEchoSuppression> = None;
    let mut shell_setup_waiting_for_prompt = shell_setup_script.is_some();
    let mut shell_prompt_buffer = String::new();
    // ── Main event loop: terminal reads + command dispatch ─────────────────
    let mut cwd_buffer = String::new();
    let cwd_refresh = start_cwd_refresh(
        app.clone(),
        tab_id.to_string(),
        Arc::clone(&handle),
        operation_timeout,
        cancellation.clone(),
    );
    // `__tdcwd` prints OSC7 (CWD) immediately before OSC1337 (user). SSH can
    // split those two markers into separate packets; defer a CWD-only event
    // until its matching user marker arrives so a root transition cannot
    // briefly browse through the stale sudo method.
    let mut pending_cwd_marker_without_user: Option<String> = None;
    let mut batch_buffer: Vec<u8> = Vec::new();
    let mut stdout_decoder = SshUtf8Decoder::default();
    let mut stderr_decoder = SshUtf8Decoder::default();
    let mut last_emit = Instant::now();
    // User input must not enter the PTY while the first prompt is being
    // identified or while the internal setup command is executing. Otherwise
    // the shell echoes a literal `#` into the same stream that the prompt
    // heuristic inspects and the setup command races with that input.
    let mut deferred_terminal_input: Vec<Vec<u8>> = Vec::new();

    let terminal_output_tx = spawn_terminal_output_pump(app, tab_id);

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

    let tunnel_command_tx = start_tunnel_command_runtime(profile, tab_id, app, &handle).await;

    // A deferred auxiliary result starts the prompt deadline only when
    // the initial shell setup can safely be injected.
    let mut shell_setup_prompt_deadline =
        shell_setup_script.map(|_| Instant::now() + SHELL_SETUP_PROMPT_TIMEOUT);

    loop {
        // 16ms batch window for terminal output.
        let next_batch_deadline =
            tokio::time::Instant::from_std(last_emit + Duration::from_millis(16));

        tokio::select! {
            ready = &mut auxiliary_rx, if auxiliary_pending => {
                auxiliary_pending = false;
                if let Ok(ready) = ready {
                    sftp_arc = ready.sftp;
                    sftp_unavailable_reason = ready.sftp_unavailable_reason;
                    shell_setup_script = ready.shell_setup_script;
                    // The shell can emit a CWD marker while the auxiliary
                    // channels are still negotiating. `shell_data.rs` records
                    // that marker, but cannot enqueue a follow request until
                    // the SFTP handle exists. Replay the latest shell state at
                    // this boundary so the SFTP pane does not remain on the
                    // pre-startup directory until the next prompt.
                    if let Some(sftp) = sftp_arc.as_ref() {
                        let startup_cwd = state
                            .sessions
                            .read()
                            .await
                            .get(tab_id)
                            .filter(|session| session.follow_shell_cwd)
                            .and_then(|session| session.shell_cwd.clone());
                        if let Some(cwd) = startup_cwd {
                            cwd_refresh.send_replace(Some(CwdRefreshRequest {
                                cwd,
                                sftp: Arc::clone(sftp),
                                file_access_mode: file_access_mode.clone(),
                                root_file_access_method,
                                sudo_user: sudo_user.clone(),
                                sudo_password: root_password.clone(),
                            }));
                        }
                    }
                    // Never inject a setup command into a command the user has
                    // already started while auxiliary initialization was pending.
                    if !terminal_input_started {
                        if let Some(setup) = shell_setup_script {
                            if looks_like_shell_prompt(&startup_prompt) {
                                if write_shell_data(&shell_writer, format!(" {setup}\r").into_bytes()).await.is_ok() {
                                    pending_shell_setup_echo = Some(ShellSetupEchoSuppression::new(false));
                                    last_shell_setup_injection = Instant::now();
                                }
                            } else {
                                shell_setup_waiting_for_prompt = true;
                                shell_setup_prompt_deadline = Some(Instant::now() + SHELL_SETUP_PROMPT_TIMEOUT);
                            }
                        }
                    }
                    startup_prompt.clear();
                }
            }
            _ = cancellation.cancelled() => {
                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                metrics_shutdown.notify_waiters();
                return Ok(SshWorkerExit::cancelled());
            }
            input = terminal_input_rx.recv() => {
                let Some(data) = input else {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    metrics_shutdown.notify_waiters();
                    return Ok(SshWorkerExit::input_closed());
                };
                let data = coalesce_terminal_input(data, terminal_input_rx);
                terminal_input_started |= !data.is_empty();
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
                if cmd.is_none() {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    metrics_shutdown.notify_waiters();
                    return Ok(SshWorkerExit::input_closed());
                }
                if let Some(WorkerCmd::WriteTerminal(data)) = &cmd {
                    terminal_input_started |= !data.is_empty();
                }
                match handle_worker_command_event(
                    cmd,
                    network_device_mode,
                    &handle,
                    &shell_writer,
                    sftp_arc.as_ref(),
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
                    sftp_unavailable_reason
                        .as_deref()
                        .unwrap_or(SFTP_UNAVAILABLE_FALLBACK),
                    exec_channel_enabled,
                    &mut awaiting_root_access_auth,
                    &mut pending_sudo_password,
                    &mut recent_terminal_input,
                    &mut last_authenticated_root_access,
                    &mut pending_root_access_command,
                )
                .await
                {
                    Ok(true) => {
                        batch_buffer.extend_from_slice(stdout_decoder.finish().as_bytes());
                        batch_buffer.extend_from_slice(stderr_decoder.finish().as_bytes());
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
                        return Ok(SshWorkerExit::explicit_disconnect());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        crate::services::logging::session(
                            app,
                            "WARN",
                            "ssh",
                            tab_id,
                            format!("command failed: {e}"),
                        );
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
                        include!("shell_data.rs");
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        // PTY implementations normally merge stderr into Data,
                        // but some SSH servers still deliver the password
                        // prompt as ExtendedData. Feed both streams through
                        // the auth detector so `su -` credentials are captured
                        // before the file exec channel is opened.
                        let text = stderr_decoder.decode(data.as_ref());
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
                        batch_buffer.extend_from_slice(text.as_bytes());
                        if batch_buffer.len() >= TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD {
                            flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                            last_emit = Instant::now();
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        // A shell channel can close independently of the SSH
                        // transport. Only reconnect when russh tells us that the
                        // connection itself was lost; otherwise a command such
                        // as `exit` would create an endless reconnect loop.
                        batch_buffer.extend_from_slice(stdout_decoder.finish().as_bytes());
                        batch_buffer.extend_from_slice(stderr_decoder.finish().as_bytes());
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
                        let disconnect = disconnect_reason
                            .lock()
                            .ok()
                            .and_then(|reason| reason.clone())
                            .or_else(|| {
                                handle.is_closed().then(|| SshDisconnectInfo {
                                    kind: SshDisconnectKind::Transport,
                                    message: "transport closed without a disconnect callback"
                                        .to_string(),
                                })
                            });
                        let stable = connected_at.elapsed() >= SSH_CONNECTION_STABILITY_WINDOW;
                        let exit = match disconnect {
                            Some(reason) => SshWorkerExit::transport_closed(reason, stable),
                            None => SshWorkerExit::shell_closed(stable),
                        };
                        crate::services::logging::session(
                            app,
                            if exit.should_reconnect() { "WARN" } else { "INFO" },
                            "ssh",
                            tab_id,
                            exit.description(),
                        );
                        return Ok(exit);
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
