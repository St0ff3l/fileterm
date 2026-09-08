{
    let bytes = data.as_ref();
    let text = stdout_decoder.decode(bytes);
    if auxiliary_pending && !terminal_input_started {
        startup_prompt.push_str(&visible_shell_text(&text));
        if startup_prompt.len() > 4096 {
            trim_string_front(&mut startup_prompt, 2048);
        }
    }
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
    let prompt_cwd = take_prompt_cwd(
        new_cwd.clone(),
        new_user.is_some(),
        &mut pending_cwd_marker_without_user,
    );
    let mut cwd_to_follow = None;
    let mut file_mode_switch: Option<(String, Option<String>, RootFileAccessMethod)> = None;
    let mut session_state_changed = false;
    let mut ai_target_changed = false;
    if new_cwd.is_some() || new_user.is_some() {
        let mut sessions = state.sessions.write().await;
        if let Some(s) = sessions.get_mut(tab_id) {
            if s.follow_shell_cwd {
                cwd_to_follow = prompt_cwd;
            }
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
                }
            }
            if let Some(user) = &new_user {
                let shell_user_changed =
                    observe_shell_user(&mut s.login_user, &mut s.shell_user, user);
                if shell_user_changed {
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
                    // Only a shell identity transition may auto-switch the
                    // visible file mode. Repeated RemoteUser markers from the
                    // same root shell must not undo a manual user/root choice.
                    let mode_changed = shell_user_changed && s.file_access_mode != target_mode;
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
                        let access_changed = root_file_access_method != access_method
                            || sudo_user.as_deref() != Some(observed_sudo_user.as_str());
                        s.sudo_user = Some(observed_sudo_user.clone());
                        s.has_reusable_sudo_auth = access_method == RootFileAccessMethod::Sudo
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
                            file_mode_switch =
                                Some((next_mode, Some(observed_sudo_user), access_method));
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
            root_password = root_password_for_method(access_method, &sudo_password, &su_password);
        }
        if let (Some(cwd), Some(sftp)) = (cwd_to_follow, sftp_arc.as_ref()) {
            cwd_refresh.send_replace(Some(CwdRefreshRequest {
                cwd,
                sftp: Arc::clone(sftp),
                file_access_mode: file_access_mode.clone(),
                root_file_access_method,
                sudo_user: sudo_user.clone(),
                sudo_password: root_password.clone(),
            }));
        } else if session_state_changed {
            // 解耦：get_workspace_snapshot 会读整个 sessions
            // RwLock + 序列化所有 tab 数据，在 shell output 分支
            // 内同步 await 会阻塞 select! 轮询 terminal_input_rx。
            // CWD/user 变化频率有限，spawn 到后台不阻塞主循环。
            let snap_app = app.clone();
            tokio::spawn(async move {
                if let Ok(snap) = crate::commands::get_workspace_snapshot(snap_app.clone()).await {
                    let _ = snap_app.emit("workspace:snapshot", snap);
                }
            });
        }
    }

    let setup_echo_was_pending = pending_shell_setup_echo.is_some();
    let mut visible = suppress_shell_setup_echo(&mut pending_shell_setup_echo, &text);
    if setup_echo_was_pending && pending_shell_setup_echo.is_none() {
        flush_deferred_terminal_input(&mut deferred_terminal_input, &terminal_write_tx)?;
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
                let (banner, prompt_tail) = split_prompt_tail_for_setup_wait(&visible);
                last_shell_setup_injection = Instant::now();
                match write_shell_data(&shell_writer, format!(" {setup}\r").into_bytes()).await {
                    Ok(()) => {
                        visible = banner;
                        pending_shell_setup_echo =
                            Some(ShellSetupEchoSuppression::with_fallback(prompt_tail));
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

    if shell_setup_waiting_for_prompt && looks_like_shell_prompt(&shell_prompt_buffer) {
        shell_setup_waiting_for_prompt = false;
        shell_setup_prompt_deadline = None;
        shell_prompt_buffer.clear();
        if let Some(setup) = shell_setup_script {
            last_shell_setup_injection = Instant::now();
            let setup_command = format!(" {setup}\r");
            match write_shell_data(&shell_writer, setup_command.into_bytes()).await {
                Ok(()) => {
                    // setup 注入成功，suppress 接管后续 echo 和新 prompt。
                    pending_shell_setup_echo = Some(ShellSetupEchoSuppression::new(false));
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
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        format!("shell setup write failed: {error}"),
                    );
                }
            }
        }
    }
}
