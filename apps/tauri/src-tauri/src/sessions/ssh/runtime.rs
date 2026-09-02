pub fn start_ssh_worker(
    tab_id: String,
    profile: Value,
    mut cmd_rx: mpsc::Receiver<WorkerCmd>,
    mut terminal_input_rx: mpsc::UnboundedReceiver<String>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    tokio::spawn(async move {
        let tid = tab_id.clone();
        let reconnect_policy = ReconnectPolicy::from_profile(&profile);
        crate::services::logging::session(&app, "INFO", "ssh", &tid, "worker started");
        // The initial "连接主机...\r\n" notice is already in the session
        // snapshot's `terminal_transcript` (set by `app_open_profile`), so
        // the renderer hydrates it via `bootText` — no need to emit it here.
        // Emitting here would race the renderer's listener registration.
        //
        // 监督层：run_worker_loop 过去直接 await 在这个 spawn 里，循环内任何
        // panic（例如热路径上的 String 字节切片切到多字节字符内部）都会无声
        // 杀死任务——JoinHandle 无人 await，没有日志、没有状态更新，renderer
        // 永远显示"已连接"，终端冻结且 Ctrl+C 无效。现在把循环放进独立任务
        // 并 await 其 JoinHandle：panic 转成 JoinError 后走下面统一的失败
        // 收尾路径（错误日志 + transcript 提示 + 状态广播 + 自动重连判断）。
        // 关闭链路只用 CancellationToken（无 abort），所以 JoinError 一定
        // 是 panic，不是正常取消。
        //
        // panic 位置由 logging::install_panic_hook 在 panic 发生时即写入文件
        // 日志（scope=panic），这里只负责把 JoinError 分类后落到 per-tab 日志
        // 和 transcript，便于和 panic hook 那行交叉定位。
        let worker_app = app.clone();
        let worker_cancellation = cancellation.clone();
        let worker_profile = profile.clone();
        let run_result = tokio::spawn(async move {
            run_worker_loop(
                &tab_id,
                &worker_profile,
                &mut cmd_rx,
                &mut terminal_input_rx,
                &worker_app,
                worker_cancellation,
            )
            .await
        })
        .await
        .unwrap_or_else(|join_error| {
            // JoinError.Display 不带源码位置，所以这里只输出分类 + 系统消息；
            // 真正的 panic 位置在 panic hook 写的那行里。
            let kind = if join_error.is_cancelled() {
                "cancelled"
            } else if join_error.is_panic() {
                "panic"
            } else {
                "aborted"
            };
            Err(format!("worker task {kind}: {join_error}"))
        });
        if cancellation.is_cancelled() {
            app.state::<crate::services::workspace::WorkspaceState>()
                .serial_reconnect_attempts
                .write()
                .await
                .remove(&tid);
            crate::services::logging::session(&app, "INFO", "ssh", &tid, "worker cancelled");
            return;
        }
        let should_reconnect = match &run_result {
            // Startup failures do not have a live worker exit classification,
            // so preserve the existing retry behavior for those failures.
            Err(_) => true,
            Ok(exit) => exit.should_reconnect(),
        };
        let connection_was_stable = run_result
            .as_ref()
            .ok()
            .is_some_and(|exit| exit.connection_was_stable);
        if connection_was_stable {
            // Reset only after a connection was genuinely usable for a while.
            // An accept-then-drop loop must keep its attempt count so the
            // configured limit and exponential backoff remain effective.
            app.state::<crate::services::workspace::WorkspaceState>()
                .serial_reconnect_attempts
                .write()
                .await
                .remove(&tid);
        }
        let final_status = match run_result {
            Ok(exit) => {
                crate::services::logging::session(
                    &app,
                    "INFO",
                    "ssh",
                    &tid,
                    format!("worker exited: {}", exit.description()),
                );
                if exit.should_reconnect() {
                    emit_terminal_data(&app, &tid, "SSH 连接中断，正在准备重连...\r\n").await;
                } else {
                    emit_terminal_data(&app, &tid, "连接已断开\r\n").await;
                }
                WorkspaceTabStatus::Closed
            }
            Err(e) => {
                crate::services::logging::session(
                    &app,
                    "ERROR",
                    "ssh",
                    &tid,
                    format!("worker failed: {e}"),
                );
                emit_terminal_data(&app, &tid, &format!("连接失败: {}\r\n", e)).await;
                let failure_code =
                    crate::services::connection_operations::ssh_connection_error_code(&e);
                app.state::<crate::services::workspace::WorkspaceState>()
                    .connection_operations
                    .fail_for_tab(&tid, failure_code)
                    .await;
                WorkspaceTabStatus::Error
            }
        };
        update_tab_status_and_emit(&app, &tid, final_status).await;

        // ── Auto-reconnect with bounded exponential backoff ────────────────
        // The attempt counter lives in WorkspaceState so it survives the
        // worker replacement triggered below. A successful connection or an
        // explicit close clears it; repeated failures therefore cannot create
        // a tight reconnect loop and a configured maximum is enforceable.
        // Read the live session policy instead of the worker's startup copy.
        // The connection editor can change reconnectMode while this worker is
        // still alive, and the next disconnect must use that new policy.
        let reconnect_mode = {
            let state = app.state::<crate::services::workspace::WorkspaceState>();
            let sessions = state.sessions.read().await;
            let mode = sessions
                .get(&tid)
                .and_then(|session| session.reconnect_mode.clone())
                .or_else(|| crate::services::workspace::reconnect_mode_for_profile(&profile));
            mode.unwrap_or_else(|| "none".to_string())
        };
        if should_reconnect && reconnect_mode == "auto" {
            let next_attempt = {
                let state = app.state::<crate::services::workspace::WorkspaceState>();
                let mut attempts = state.serial_reconnect_attempts.write().await;
                let current = attempts.get(&tid).copied().unwrap_or(0);
                let next = reconnect_policy
                    .next_attempt_after_connection(current, connection_was_stable);
                if let Some(attempt) = next {
                    attempts.insert(tid.clone(), attempt);
                }
                next
            };
            let Some(attempt) = next_attempt else {
                crate::services::logging::session(
                    &app,
                    "ERROR",
                    "ssh",
                    &tid,
                    "auto-reconnect limit reached",
                );
                update_tab_status_and_emit(&app, &tid, WorkspaceTabStatus::Error).await;
                return;
            };
            let delay = reconnect_policy.delay_for_attempt(attempt);
            crate::services::logging::session(
                &app,
                "INFO",
                "ssh",
                &tid,
                format!(
                    "auto-reconnect scheduled attempt={attempt} delay_ms={}",
                    delay.as_millis()
                ),
            );
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = cancellation.cancelled() => {
                    crate::services::logging::session(
                        &app,
                        "DEBUG",
                        "ssh",
                        &tid,
                        "auto-reconnect canceled by session shutdown",
                    );
                    return;
                }
            }

            // Re-check: tab may have been closed or already reconnected by
            // the user during the delay.
            let state = app.state::<crate::services::workspace::WorkspaceState>();
            let should_reconnect = {
                let tabs = state.tabs.read().await;
                let sessions = state.sessions.read().await;
                match (tabs.iter().find(|tab| tab.id == tid), sessions.get(&tid)) {
                    (Some(tab), Some(session)) => {
                        let mode = session
                            .reconnect_mode
                            .as_deref()
                            .or_else(|| profile.get("reconnectMode").and_then(Value::as_str))
                            .unwrap_or("none");
                        !cancellation.is_cancelled()
                            && tab.status != WorkspaceTabStatus::Connecting
                            && !session.connected
                            && mode == "auto"
                            && session.summary != "连接已断开"
                            && session.summary != "Connection closed"
                    }
                    _ => false,
                }
            };

            if should_reconnect {
                crate::services::logging::session(
                    &app,
                    "INFO",
                    "ssh",
                    &tid,
                    "auto-reconnect firing",
                );
                // Trigger reconnect via the same path the renderer uses.
                let _ = crate::commands::app_reconnect_tab(app.clone(), tid.clone()).await;
            } else {
                crate::services::logging::session(
                    &app,
                    "DEBUG",
                    "ssh",
                    &tid,
                    "auto-reconnect canceled",
                );
            }
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler implementation
// ─────────────────────────────────────────────────────────────────────────────

fn emit_ssh_interaction(
    app: &AppHandle,
    interaction_window_label: Option<&str>,
    payload: &Value,
) -> Result<(), tauri::Error> {
    let request_id = payload
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let flow_id = payload
        .get("flowId")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let tab_id = payload
        .get("tabId")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let kind = payload
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let stage = payload
        .get("stage")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let target = payload
        .get("authenticationTarget")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let hop = payload
        .get("hopIndex")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let sequence = payload
        .get("sequence")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let (result, route) = match interaction_window_label
        .and_then(|label| app.get_webview_window(label))
    {
        Some(window) => (
            app.emit_to(
                EventTarget::webview_window(window.label()),
                "ssh:interaction",
                payload,
            ),
            format!("window:{}", window.label()),
        ),
        None => (app.emit("ssh:interaction", payload), "broadcast".to_string()),
    };
    crate::services::logging::session(
        app,
        if result.is_ok() { "DEBUG" } else { "WARN" },
        "ssh",
        tab_id,
        format!(
            "interaction emitted flow_id={} request_id={} kind={} stage={} target={} hop={} sequence={} route={} result={}",
            flow_id,
            request_id.escape_default(),
            kind,
            stage,
            target,
            hop,
            sequence,
            route,
            result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .unwrap_or_else(|| "ok".to_string()),
        ),
    );
    result
}

pub struct ClientHandler {
    app: AppHandle,
    tab_id: String,
    profile_id: String,
    host: String,
    port: u16,
    trusted_fingerprint: Option<String>,
    host_verification_waiting: Arc<AtomicBool>,
    interaction_timeout: Duration,
    remote_sshid: SharedRemoteSshId,
    /// The WebView that owns this SSH interaction. Connection tests run in a
    /// standalone form window, so broadcasting the request to every WebView
    /// can race with window startup and leave the handshake waiting forever.
    /// Normal sessions use the main workspace window; if the target is gone,
    /// the emitter below still falls back to the app-wide event for recovery.
    interaction_window_label: Option<String>,
    interaction: SshInteractionContext,
    disconnect_reason: SharedSshDisconnectReason,
}

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn disconnected(
        &mut self,
        reason: russh::client::DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        let (info, transport_error) = match reason {
            russh::client::DisconnectReason::ReceivedDisconnect(info) => (
                SshDisconnectInfo {
                    kind: SshDisconnectKind::Remote,
                    message: format!(
                        "code={:?} message={}",
                        info.reason_code, info.message
                    ),
                },
                None,
            ),
            russh::client::DisconnectReason::Error(error) => {
                let message = error.to_string();
                (
                    SshDisconnectInfo {
                        kind: SshDisconnectKind::Transport,
                        message,
                    },
                    Some(error),
                )
            }
        };
        if let Ok(mut slot) = self.disconnect_reason.lock() {
            if slot.is_none() {
                *slot = Some(info.clone());
            }
        }
        crate::services::logging::session(
            &self.app,
            if matches!(info.kind, SshDisconnectKind::Transport) {
                "ERROR"
            } else {
                "WARN"
            },
            "ssh",
            &self.tab_id,
            format!(
                "SSH transport disconnected kind={} reason={}",
                info.kind.as_str(),
                info.message
            ),
        );
        if let Some(error) = transport_error {
            Err(error)
        } else {
            Ok(())
        }
    }

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        _names: &russh::Names,
        session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        if let Ok(mut remote_sshid) = self.remote_sshid.lock() {
            *remote_sshid = Some(session.remote_sshid().to_vec());
        }
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fp = fingerprint_sha256_base64(&server_public_key.public_key());
        crate::services::logging::session(
            &self.app,
            "DEBUG",
            "ssh",
            &self.tab_id,
            format!(
                "host-key verification host={} port={}",
                self.host, self.port
            ),
        );
        // Short-circuit: if the profile already trusts this exact
        // fingerprint, accept without prompting. This is the common path
        // after the user previously chose "accept-and-save".
        if let Some(known) = &self.trusted_fingerprint {
            if known == &fp {
                crate::services::logging::session(
                    &self.app,
                    "INFO",
                    "ssh",
                    &self.tab_id,
                    "host-key matched saved fingerprint",
                );
                return Ok(true);
            }
            crate::services::logging::session(
                &self.app,
                "WARN",
                "ssh",
                &self.tab_id,
                "host-key mismatch; requesting user verification",
            );
        }
        let known = self.trusted_fingerprint.clone();
        let request_id = uuid::Uuid::new_v4().to_string();
        let sequence = self.interaction.next_sequence();
        let expires_at = ssh_interaction_expires_at(self.interaction_timeout);
        let (tx, rx) = oneshot::channel::<Value>();
        let pending_count = {
            let state = self
                .app
                .state::<crate::services::workspace::WorkspaceState>();
            let mut pending = state.pending_interactions.write().await;
            pending.insert(request_id.clone(), tx);
            pending.len()
        };
        let _pending_cleanup = PendingSshInteractionGuard::new(
            &self.app,
            &self.interaction.tab_id,
            &request_id,
        );
        self.host_verification_waiting
            .store(true, Ordering::Release);
        self.interaction.log_interaction(
            &self.app,
            "DEBUG",
            &request_id,
            "host-verification",
            "host-key",
            sequence,
            format!(
                "queued known_fingerprint={} pending={} timeout_secs={}",
                known.is_some(),
                pending_count,
                self.interaction_timeout.as_secs(),
            ),
        );
        // Emit a `host-verification` interaction request. The payload shape
        // matches `SshHostVerificationRequest` in packages/core so the
        // renderer's `useSshInteractions` hook recognises it and shows the
        // accept/reject dialog. The renderer resolves via
        // `app_resolve_ssh_interaction`, which forwards the response back
        // through the oneshot channel.
        let payload = serde_json::json!({
            "requestId": request_id,
            "kind": "host-verification",
            "flowId": self.interaction.flow.flow_id,
            "tabId": self.tab_id,
            "profileId": self.profile_id,
            "connectionName": self.interaction.connection_name,
            "authenticationTarget": self.interaction.authentication_target.as_str(),
            "hopIndex": self.interaction.hop_index,
            "stage": "host-key",
            "sequence": sequence,
            "expiresAt": expires_at,
            "host": self.host,
            "port": self.port,
            "fingerprint": fp,
            "knownFingerprint": known,
        });
        let emit_result = emit_ssh_interaction(
            &self.app,
            self.interaction_window_label.as_deref(),
            &payload,
        );
        if emit_result.is_err() {
            self.host_verification_waiting
                .store(false, Ordering::Release);
            let (_, pending_after) =
                remove_pending_ssh_interaction(&self.app, &request_id).await;
            self.interaction.log_interaction(
                &self.app,
                "WARN",
                &request_id,
                "host-verification",
                "host-key",
                sequence,
                format!("event emission failed pending={pending_after}"),
            );
            return Ok(false);
        }
        let response = wait_for_ssh_interaction(
            &self.interaction,
            rx,
            self.interaction_timeout,
        )
        .await;
        // The renderer normally removes this entry when it resolves the
        // interaction. A timeout has no renderer response, so clean it up
        // here to prevent stale host-key requests from affecting later
        // connection attempts.
        let (_, pending_after) =
            remove_pending_ssh_interaction(&self.app, &request_id).await;
        self.host_verification_waiting
            .store(false, Ordering::Release);
        let decision = match response {
            SshInteractionWaitResult::Response(response) => {
                let decision = response
                    .get("decision")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cancel")
                    .to_string();
                self.interaction.log_interaction(
                    &self.app,
                    "INFO",
                    &request_id,
                    "host-verification",
                    "host-key",
                    sequence,
                    format!("resolved decision={} pending={pending_after}", decision),
                );
                decision
            }
            SshInteractionWaitResult::ReceiverClosed => {
                self.interaction.log_interaction(
                    &self.app,
                    "WARN",
                    &request_id,
                    "host-verification",
                    "host-key",
                    sequence,
                    format!("renderer receiver closed pending={pending_after}"),
                );
                "cancel".to_string()
            }
            SshInteractionWaitResult::Timeout => {
                self.interaction.log_interaction(
                    &self.app,
                    "WARN",
                    &request_id,
                    "host-verification",
                    "host-key",
                    sequence,
                    format!(
                        "expired reason=interaction-timeout timeout_secs={} pending={pending_after}",
                        self.interaction_timeout.as_secs()
                    ),
                );
                "cancel".to_string()
            }
            SshInteractionWaitResult::Cancelled => {
                self.interaction.log_interaction(
                    &self.app,
                    "INFO",
                    &request_id,
                    "host-verification",
                    "host-key",
                    sequence,
                    format!(
                        "canceled reason=connection-cancelled pending={pending_after}"
                    ),
                );
                "cancel".to_string()
            }
        };
        match decision.as_str() {
            "accept-and-save" => {
                // Persist the trusted fingerprint so future connects
                // short-circuit the prompt.
                let library_mutation = self
                    .app
                    .state::<crate::services::workspace::WorkspaceState>()
                    .library_mutation
                    .clone();
                let _guard = library_mutation.lock().await;
                let _ = crate::services::profile_ops::update_trusted_host_fingerprint(
                    &self.app,
                    &self.profile_id,
                    &fp,
                )
                .await;
                self.trusted_fingerprint = Some(fp);
                Ok(true)
            }
            "accept-once" => Ok(true),
            _ => Ok(false),
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let state = self
            .app
            .state::<crate::services::workspace::WorkspaceState>();
        let target = {
            let forwards = state.remote_forwards.read().await;
            forwards
                .get(&self.tab_id)
                .and_then(|rules| {
                    rules.iter().find(|rule| {
                        rule.bind_port == connected_port
                            && remote_bind_host_matches(&rule.bind_host, connected_address)
                    })
                })
                .cloned()
        };

        let Some(target) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };

        reply.accept().await;
        let tab_id = self.tab_id.clone();
        let app = self.app.clone();
        tokio::spawn(async move {
            let result = async {
                // 加 timeout：远端转发的目标 host 卡住时 TcpStream::connect
                // 会永久 await，spawn task 不退出，远端发起方也一直等。
                // 10 秒覆盖正常 RTT，超时后清理 task 让远端拿到连接重置。
                let mut local = timeout(
                    Duration::from_secs(10),
                    TcpStream::connect((&*target.target_host, target.target_port)),
                )
                .await
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "remote forward target connect timed out",
                    )
                })??;
                let mut remote = channel.into_stream();
                copy_bidirectional(&mut local, &mut remote).await?;
                Ok::<(), std::io::Error>(())
            }
            .await;
            if let Err(error) = result {
                crate::services::logging::session(
                    &app,
                    "WARN",
                    "tunnel",
                    &tab_id,
                    format!("remote forward connection failed: {error}"),
                );
            }
        });
        Ok(())
    }
}
