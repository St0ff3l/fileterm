async fn prepare_metrics_channel(runtime: &mut MetricsRuntime) -> Result<PreparedMetrics, ()> {
    let cancellation = runtime.attempt_cancellation.clone();
    let metrics_app = runtime.app.clone();
    let metrics_tid = runtime.tab_id.clone();
    let metrics_plat = runtime.platform.clone();
    let metrics_handle = runtime.handle.clone();
    let metrics_interval_seconds = runtime.policy.interval;
    let metrics_request_pty = runtime.request_pty;
    // Build the infinite-loop script. Each iteration emits a
    // delimited metrics block and sleeps for the configured interval. We use a
    // unique marker so the stream parser can reliably slice blocks.
    let marker = "__FILETERM_METRICS_BLOCK__";
    let (windows_command, script_body) = if metrics_plat == "windows" {
        let command =
            match crate::sessions::system_metrics::build_windows_streaming_metrics_exec_command(
                metrics_interval_seconds,
            ) {
                Ok(command) => command,
                Err(error) => {
                    runtime
                        .failure(format!("Windows streaming command build failed: {error}"))
                        .await;
                    return Err(());
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
        let collector_preamble = if metrics_request_pty {
            "stty -echo 2>/dev/null || true\ncd / >/dev/null 2>&1 || true"
        } else {
            "cd / >/dev/null 2>&1 || true"
        };
        let script = format!(
            "{}\nwhile true; do\n{}\necho '{}'\nsleep {}\ndone\n",
            collector_preamble, metrics, marker, metrics_interval_seconds
        );
        (None, Some(script))
    };

    // Open one persistent shell channel for the entire session.
    // 加 timeout：服务器 MaxSessions 满或网络抖动时这一步会卡住，
    // 不加超时 metrics task 会永久 await，虽然不阻塞主循环，但
    // 用户看不到系统监控数据且 worker 不会自动重试。
    let mut channel =
        match open_metrics_channel(&cancellation, runtime.connection_cancellation.clone(), runtime.channel_open_gate.clone(), metrics_handle, SHELL_INIT_STEP_TIMEOUT).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                runtime.failure(format!("open channel failed: {e}")).await;
                return Err(());
            }
            Err(_) => {
                runtime.failure("open channel timed out").await;
                return Err(());
            }
        };

    // Carry the compatibility decision from platform probing into the
    // persistent collector. A Go-based jump host can accept the visible
    // terminal PTY while returning `No PTY requested.` for every plain
    // exec channel; requesting the PTY here keeps metrics on the same
    // transport instead of disabling the sidebar after exit status 0.
    let collector_request_pty = if metrics_request_pty {
        crate::services::logging::session(
            &metrics_app,
            "DEBUG",
            "metrics",
            &metrics_tid,
            "collector requesting PTY transport=exec-pty reason=platform-probe-or-server-banner",
        );
        match metrics_startup_step(
            &cancellation,
            channel.request_pty(
                true,
                "xterm-256color",
                80,
                24,
                0,
                0,
                &[
                    (russh::Pty::ECHO, 0),
                    (russh::Pty::ECHOE, 0),
                    (russh::Pty::ECHOK, 0),
                    (russh::Pty::ECHONL, 0),
                    (russh::Pty::TTY_OP_ISPEED, 115200),
                    (russh::Pty::TTY_OP_OSPEED, 115200),
                ],
            ),
        )
        .await
        {
            Ok(Ok(())) => {
                crate::services::logging::session(
                    &metrics_app,
                    "INFO",
                    "metrics",
                    &metrics_tid,
                    "collector PTY request accepted transport=exec-pty",
                );
                true
            }
            Ok(Err(error)) => {
                crate::services::logging::session(
                    &metrics_app,
                    "WARN",
                    "metrics",
                    &metrics_tid,
                    format!("collector PTY request rejected error={error}"),
                );
                false
            }
            Err(_) => {
                crate::services::logging::session(
                    &metrics_app,
                    "WARN",
                    "metrics",
                    &metrics_tid,
                    format!(
                        "collector PTY request timed out timeout_secs={}",
                        SHELL_INIT_STEP_TIMEOUT.as_secs()
                    ),
                );
                false
            }
        }
    } else {
        false
    };

    // A PTY-required gateway cannot use the regular non-PTY collector
    // path: KoKo-style proxies inspect the raw exec request before
    // forwarding it to the asset. If the requested PTY was rejected, fail
    // closed instead of falling back to a script write that would leave
    // the channel hanging with no target sample.
    if metrics_request_pty && !collector_request_pty {
        crate::services::logging::session(
                &metrics_app,
                "WARN",
                "metrics",
                &metrics_tid,
                "collector PTY required but unavailable; disabling POSIX collector reason=pty-request-rejected",
            );
        close_metrics_channel(
            &mut channel,
            &metrics_app,
            &metrics_tid,
            "pty-request-rejected",
        )
        .await;
        runtime
            .failure("collector PTY request rejected for PTY-only gateway")
            .await;
        return Err(());
    }

    // Use an inline command for the normal PTY-exec path. Some KoKo/asset
    // combinations return an asset shell prompt instead of executing it;
    // the stream reader below then retries the same script through PTY
    // stdin, after the prompt proves that the target shell is ready.
    let inline_login_shell_command = if collector_request_pty && windows_command.is_none() {
        script_body
            .as_deref()
            .map(crate::sessions::system_metrics::build_pty_login_shell_command)
    } else {
        None
    };
    let collector_command_mode = if windows_command.is_some() {
        if collector_request_pty {
            "exec-pty"
        } else {
            "exec"
        }
    } else if inline_login_shell_command.is_some() {
        "exec-pty-login-shell-inline-b64"
    } else {
        "exec-sh-stdin"
    };
    crate::services::logging::session(
            &metrics_app,
            "DEBUG",
            "metrics",
            &metrics_tid,
            format!(
                "collector command prepared flow_role=target command_mode={collector_command_mode} script_bytes={} command_bytes={} stdin_bytes={}",
                script_body.as_deref().map(str::len).unwrap_or(0),
                inline_login_shell_command
                    .as_deref()
                    .or(windows_command.as_deref())
                    .map(str::len)
                    .unwrap_or_else(|| "sh -s".len()),
                if inline_login_shell_command.is_some() {
                    0
                } else {
                    script_body.as_deref().map(str::len).unwrap_or(0)
                },
            ),
        );

    // Windows OpenSSH can stall with large scripts on stdin.
    // Gzip + base64 keeps
    // the loader below cmd.exe's safe command-line budget, while the
    // decoded script runs as one persistent PowerShell process.
    crate::services::logging::session(
            &metrics_app,
            "DEBUG",
            "metrics",
            &metrics_tid,
            format!(
                "collector exec request sending flow_role=target command_mode={collector_command_mode} request_pty={collector_request_pty}"
            ),
        );
    let collector_start = if let Some(command) = windows_command.as_deref() {
        metrics_startup_step(&cancellation, channel.exec(true, command)).await
    } else if let Some(command) = inline_login_shell_command.as_deref() {
        metrics_startup_step(&cancellation, channel.exec(true, command)).await
    } else {
        // Normal OpenSSH servers keep the existing non-interactive
        // `sh -s` + stdin path. It avoids a very long command line when a
        // PTY-only gateway is not involved.
        metrics_startup_step(&cancellation, channel.exec(true, "sh -s")).await
    };
    let collector_start = match collector_start {
        Ok(inner) => inner,
        Err(_) => {
            close_metrics_channel(
                &mut channel,
                &metrics_app,
                &metrics_tid,
                "collector-start-timeout",
            )
            .await;
            runtime.failure("start collector timed out").await;
            return Err(());
        }
    };
    if let Err(e) = collector_start {
        close_metrics_channel(
            &mut channel,
            &metrics_app,
            &metrics_tid,
            "collector-start-error",
        )
        .await;
        runtime
            .failure(format!("start collector failed: {e}"))
            .await;
        return Err(());
    }

    if inline_login_shell_command.is_none() {
        if let Some(script) = script_body.as_deref() {
            // Keep a timeout on the non-PTY stdin path as well: a
            // constrained OpenSSH server can stop accepting channel data
            // without closing the channel, which otherwise leaves the
            // metrics task awaiting forever.
            match metrics_startup_step(&cancellation, channel.data(script.as_bytes())).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    close_metrics_channel(
                        &mut channel,
                        &metrics_app,
                        &metrics_tid,
                        "collector-script-write-error",
                    )
                    .await;
                    runtime
                        .failure(format!("write collector script failed: {e}"))
                        .await;
                    return Err(());
                }
                Err(_) => {
                    close_metrics_channel(
                        &mut channel,
                        &metrics_app,
                        &metrics_tid,
                        "collector-script-write-timeout",
                    )
                    .await;
                    runtime.failure("write collector script timed out").await;
                    return Err(());
                }
            }
        }
    }

    crate::services::logging::session(
            &metrics_app,
            "INFO",
            "metrics",
            &metrics_tid,
            format!(
                "collector started; waiting for first sample flow_role=target command_mode={collector_command_mode} request_pty={} script_bytes={} command_bytes={} idle_timeout_secs={}",
                collector_request_pty,
                script_body.as_deref().map(str::len).unwrap_or(0),
                inline_login_shell_command
                    .as_deref()
                    .or(windows_command.as_deref())
                    .map(str::len)
                    .unwrap_or_else(|| "sh -s".len()),
                runtime.policy.first_sample.as_secs()
            ),
        );

    Ok(PreparedMetrics {
        channel,
        script_body,
        collector_request_pty,
        collector_command_mode,
        marker,
    })
}

struct PreparedMetrics {
    channel: Channel<russh::client::Msg>,
    script_body: Option<String>,
    collector_request_pty: bool,
    collector_command_mode: &'static str,
    marker: &'static str,
}

/// Cancellation interrupts individual setup operations while allowing the
/// caller to close an already-open channel before returning.
async fn metrics_startup_step<T>(
    cancellation: &CancellationToken,
    operation: impl std::future::Future<Output = T>,
) -> Result<T, ()> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err(()),
        result = timeout(SHELL_INIT_STEP_TIMEOUT, operation) => result.map_err(|_| ()),
    }
}
