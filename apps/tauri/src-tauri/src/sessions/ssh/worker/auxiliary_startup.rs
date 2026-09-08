{
    // ── Probe platform ─────────────────────────────────────────────────────
    // 加 timeout：probe 内部最多 6 组串行探针（PTY-only server 会对每组
    // 再重试一次，最多 12 个 exec channel），每次都用
    // channel.wait() 循环读取且无内层 timeout。服务器在 exec 模式下卡住
    // 时整个 probe 会永久 await，worker 永远起不来。超时后回落到
    // "unknown"，shell CWD 注入会被 fail-closed 门控跳过，终端仍可用。
    let (platform, mut metrics_request_pty, interactive_gateway) = if network_device_mode {
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "network-device mode; skipping platform probe",
        );
        ("unknown".to_string(), false, false)
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
            crate::sessions::system_metrics::probe_remote_platform_for_session_with_transport(
                &handle,
                Some(tab_id),
            ),
        )
        .await
        {
            Ok(result) => (
                result.platform,
                result.request_pty,
                result.interactive_gateway,
            ),
            Err(_) => {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "metrics",
                    tab_id,
                    format!(
                        "platform probe timed out, falling back to unknown route_hint={route_hint} username_normalized={username_normalized}"
                    ),
                );
                ("unknown".to_string(), false, false)
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
        ("unknown".to_string(), false, false)
    };

    // Go-based jump hosts commonly gate all command handlers on a PTY. Keep a
    // conservative banner heuristic for the case where the probe itself was
    // interrupted or all PTY retries were rejected; requesting a PTY for the
    // detached collector is harmless on these servers and prevents the exact
    // `No PTY requested.` exit-0 failure seen in the field.
    let remote_sshid_is_go = String::from_utf8_lossy(&remote_sshid)
        .to_ascii_lowercase()
        .starts_with("ssh-2.0-go");
    if exec_channel_enabled && remote_sshid_is_go && !metrics_request_pty {
        metrics_request_pty = true;
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "metrics transport heuristic enabled request_pty=true reason=go-server-identification",
        );
    }
    crate::services::logging::session(
        app,
        "INFO",
        "metrics",
        tab_id,
        format!(
            "platform probe completed platform={platform} transport={} metrics_request_pty={metrics_request_pty} interactive_gateway={interactive_gateway} route_hint={route_hint} username_normalized={username_normalized}",
            if metrics_request_pty {
                "exec-pty"
            } else {
                "exec"
            }
        ),
    );
    if interactive_gateway {
        crate::services::logging::session(
            app,
            "WARN",
            "ssh",
            tab_id,
            format!(
                "interactive SSH gateway detected route_hint={route_hint} route_state=foreground-menu-only username_normalized={username_normalized} direct_login_hint={}; terminal is still connected but target asset is not routed; deferring metrics and SFTP; use a JumpServer direct username or a transparent jumpProfileId route through an ordinary OpenSSH jump host",
                direct_login_hint.unwrap_or("none"),
            ),
        );
    } else if remote_sshid_is_go {
        crate::services::logging::session(
            app,
            "DEBUG",
            "ssh",
            tab_id,
            "SSH-2.0-Go banner observed without an interactive asset menu; continuing auxiliary channels and requiring a real target identity in the first metrics sample",
        );
    }

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
        metrics_request_pty,
        interactive_gateway,
        route_hint,
        cancellation: &cancellation,
        state: &state,
    };
    if interactive_gateway {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.capabilities.files = false;
            session.capabilities.file_access = false;
            session.capabilities.resource_monitoring = false;
            session.capabilities.shell_integration = false;
            session.follow_shell_cwd = false;
            session.resource_monitoring_unavailable_reason =
                Some("interactive-gateway-target-route-required".to_string());
        }
    }
    let (sftp_arc, sftp_unavailable_reason) = initialize_sftp_session(&startup).await;

    // Library/transfer hydration must not hold back the ready SFTP handle
    // or shell integration. Snapshot revisions preserve publication ordering.
    schedule_workspace_snapshot_emit(app);
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

    spawn_metrics_collector(&startup, metrics_shutdown.clone()).await;

    SshAuxiliaryReady {
        sftp: sftp_arc,
        sftp_unavailable_reason,
        shell_setup_script,
    }
}
