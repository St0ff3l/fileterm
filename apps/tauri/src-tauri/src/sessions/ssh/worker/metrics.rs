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
let platform = startup.platform;
let cancellation = startup.cancellation;
let state = startup.state;
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

}
