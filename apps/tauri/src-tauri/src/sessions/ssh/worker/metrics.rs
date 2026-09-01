const METRICS_MAX_BLOCK_BYTES: usize = 256 * 1024;
const METRICS_MAX_BUFFER_BYTES: usize = 1_000_000;
const METRICS_BUFFER_TARGET_BYTES: usize = 500_000;
const METRICS_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

fn should_log_metrics_anomaly(count: u64) -> bool {
    count <= 3 || count.is_power_of_two()
}

async fn close_metrics_channel(
    channel: &mut Channel<russh::client::Msg>,
    app: &AppHandle,
    tab_id: &str,
    reason: &str,
) {
    match timeout(METRICS_CLOSE_TIMEOUT, channel.close()).await {
        Ok(Ok(())) => crate::services::logging::session(
            app,
            "DEBUG",
            "metrics",
            tab_id,
            format!("collector channel closed reason={reason}"),
        ),
        Ok(Err(error)) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!("collector channel close failed reason={reason} error={error}"),
        ),
        Err(_) => crate::services::logging::session(
            app,
            "WARN",
            "metrics",
            tab_id,
            format!(
                "collector channel close timed out reason={reason} timeout_secs={}",
                METRICS_CLOSE_TIMEOUT.as_secs()
            ),
        ),
    }
}

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

        if let Some(script) = script_body.as_deref() {
            // 写脚本也加 timeout：Windows OpenSSH 在大脚本场景偶发 stall，
            // 不加超时会让 metrics task 永久卡在 data() 调用上。
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

        crate::services::logging::session(
            &metrics_app,
            "INFO",
            "metrics",
            &metrics_tid,
            format!(
                "collector started; waiting for first sample idle_timeout_secs={}",
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
        let close_reason = loop {
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
                                sample_count += 1;
                                if sample_count == 1 {
                                    crate::services::logging::session(
                                        &metrics_app,
                                        "INFO",
                                        "metrics",
                                        &metrics_tid,
                                        format!("first sample cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1}"),
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
                                if let Err(error) = metrics_app.emit("workspace:sessionMetrics", payload) {
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
                        Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                            buffer.extend_from_slice(data.as_ref());
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
                                        "collector stderr buffer capped dropped_bytes={} total_dropped_bytes={} buffer_bytes={}",
                                        dropped,
                                        dropped_buffer_bytes,
                                        buffer.len(),
                                    ),
                                );
                            }
                        }
                        Ok(Some(ChannelMsg::ExitStatus { .. })) | Ok(None) => {
                            if !metrics_cancellation.is_cancelled() {
                                disable_resource_monitoring_capability(
                                    &metrics_app,
                                    &metrics_tid,
                                    "collector channel closed",
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
                "collector stopped reason={close_reason} samples={} malformed_samples={} oversized_samples={} dropped_buffer_bytes={}",
                sample_count,
                malformed_block_count,
                oversized_block_count,
                dropped_buffer_bytes,
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
