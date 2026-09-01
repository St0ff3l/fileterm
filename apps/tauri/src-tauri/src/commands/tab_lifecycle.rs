// Reconnect, disconnect, close, and local terminal commands.
#[tauri::command]
pub async fn app_reconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tab_metadata = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .map(|t| (t.profile_id.clone(), t.session_type.clone()))
    };

    if let Some((profile_id, session_type)) = tab_metadata {
        if session_type == "local" {
            let should_start = {
                let mut tabs = state.tabs.write().await;
                claim_reconnect_tab(&mut tabs, &tab_id)
            };
            if !should_start {
                return get_workspace_snapshot(app).await;
            }

            stop_session_worker(&state, &tab_id).await;
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    session.connected = false;
                    if !session.terminal_transcript.is_empty() {
                        session
                            .terminal_transcript
                            .push_str("\r\n--- Local shell restarted ---\r\n");
                    }
                    session
                        .terminal_transcript
                        .push_str(crate::sessions::terminal::local_terminal_startup_transcript());
                }
            }
            state.touch_ai_session_revision(&tab_id).await;

            let mut launch = state
                .local_terminal_launches
                .read()
                .await
                .get(&tab_id)
                .cloned()
                .unwrap_or_else(crate::sessions::local_terminal::default_launch);
            if let Some(cwd) = state
                .sessions
                .read()
                .await
                .get(&tab_id)
                .and_then(|session| session.shell_cwd.clone())
            {
                launch.cwd = cwd;
            }
            match start_local_terminal_for_tab(&app, &state, &tab_id, launch).await {
                Ok(startup) => {
                    let startup_app = app.clone();
                    let startup_tab_id = tab_id.clone();
                    tauri::async_runtime::spawn(async move {
                        finish_local_terminal_startup(&startup_app, &startup_tab_id, startup, true)
                            .await;
                    });
                }
                Err(error) => {
                    crate::sessions::terminal::set_terminal_state(
                        &app,
                        &tab_id,
                        error,
                        crate::services::WorkspaceTabStatus::Error,
                    )
                    .await;
                }
            }
            return get_workspace_snapshot(app).await;
        }

        let pid = profile_id;
        let profiles = read_json_array(&app, "profiles.json")?;
        if let Some(profile) = profiles
            .iter()
            .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&pid))
        {
            let resolved_profile = resolve_profile_for_session(&app, profile)?;
            let profile = &resolved_profile;
            // Claim the reconnect before awaiting worker shutdown. Tauri can
            // dispatch Enter/button/auto-reconnect commands concurrently; a
            // status check performed after an await lets each caller replace
            // the worker and append another reconnect banner.
            let should_start = {
                let mut tabs = state.tabs.write().await;
                let should_start = claim_reconnect_tab(&mut tabs, &tab_id);
                if should_start {
                    if let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) {
                        tab.layout = create_tab_layout(profile);
                    }
                }
                should_start
            };
            if !should_start {
                return get_workspace_snapshot(app).await;
            }

            // Terminate existing worker
            stop_session_worker(&state, &tab_id).await;

            // Set connecting status. Preserve the existing transcript so the
            // renderer can re-hydrate the terminal with prior history on
            // reconnect (mirrors Electron's BoundedTextBuffer retention).
            // We only append a separator + "连接主机..." notice so the user
            // sees that a reconnect is in progress.
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    session.connected = false;
                    session.device_mode =
                        crate::services::workspace::configured_device_mode_for_profile(profile);
                    session.remote_files_loading = false;
                    session.shell_user = None;
                    if crate::services::workspace::ConnectionCapabilities::is_network_device_profile(
                        profile,
                    ) {
                        session.shell_cwd = None;
                    }
                    session.file_access_mode = "user".to_string();
                    session.has_reusable_sudo_auth = false;
                    session.reconnect_mode =
                        crate::services::workspace::reconnect_mode_for_profile(profile);
                    session.capabilities =
                        crate::services::workspace::ConnectionCapabilities::for_profile(profile);
                    session.follow_shell_cwd = session.capabilities.shell_integration;
                    // Append a reconnect separator instead of wiping history.
                    if !session.terminal_transcript.is_empty() {
                        session
                            .terminal_transcript
                            .push_str("\r\n--- 重新连接 ---\r\n");
                    }
                    session.terminal_transcript.push_str("连接主机...\r\n");
                    // Cap to 200k chars (matches Electron's BoundedTextBuffer).
                    if session.terminal_transcript.len() > 200_000 {
                        let mut cut = session.terminal_transcript.len() - 180_000;
                        while cut < session.terminal_transcript.len()
                            && !session.terminal_transcript.is_char_boundary(cut)
                        {
                            cut += 1;
                        }
                        session.terminal_transcript =
                            session.terminal_transcript[cut..].to_string();
                    }
                    session.remote_files = Vec::new();
                    session.system_metrics = None;
                }
            }
            state.touch_ai_session_revision(&tab_id).await;

            // Renderer-triggered reconnects apply the returned snapshot, but
            // auto-reconnect is initiated by the worker and has no renderer
            // caller to apply it. Broadcast the connecting snapshot for both
            // paths so the terminal/file panes cannot remain on stale state.
            if let Ok(snapshot) = get_workspace_snapshot(app.clone()).await {
                let _ = app.emit("workspace:snapshot", snapshot);
            }

            let (tx, rx) = mpsc::channel(100);
            let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("ssh");
            let (terminal_input_tx, terminal_input_rx) = if profile_type == "ssh" {
                let (sender, receiver) = mpsc::unbounded_channel();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
            let worker_control = CancellationToken::new();
            {
                let mut workers = state.workers.write().await;
                workers.insert(tab_id.clone(), tx);
            }
            if let Some(sender) = terminal_input_tx {
                state
                    .terminal_inputs
                    .write()
                    .await
                    .insert(tab_id.clone(), sender);
            }
            state
                .worker_controls
                .write()
                .await
                .insert(tab_id.clone(), worker_control.clone());

            if let Err(error) =
                crate::services::session_logs::start_for_tab(&app, &state, &tab_id, profile).await
            {
                crate::services::logging::warn(
                    &app,
                    "session-log",
                    format!("启动会话日志失败 tab={tab_id}: {error}"),
                );
            }

            start_session_worker(
                tab_id,
                profile.clone(),
                rx,
                terminal_input_rx,
                app.clone(),
                worker_control,
            );
        }
    }

    get_workspace_snapshot(app).await
}

fn claim_reconnect_tab(tabs: &mut [crate::services::WorkspaceTab], tab_id: &str) -> bool {
    let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) else {
        return false;
    };
    if tab.status == crate::services::WorkspaceTabStatus::Connecting {
        return false;
    }
    tab.status = crate::services::WorkspaceTabStatus::Connecting;
    true
}

#[tauri::command]
pub async fn app_disconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let is_local_terminal = app
        .state::<crate::services::workspace::WorkspaceState>()
        .tabs
        .read()
        .await
        .iter()
        .any(|tab| tab.id == tab_id && tab.session_type == "local");
    if is_local_terminal {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        stop_session_worker(&state, &tab_id).await;
        crate::sessions::terminal::set_terminal_state(
            &app,
            &tab_id,
            "Local shell stopped".to_string(),
            crate::services::WorkspaceTabStatus::Closed,
        )
        .await;
        return get_workspace_snapshot(app).await;
    }

    crate::services::transfers::pause_for_tab(&app, &tab_id, "连接断开，可在重连后继续传输")
        .await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let was_connected = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| session.connected)
        .unwrap_or(false);
    stop_session_worker(&state, &tab_id).await;
    state
        .serial_reconnect_attempts
        .write()
        .await
        .remove(&tab_id);
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = crate::services::WorkspaceTabStatus::Closed;
        }
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.connected = false;
            session.remote_files_loading = false;
            session.remote_files = Vec::new();
            session.shell_user = None;
            session.file_access_mode = "user".to_string();
            session.has_reusable_sudo_auth = false;
            session.system_metrics = None;
        }
    }
    state.touch_ai_session_revision(&tab_id).await;

    // Cancelling an SSH worker intentionally suppresses its normal worker
    // shutdown callback. Emit the same terminal notice/state that a network
    // disconnect would have emitted, otherwise the renderer only receives a
    // workspace snapshot and keeps showing the last shell prompt forever.
    if was_connected {
        crate::sessions::terminal::emit_terminal_data(&app, &tab_id, "\r\n连接已断开\r\n").await;
    }
    crate::sessions::terminal::set_terminal_state(
        &app,
        &tab_id,
        "连接已断开".to_string(),
        crate::services::WorkspaceTabStatus::Closed,
    )
    .await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_close_tab(app: AppHandle, tab_id: String) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    // 检查是否是分屏 root：如果是，关闭所有 leaf 的 worker 和传输
    let pane_leaf_ids: Vec<String> = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.pane_root.as_ref())
            .map(|root| root.leaf_tab_ids())
            .unwrap_or_default()
    };

    // 检查是否是某个 root 的 leaf
    let containing_root_id: Option<String> = if pane_leaf_ids.is_empty() {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| {
                t.pane_root
                    .as_ref()
                    .map(|root| root.leaf_tab_ids().iter().any(|id| id == &tab_id))
                    .unwrap_or(false)
            })
            .map(|t| t.id.clone())
    } else {
        None
    };

    if let Some(root_id) = containing_root_id {
        // tab_id 是某个 root 的 leaf，等价于 close_pane
        // 暂停传输
        let _ = crate::services::transfers::pause_for_tab(
            &app,
            &tab_id,
            "Pane 关闭后已暂停，可在重连后继续传输",
        )
        .await;
        stop_session_worker(&state, &tab_id).await;
        crate::services::session_logs::stop_for_tab(&state, &tab_id).await;
        state
            .serial_reconnect_attempts
            .write()
            .await
            .remove(&tab_id);
        {
            let mut tabs = state.tabs.write().await;
            let root_idx = tabs
                .iter()
                .position(|t| t.id == root_id)
                .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_id)))?;
            {
                let root_tab = &mut tabs[root_idx];
                if let Some(ref mut pane_root) = root_tab.pane_root {
                    pane_root.remove_leaf(&tab_id);
                    if let crate::services::PaneNode::Leaf { .. } = pane_root {
                        root_tab.pane_root = None;
                    }
                }
            }
            tabs.retain(|t| t.id != tab_id);
            let mut sessions = state.sessions.write().await;
            sessions.remove(&tab_id);
            state.local_terminal_launches.write().await.remove(&tab_id);
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            if let Some(root_tab) = tabs.get(root_idx) {
                if root_tab.pane_root.is_none() {
                    active_panes.remove(&root_id);
                } else if let Some(ref pane_root) = root_tab.pane_root {
                    let leaves = pane_root.leaf_tab_ids();
                    if active_panes
                        .get(&root_id)
                        .map(|id| id == &tab_id || !leaves.contains(id))
                        .unwrap_or(true)
                    {
                        if let Some(first) = leaves.first() {
                            active_panes.insert(root_id.clone(), first.clone());
                        }
                    }
                }
            }
        }
        state.remove_ai_session_revision(&tab_id).await;
    } else {
        // 普通关闭（可能是独立 tab 或分屏 root）
        let all_ids_to_close = if pane_leaf_ids.is_empty() {
            vec![tab_id.clone()]
        } else {
            pane_leaf_ids
        };

        for id in &all_ids_to_close {
            crate::services::transfers::pause_for_tab(
                &app,
                id,
                "标签关闭后已暂停，可在重连后继续传输",
            )
            .await?;
            stop_session_worker(&state, id).await;
            crate::services::session_logs::stop_for_tab(&state, id).await;
            state.serial_reconnect_attempts.write().await.remove(id);
        }
        {
            let mut tabs = state.tabs.write().await;
            tabs.retain(|t| !all_ids_to_close.contains(&t.id));

            let mut active = state.active_tab_id.write().await;
            if *active == Some(tab_id.clone()) {
                *active = tabs.last().map(|t| t.id.clone());
            }

            let mut sessions = state.sessions.write().await;
            for id in &all_ids_to_close {
                sessions.remove(id);
                state.local_terminal_launches.write().await.remove(id);
            }
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            active_panes.remove(&tab_id);
        }
        for id in &all_ids_to_close {
            state.remove_ai_session_revision(id).await;
        }
    }

    get_workspace_snapshot(app).await
}
