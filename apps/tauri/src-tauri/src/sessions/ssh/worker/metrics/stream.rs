async fn read_metrics_channel(
    runtime: &mut MetricsRuntime,
    prepared: PreparedMetrics,
    retry_rx: &mut mpsc::Receiver<()>,
) -> &'static str {
    let PreparedMetrics {
        mut channel,
        script_body,
        collector_request_pty,
        collector_command_mode,
        marker,
    } = prepared;
    let metrics_app = runtime.app.clone();
    let metrics_tid = runtime.tab_id.clone();
    let metrics_plat = runtime.platform.clone();
    let target_host = runtime.host.clone();
    let target_port = runtime.port;
    let route_hint = runtime.route_hint;
    let collector_started_at = Instant::now();
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
    let mut pty_fallback_sent = false;
    let close_reason = 'collector: loop {
        tokio::select! {
            biased;
            _ = runtime.attempt_cancellation.cancelled() => { break "session-cancelled"; }
            _ = retry_rx.recv(), if runtime.status.phase == "paused" => { break "manual-retry"; }
            _ = tokio::time::sleep_until(runtime.next_deadline()) => {
                if runtime.mark_stalled().await {
                    break if runtime.health.last_sample.is_some() { "stalled-channel" } else { "first-sample-timeout" };
                }
            }
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        stdout_bytes = stdout_bytes.saturating_add(data.len() as u64);
                        append_metrics_stderr_tail(&mut stdout_tail, data.as_ref());
                        if interactive_gateway_menu_detected(&stdout_tail, sample_count) {
                            remote_terminal_event = "interactive-gateway-menu";
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "interactive gateway menu received on auxiliary metrics channel; target route is unavailable flow_role=target route_hint={route_hint} stdout_bytes={} reason=target-route-required unavailable_reason=interactive-gateway-target-route-required",
                                    stdout_bytes,
                                ),
                            );
                            if !runtime.attempt_cancellation.is_cancelled() {
                                runtime.failure(
                                    "interactive gateway menu received on auxiliary metrics channel; target route is unavailable",
                                )
                                .await;
                            }
                            break 'collector "interactive-gateway-menu";
                        }
                        if !pty_fallback_sent
                            && collector_request_pty
                            && script_body.is_some()
                            && target_shell_prompt_detected(&stdout_tail, sample_count)
                        {
                            pty_fallback_sent = true;
                            remote_terminal_event = "target-shell-prompt-fallback";
                            let script = script_body
                                .as_deref()
                                .expect("script_body checked before PTY fallback");
                            crate::services::logging::session(
                                &metrics_app,
                                "WARN",
                                "metrics",
                                &metrics_tid,
                                format!(
                                    "target shell prompt received before first metrics sample; retrying collector through PTY stdin flow_role=target fallback_mode=pty-shell-stdin script_bytes={} stdout_bytes={} route_hint={route_hint}",
                                    script.len(),
                                    stdout_bytes,
                                ),
                            );
                            if let Err(error) = send_metrics_pty_fallback(
                                &mut channel,
                                &metrics_app,
                                &metrics_tid,
                                script,
                            )
                            .await
                            {
                                runtime.failure(
                                    error,
                                )
                                .await;
                                break 'collector "pty-fallback-write-error";
                            }
                        }
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
                            if !metrics_block_has_sample(&block) || (cpu_pct < 0.0 && mem_pct < 0.0) {
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
                                            "target identity rejected before first snapshot flow_role=target target={target_host}:{target_port} profile_endpoint={target_host}:{target_port} route_hint={route_hint} platform={} remote_ip={remote_ip:?} remote_hostname={remote_hostname:?} remote_os={remote_os:?} remote_kernel={remote_kernel:?} reason=missing-or-incompatible-identity",
                                            metrics_plat,
                                        ),
                                    );
                                    runtime.failure(
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
                                        "target identity validated before first snapshot flow_role=target target={target_host}:{target_port} profile_endpoint={target_host}:{target_port} route_hint={route_hint} platform={} remote_ip={remote_ip:?} remote_hostname={remote_hostname:?} remote_os={remote_os:?} remote_kernel={remote_kernel:?}",
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
                                        "first sample flow_role=target target={target_host}:{target_port} profile_endpoint={target_host}:{target_port} route_hint={route_hint} cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1} remote_ip={} remote_hostname={:?} remote_os={:?} remote_kernel={:?}",
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
                            if !runtime.sample(val).await { break 'collector "session-replaced"; }
                            // A sample can win a race with a click accepted while paused.
                            // Do not keep that stale click for the next outage.
                            while retry_rx.try_recv().is_ok() {}

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
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
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
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                format!("collector exited with status {exit_status}"),
                            )
                            .await;
                        }
                        break "channel-exit";
                    }
                    Some(ChannelMsg::ExitSignal {
                        signal_name,
                        core_dumped,
                        error_message,
                        lang_tag,
                    }) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                format!("collector terminated by signal {signal_name:?}"),
                            )
                            .await;
                        }
                        break "channel-exit-signal";
                    }
                    Some(ChannelMsg::Eof) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                "remote collector sent EOF before exit status",
                            )
                            .await;
                        }
                        break "remote-eof";
                    }
                    Some(ChannelMsg::Close) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                "remote collector sent channel close without exit status",
                            )
                            .await;
                        }
                        break "remote-close";
                    }
                    Some(ChannelMsg::Failure) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                "remote collector channel request failed",
                            )
                            .await;
                        }
                        break "remote-failure";
                    }
                    Some(ChannelMsg::OpenFailure(reason)) => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
                                "remote collector channel open failure",
                            )
                            .await;
                        }
                        break "remote-open-failure";
                    }
                    None => {
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
                        if !runtime.attempt_cancellation.is_cancelled() {
                            runtime.failure(
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

    if !runtime.attempt_cancellation.is_cancelled() && runtime.status.phase != "unsupported" {
        runtime.status.phase = "recovering".into();
        runtime.status.next_retry_at = None;
        runtime.publish(None).await;
    }
    close_metrics_channel(&mut channel, &metrics_app, &metrics_tid, close_reason).await;
    crate::services::logging::session(
            &metrics_app,
            "INFO",
            "metrics",
            &metrics_tid,
            format!(
                "collector stopped flow_role=target reason={close_reason} remote_terminal_event={remote_terminal_event} exit_code={} samples={} malformed_samples={} oversized_samples={} dropped_buffer_bytes={} stdout_bytes={} stdout_tail={} stderr_bytes={} stderr_tail={} transport={} pty_fallback_sent={}",
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
                pty_fallback_sent,
            ),
        );

    close_reason
}
