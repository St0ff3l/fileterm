/// Run the fair terminal/input/command select loop for one SSH session.
///
/// The context owns connection resources; this function owns only the mutable
/// event-loop state and exits cleanly on cancellation or channel closure.
async fn run_worker_event_loop(
    context: SshSessionContext,
    mut shell_reader: russh::ChannelReadHalf,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
) -> Result<(), String> {
    let app = &context.app;
    let tab_id = context.tab_id.as_str();
    let profile = &context.profile;
    let handle = Arc::clone(&context.handle);
    let shell_writer = Arc::clone(&context.shell_writer);
    let sftp_arc = context.sftp.clone();
    let transfer_sftp_slot = Arc::clone(&context.transfer_sftp_slot);
    let operation_timeout = context.operation_timeout;
    let network_device_mode = context.network_device_mode;
    let exec_channel_enabled = context.exec_channel_enabled;
    let sftp_unavailable_reason = context.sftp_unavailable_reason.clone();
    let cancellation = context.cancellation.clone();
    let metrics_shutdown = Arc::clone(&context.metrics_shutdown);
    let shell_setup_script = context.shell_setup_script;
    let terminal_write_tx = context.terminal_write_tx.clone();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut pending_shell_setup_echo: Option<ShellSetupEchoSuppression> = None;
    let mut shell_setup_waiting_for_prompt = shell_setup_script.is_some();
    let mut shell_prompt_buffer = String::new();
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

let tunnel_command_tx =
    start_tunnel_command_runtime(profile, tab_id, app, &handle).await;

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
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    metrics_shutdown.notify_waiters();
                    return Ok(());
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
