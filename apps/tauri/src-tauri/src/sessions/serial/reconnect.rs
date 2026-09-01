async fn schedule_auto_reconnect(
    app: &AppHandle,
    tab_id: &str,
    profile: &Value,
    cancellation: CancellationToken,
) {
    let reconnect_mode = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let sessions = state.sessions.read().await;
        sessions
            .get(tab_id)
            .and_then(|session| session.reconnect_mode.clone())
            .or_else(|| crate::services::workspace::reconnect_mode_for_profile(profile))
            .unwrap_or_else(|| "none".to_string())
    };
    if reconnect_mode != "auto" {
        return;
    }

    let policy = ReconnectPolicy::from_profile(profile);
    let attempt = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut attempts = state.serial_reconnect_attempts.write().await;
        let previous = attempts.get(tab_id).copied().unwrap_or_default();
        if let Some(next) = policy.next_attempt(previous) {
            attempts.insert(tab_id.to_string(), next);
            Some(next)
        } else {
            attempts.remove(tab_id);
            None
        }
    };
    let Some(attempt) = attempt else {
        let maximum = policy
            .max_attempts
            .map(|value| value.to_string())
            .unwrap_or_else(|| "∞".to_string());
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("auto-reconnect stopped after reaching max_attempts={maximum}"),
        );
        emit_terminal_data(
            app,
            tab_id,
            &format!("\r\n[串口] 自动重连已达到上限（{maximum} 次）\r\n"),
        )
        .await;
        return;
    };
    let delay = policy.delay_for_attempt(attempt);

    crate::services::logging::session(
        app,
        "INFO",
        "serial",
        tab_id,
        format!(
            "auto-reconnect scheduled attempt={attempt} delay_ms={}",
            delay.as_millis()
        ),
    );
    emit_terminal_data(
        app,
        tab_id,
        &format!(
            "\r\n[串口] 将在 {} 秒后自动重连（第 {attempt} 次）\r\n",
            delay.as_secs()
        ),
    )
    .await;
    tokio::select! {
        _ = tokio::time::sleep(delay) => {}
        _ = cancellation.cancelled() => {
            crate::services::logging::session(
                app,
                "DEBUG",
                "serial",
                tab_id,
                "auto-reconnect canceled by session shutdown",
            );
            return;
        }
    }

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let should_reconnect = {
        let tabs = state.tabs.read().await;
        let sessions = state.sessions.read().await;
        let Some(tab) = tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let Some(session) = sessions.get(tab_id) else {
            return;
        };
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
            && session.summary != "串口已断开"
    };

    if should_reconnect {
        crate::services::logging::session(app, "INFO", "serial", tab_id, "auto-reconnect firing");
        let _ = crate::commands::app_reconnect_tab(app.clone(), tab_id.to_string()).await;
    } else {
        crate::services::logging::session(
            app,
            "DEBUG",
            "serial",
            tab_id,
            "auto-reconnect canceled",
        );
    }
}
