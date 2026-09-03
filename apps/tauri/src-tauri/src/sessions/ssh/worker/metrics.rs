/// Start the persistent system-metrics collector for an SSH session.
///
/// Metrics are intentionally detached from the terminal event loop: a slow
/// collector or a restricted exec channel must never delay terminal input.
async fn spawn_metrics_collector(
    startup: &SshWorkerStartupContext<'_>,
    metrics_shutdown: Arc<tokio::sync::Notify>,
) {
let app = startup.app;
let tab_id = startup.tab_id;
let profile = startup.profile;
let handle = startup.handle;
let target_host = startup.host.to_string();
let target_port = startup.port;
let platform = startup.platform;
let cancellation = startup.cancellation;
let state = startup.state;
let metrics_request_pty = startup.metrics_request_pty;
// ── Spawn metrics collection task (single persistent channel) ─────────
// Instead of opening a new exec channel every second (which adds variable
// SSH overhead and makes the refresh cadence jittery), we open one
// long-lived shell channel and pipe an infinite-loop script into it.
// The remote side controls the 1s cadence via `sleep 1`, so data arrives
// at a rock-steady interval regardless of SSH RTT.
if effective_resource_monitoring_enabled(profile) {
    let metrics_shutdown_clone = metrics_shutdown.clone();
    let metrics_handle = Arc::clone(handle);
    let metrics_app = app.clone();
    let metrics_tid = tab_id.to_string();
    let metrics_plat = platform.to_string();
    let metrics_interval_seconds = resource_monitoring_interval_seconds(profile);
    // A healthy collector emits at least one marker per configured interval.
    // Give the server three intervals (and never less than 15s) before
    // declaring the channel stalled, so a silent jump-host path cannot leave
    // the sidebar waiting forever with no diagnostic.
    let metrics_idle_timeout = Duration::from_secs(
        metrics_interval_seconds
            .saturating_mul(3)
            .max(15),
    );
    let metrics_cancellation = cancellation.clone();
    tokio::spawn(async move {
        crate::services::logging::session(
            &metrics_app,
            "INFO",
            "metrics",
            &metrics_tid,
            format!(
                "collector starting flow_role=target target={target_host}:{target_port} platform={} collector_variant={} interval_seconds={metrics_interval_seconds} request_pty={metrics_request_pty}",
                metrics_plat,
                if metrics_plat == "windows" {
                    "windows"
                } else if metrics_plat == "freebsd" {
                    "freebsd"
                } else if metrics_plat == "busybox" {
                    "busybox"
                } else {
                    "posix-linux"
                },
            ),
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
        let collector_started_at = Instant::now();
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
            match timeout(
                SHELL_INIT_STEP_TIMEOUT,
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
            disable_resource_monitoring_capability(
                &metrics_app,
                &metrics_tid,
                "collector PTY request rejected for PTY-only gateway",
            )
            .await;
            return;
        }

        // PTY mode sends no script bytes through stdin. Encoding the complete
        // collector in the command keeps the gateway's login-shell matcher
        // satisfied and avoids TTY line-buffer limits and input echo.
        let inline_login_shell_command = if collector_request_pty && windows_command.is_none() {
            script_body.as_deref().map(
                crate::sessions::system_metrics::build_pty_login_shell_command,
            )
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

        // Windows OpenSSH on this host stalls when a large script is sent
        // through stdin. Match Electron's transport: gzip + base64 keeps
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
            timeout(SHELL_INIT_STEP_TIMEOUT, channel.exec(true, command)).await
        } else if let Some(command) = inline_login_shell_command.as_deref() {
            timeout(SHELL_INIT_STEP_TIMEOUT, channel.exec(true, command)).await
        } else {
            // Normal OpenSSH servers keep the existing non-interactive
            // `sh -s` + stdin path. It avoids a very long command line when a
            // PTY-only gateway is not involved.
            timeout(SHELL_INIT_STEP_TIMEOUT, channel.exec(true, "sh -s")).await
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
            close_metrics_channel(
                &mut channel,
                &metrics_app,
                &metrics_tid,
                "collector-start-error",
            )
            .await;
            disable_resource_monitoring_capability(
                &metrics_app,
                &metrics_tid,
                format!("start collector failed: {e}"),
            )
            .await;
            return;
        }

        if inline_login_shell_command.is_none() {
            if let Some(script) = script_body.as_deref() {
                // Keep a timeout on the non-PTY stdin path as well: a
                // constrained OpenSSH server can stop accepting channel data
                // without closing the channel, which otherwise leaves the
                // metrics task awaiting forever.
                match timeout(SHELL_INIT_STEP_TIMEOUT, channel.data(script.as_bytes())).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        close_metrics_channel(
                            &mut channel,
                            &metrics_app,
                            &metrics_tid,
                            "collector-script-write-error",
                        )
                        .await;
                        disable_resource_monitoring_capability(
                            &metrics_app,
                            &metrics_tid,
                            format!("write collector script failed: {e}"),
                        )
                        .await;
                        return;
                    }
                    Err(_) => {
                        close_metrics_channel(
                            &mut channel,
                            &metrics_app,
                            &metrics_tid,
                            "collector-script-write-timeout",
                        )
                        .await;
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
                metrics_idle_timeout.as_secs()
            ),
        );

        // Stream reader: accumulate data, split on the marker, parse
        // each complete block and emit it to the renderer.
        let mut buffer: Vec<u8> = Vec::new();
        let marker_bytes = marker.as_bytes();
        let mut sample_count = 0_u64;
        let mut malformed_block_count = 0_u64;
        let mut oversized_block_count = 0_u64;
        let mut dropped_buffer_bytes = 0_u64;
        let mut stdout_bytes = 0_u64;
        let mut stderr_bytes = 0_u64;
        let mut stdout_tail = Vec::with_capacity(METRICS_STDOUT_TAIL_BYTES);
        let mut stderr_tail = Vec::with_capacity(METRICS_STDERR_TAIL_BYTES);
        let mut collector_exit_code = None;
        let mut remote_terminal_event = "none";
        let close_reason = 'collector: loop {
            tokio::select! {
                biased;
                _ = metrics_shutdown_clone.notified() => {
                    break "shutdown-notify";
                }
                _ = metrics_cancellation.cancelled() => {
                    break "session-cancelled";
                }
                msg = timeout(metrics_idle_timeout, channel.wait()) => {
                    match msg {
                        Err(_) => {
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "collector idle timeout timeout_secs={} samples={} buffer_bytes={}",
                                    metrics_idle_timeout.as_secs(),
                                    sample_count,
                                    buffer.len(),
                                ),
                            );
                            disable_resource_monitoring_capability(
                                &metrics_app,
                                &metrics_tid,
                                format!(
                                    "collector idle timeout after {} seconds",
                                    metrics_idle_timeout.as_secs()
                                ),
                            )
                            .await;
                            break "idle-timeout";
                        }
                        Ok(Some(ChannelMsg::Data { data })) => {
                            stdout_bytes = stdout_bytes.saturating_add(data.len() as u64);
                            append_metrics_stderr_tail(&mut stdout_tail, data.as_ref());
                            buffer.extend_from_slice(data.as_ref());
                            // Drain all complete blocks from the buffer.
                            while let Some(idx) = find_subsequence(&buffer, marker_bytes) {
                                // A malformed or unexpectedly large process list must not
                                // monopolize the Tokio worker and freeze the native webview.
                                // Keep one bounded metrics sample; the next marker resumes
                                // normal streaming collection.
                                if idx > METRICS_MAX_BLOCK_BYTES {
                                    oversized_block_count += 1;
                                    dropped_buffer_bytes = dropped_buffer_bytes.saturating_add(
                                        (idx + marker_bytes.len()) as u64,
                                    );
                                    if should_log_metrics_anomaly(oversized_block_count) {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "WARN",
                                            "metrics",
                                            &metrics_tid,
                                            format!(
                                                "oversized sample dropped count={} block_bytes={} total_dropped_bytes={}",
                                                oversized_block_count,
                                                idx,
                                                dropped_buffer_bytes,
                                            ),
                                        );
                                    }
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
                                    malformed_block_count += 1;
                                    if should_log_metrics_anomaly(malformed_block_count) {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "WARN",
                                            "metrics",
                                            &metrics_tid,
                                            format!(
                                                "malformed sample dropped count={} block_bytes={} buffer_bytes={}",
                                                malformed_block_count,
                                                block.len(),
                                                buffer.len(),
                                            ),
                                        );
                                    }
                                    continue;
                                }

                                if sample_count == 0 {
                                    let remote_ip = metrics_identity_field(&val, "ip");
                                    let remote_hostname = metrics_identity_field(&val, "hostname");
                                    let remote_os = metrics_identity_field(&val, "osName");
                                    let remote_kernel = metrics_identity_field(&val, "kernelName");
                                    if !target_identity_is_valid(&val, &metrics_plat) {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "WARN",
                                            "metrics",
                                            &metrics_tid,
                                            format!(
                                                "target identity rejected before first snapshot flow_role=target target={target_host}:{target_port} platform={} remote_ip={remote_ip:?} remote_hostname={remote_hostname:?} remote_os={remote_os:?} remote_kernel={remote_kernel:?} reason=missing-or-incompatible-identity",
                                                metrics_plat,
                                            ),
                                        );
                                        disable_resource_monitoring_capability(
                                            &metrics_app,
                                            &metrics_tid,
                                            "first metrics snapshot did not identify a compatible target",
                                        )
                                        .await;
                                        remote_terminal_event = "target-identity-invalid";
                                        break 'collector "target-identity-invalid";
                                    }
                                    crate::services::logging::session(
                                        &metrics_app,
                                        "INFO",
                                        "metrics",
                                        &metrics_tid,
                                        format!(
                                            "target identity validated before first snapshot flow_role=target target={target_host}:{target_port} platform={} remote_ip={remote_ip:?} remote_hostname={remote_hostname:?} remote_os={remote_os:?} remote_kernel={remote_kernel:?}",
                                            metrics_plat,
                                        ),
                                    );
                                }
                                sample_count += 1;
                                if sample_count == 1 {
                                    crate::services::logging::session(
                                        &metrics_app,
                                        "INFO",
                                        "metrics",
                                        &metrics_tid,
                                        format!(
                                            "first sample target={target_host}:{target_port} cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1} remote_ip={} remote_hostname={:?} remote_os={:?} remote_kernel={:?}",
                                            metrics_identity_field(&val, "ip"),
                                            metrics_identity_field(&val, "hostname"),
                                            metrics_identity_field(&val, "osName"),
                                            metrics_identity_field(&val, "kernelName"),
                                        ),
                                    );
                                } else if sample_count.is_multiple_of(60) {
                                    crate::services::logging::session(
                                        &metrics_app,
                                        "DEBUG",
                                        "metrics",
                                        &metrics_tid,
                                        format!(
                                            "sample heartbeat count={} cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1} buffer_bytes={}",
                                            sample_count,
                                            buffer.len(),
                                        ),
                                    );
                                }
                                let state_updated = {
                                    let state = metrics_app
                                        .state::<crate::services::workspace::WorkspaceState>();
                                    let mut sessions = state.sessions.write().await;
                                    if let Some(s) = sessions.get_mut(&metrics_tid) {
                                        s.system_metrics = Some(merge_system_metrics_history(
                                            s.system_metrics.as_ref(),
                                            val.clone(),
                                            600,
                                        ));
                                        true
                                    } else {
                                        false
                                    }
                                };
                                if !state_updated {
                                    crate::services::logging::session(
                                        &metrics_app,
                                        "WARN",
                                        "metrics",
                                        &metrics_tid,
                                            format!(
                                                "metrics state update skipped sample={} reason=session-not-found",
                                                sample_count,
                                            ),
                                    );
                                }
                                let payload = serde_json::json!({
                                    "tabId": metrics_tid,
                                    "systemMetrics": val,
                                    "mode": "append",
                                });
                                match metrics_app.emit("workspace:sessionMetrics", payload) {
                                    Ok(()) if sample_count == 1 => {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "INFO",
                                            "metrics",
                                            &metrics_tid,
                                            format!(
                                                "first metrics snapshot event emitted to renderer flow_role=target mode=append resource_monitoring=true state_update={}",
                                                if state_updated { "applied" } else { "skipped" },
                                            ),
                                        );
                                    }
                                    Ok(()) => {}
                                    Err(error) => {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "WARN",
                                            "metrics",
                                            &metrics_tid,
                                            format!(
                                                "metrics event emission failed samples={} error={error}",
                                                sample_count,
                                            ),
                                        );
                                    }
                                }
                            }
                            // Cap buffer to prevent unbounded growth
                            if buffer.len() > METRICS_MAX_BUFFER_BYTES {
                                let dropped = buffer.len() - METRICS_BUFFER_TARGET_BYTES;
                                buffer.drain(..dropped);
                                dropped_buffer_bytes =
                                    dropped_buffer_bytes.saturating_add(dropped as u64);
                                crate::services::logging::session(
                                    &metrics_app,
                                    "WARN",
                                    "metrics",
                                    &metrics_tid,
                                    format!(
                                        "collector buffer capped dropped_bytes={} total_dropped_bytes={} buffer_bytes={}",
                                        dropped,
                                        dropped_buffer_bytes,
                                        buffer.len(),
                                    ),
                                );
                            }
                        }
                        Ok(Some(ChannelMsg::ExtendedData { data, ext })) => {
                            if ext == METRICS_STDERR_EXTENDED_DATA_TYPE {
                                stderr_bytes = stderr_bytes.saturating_add(data.len() as u64);
                                append_metrics_stderr_tail(&mut stderr_tail, data.as_ref());
                            } else {
                                crate::services::logging::session(
                                    &metrics_app,
                                    "WARN",
                                    "metrics",
                                    &metrics_tid,
                                    format!(
                                        "collector unexpected extended data ext={} bytes={}",
                                        ext,
                                        data.len()
                                    ),
                                );
                            }
                        }
                        Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                            collector_exit_code = Some(exit_status);
                            remote_terminal_event = "exit-status";
                            crate::services::logging::session(
                                &metrics_app,
                                if exit_status == 0 { "INFO" } else { "WARN" },
                                "metrics",
                                    &metrics_tid,
                                    format!(
                                        "collector exit status exit_code={} stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={} elapsed_ms={}",
                                        exit_status,
                                        stdout_bytes,
                                        metrics_stdout_preview(&stdout_tail, sample_count),
                                        stderr_bytes,
                                        metrics_stderr_preview(&stderr_tail),
                                        collector_command_mode,
                                        collector_started_at.elapsed().as_millis(),
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    format!("collector exited with status {exit_status}"),
                                )
                                .await;
                            }
                            break "channel-exit";
                        }
                        Ok(Some(ChannelMsg::ExitSignal {
                            signal_name,
                            core_dumped,
                            error_message,
                            lang_tag,
                        })) => {
                            remote_terminal_event = "exit-signal";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "collector exit signal signal={signal_name:?} core_dumped={core_dumped} error_message={error_message:?} lang_tag={lang_tag:?} stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={}",
                                    stdout_bytes,
                                    metrics_stdout_preview(&stdout_tail, sample_count),
                                    stderr_bytes,
                                    metrics_stderr_preview(&stderr_tail),
                                    collector_command_mode,
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    format!("collector terminated by signal {signal_name:?}"),
                                )
                                .await;
                            }
                            break "channel-exit-signal";
                        }
                        Ok(Some(ChannelMsg::Eof)) => {
                            remote_terminal_event = "eof";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "remote collector sent EOF before exit status stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={}",
                                    stdout_bytes,
                                    metrics_stdout_preview(&stdout_tail, sample_count),
                                    stderr_bytes,
                                    metrics_stderr_preview(&stderr_tail),
                                    collector_command_mode,
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "remote collector sent EOF before exit status",
                                )
                                .await;
                            }
                            break "remote-eof";
                        }
                        Ok(Some(ChannelMsg::Close)) => {
                            remote_terminal_event = "close";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "remote collector sent channel close stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={}",
                                    stdout_bytes,
                                    metrics_stdout_preview(&stdout_tail, sample_count),
                                    stderr_bytes,
                                    metrics_stderr_preview(&stderr_tail),
                                    collector_command_mode,
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "remote collector sent channel close without exit status",
                                )
                                .await;
                            }
                            break "remote-close";
                        }
                        Ok(Some(ChannelMsg::Failure)) => {
                            remote_terminal_event = "failure";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "remote collector channel request failed stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={}",
                                    stdout_bytes,
                                    metrics_stdout_preview(&stdout_tail, sample_count),
                                    stderr_bytes,
                                    metrics_stderr_preview(&stderr_tail),
                                    collector_command_mode,
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "remote collector channel request failed",
                                )
                                .await;
                            }
                            break "remote-failure";
                        }
                        Ok(Some(ChannelMsg::OpenFailure(reason))) => {
                            remote_terminal_event = "open-failure";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                    format!(
                                        "remote collector channel open failure reason={reason:?} transport={}",
                                        collector_command_mode,
                                    ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "remote collector channel open failure",
                                )
                                .await;
                            }
                            break "remote-open-failure";
                        }
                        Ok(None) => {
                            remote_terminal_event = "stream-ended";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "collector channel stream ended without terminal event reason=remote-stream-ended-or-transport-drop stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={} elapsed_ms={}",
                                    stdout_bytes,
                                    metrics_stdout_preview(&stdout_tail, sample_count),
                                    stderr_bytes,
                                    metrics_stderr_preview(&stderr_tail),
                                    collector_command_mode,
                                    collector_started_at.elapsed().as_millis(),
                                ),
                            );
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "collector channel ended without exit status",
                                )
                                .await;
                            }
                            break "channel-closed";
                        }
                        _ => {}
                    }
                }
            }
        };

        close_metrics_channel(&mut channel, &metrics_app, &metrics_tid, close_reason).await;
        crate::services::logging::session(
            &metrics_app,
            "INFO",
            "metrics",
            &metrics_tid,
            format!(
                "collector stopped flow_role=target reason={close_reason} remote_terminal_event={remote_terminal_event} exit_code={} samples={} malformed_samples={} oversized_samples={} dropped_buffer_bytes={} stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={}",
                metrics_exit_status_label(collector_exit_code),
                sample_count,
                malformed_block_count,
                oversized_block_count,
                dropped_buffer_bytes,
                stdout_bytes,
                metrics_stdout_preview(&stdout_tail, sample_count),
                stderr_bytes,
                metrics_stderr_preview(&stderr_tail),
                collector_command_mode,
            ),
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

}
