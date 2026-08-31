// Session worker spawning and local terminal startup.
/// 为指定 profile 创建并启动一个新的 session（tab + session + worker）。
/// 返回新 tab_id。调用者负责更新 active_tab_id、paneRoot 以及 emit snapshot。
///
/// 抽取自 `app_open_profile`，供 `app_split_tab` 复用：分屏时基于当前 profile
/// 新建一个独立 session，不共享 PTY。
#[derive(Clone, Copy, Default)]
struct SessionSpawnOptions {
    is_background: bool,
    source: Option<crate::services::WorkspaceSessionSource>,
}

async fn spawn_session_for_profile(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    profile: &serde_json::Value,
    profile_id: &str,
    pane_root_tab_id: Option<String>,
    connection_operation_id: Option<&str>,
    options: SessionSpawnOptions,
) -> Result<String, AppError> {
    let resolved_profile = resolve_profile_for_session(app, profile)?;
    let profile = &resolved_profile;
    let profile_type = profile
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("ssh");
    let name = profile
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("SSH Session");

    let tab_id = format!("tab-{}", uuid::Uuid::new_v4());
    let capabilities = crate::services::workspace::ConnectionCapabilities::for_profile(profile);
    let new_tab = crate::services::WorkspaceTab {
        id: tab_id.clone(),
        profile_id: profile_id.to_string(),
        session_type: profile_type.to_string(),
        title: name.to_string(),
        layout: create_tab_layout(profile),
        status: crate::services::WorkspaceTabStatus::Connecting,
        is_background: options.is_background,
        source: options.source,
        pane_root: None,
        pane_root_tab_id,
    };

    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| profile.get("devicePath").and_then(Value::as_str))
        .unwrap_or("127.0.0.1");
    let port = match profile_type {
        "ssh" => crate::sessions::reconnect::port_from_profile(profile, 22, "SSH")
            .map_err(AppError::Command)?,
        "ftp" => crate::sessions::reconnect::port_from_profile(profile, 21, "FTP")
            .map_err(AppError::Command)?,
        "telnet" => crate::sessions::reconnect::port_from_profile(profile, 23, "Telnet")
            .map_err(AppError::Command)?,
        // Serial profiles do not use the network port field. Keep the legacy
        // snapshot value for display without allowing it to affect opening.
        _ => profile
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or(0),
    };
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root");
    let initial_remote_path = crate::services::workspace::initial_remote_path_for_profile(profile);

    if let Some(operation_id) = connection_operation_id {
        state
            .connection_operations
            .attach_tab(operation_id, &tab_id)
            .await
            .map_err(AppError::Command)?;
    }

    {
        let mut tabs = state.tabs.write().await;
        tabs.push(new_tab);
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            tab_id.clone(),
            crate::services::SessionSnapshot {
                profile_id: profile_id.to_string(),
                ai_session_revision: "0".to_string(),
                device_mode: crate::services::workspace::configured_device_mode_for_profile(
                    profile,
                ),
                access_host: format!("{}:{}", host, port),
                summary: format!("{}@{}", username, host),
                terminal_transcript: "连接主机...\r\n".to_string(),
                remote_path: initial_remote_path,
                shell_cwd: None,
                follow_shell_cwd: capabilities.shell_integration,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: None,
                shell_user: None,
                connected: false,
                system_metrics: None,
                capabilities,
                remote_capabilities: None,
                reconnect_mode: crate::services::workspace::reconnect_mode_for_profile(profile),
            },
        );
    }

    let (tx, rx) = mpsc::channel(100);
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
        crate::services::session_logs::start_for_tab(app, state, &tab_id, profile).await
    {
        crate::services::logging::warn(
            app,
            "session-log",
            format!("启动会话日志失败 tab={tab_id}: {error}"),
        );
    }

    start_session_worker(
        tab_id.clone(),
        profile.clone(),
        rx,
        terminal_input_rx,
        app.clone(),
        worker_control,
    );

    Ok(tab_id)
}

/// Creates one isolated local PTY and exposes it through the same runtime
/// workspace model as a remote session. A local terminal is deliberately not
/// persisted as a connection profile.
async fn spawn_local_terminal_tab(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    launch: crate::sessions::local_terminal::LocalTerminalLaunch,
    pane_root_tab_id: Option<String>,
) -> String {
    let tab_id = format!("local-{}", uuid::Uuid::new_v4());
    let is_split_pane = pane_root_tab_id.is_some();
    crate::services::logging::session(
        app,
        "INFO",
        "local",
        &tab_id,
        format!(
            "open requested split={} shell={} cwd={} args={} env_entries={}",
            is_split_pane,
            launch.shell,
            launch.cwd,
            launch.args.len(),
            launch.env.len()
        ),
    );
    let capabilities =
        crate::services::workspace::ConnectionCapabilities::for_session_type("local");

    {
        let mut tabs = state.tabs.write().await;
        tabs.push(crate::services::WorkspaceTab {
            id: tab_id.clone(),
            profile_id: "__local_terminal__".to_string(),
            session_type: "local".to_string(),
            title: launch
                .title
                .clone()
                .unwrap_or_else(|| "Local Terminal".to_string()),
            layout: "terminal-only".to_string(),
            status: crate::services::WorkspaceTabStatus::Connecting,
            is_background: false,
            source: None,
            pane_root: None,
            pane_root_tab_id,
        });
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            tab_id.clone(),
            crate::services::SessionSnapshot {
                profile_id: "__local_terminal__".to_string(),
                ai_session_revision: "0".to_string(),
                device_mode: None,
                access_host: launch.cwd.clone(),
                summary: launch.shell.clone(),
                terminal_transcript: crate::sessions::terminal::local_terminal_startup_transcript()
                    .to_string(),
                remote_path: launch.cwd.clone(),
                shell_cwd: Some(launch.cwd.clone()),
                follow_shell_cwd: false,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: None,
                shell_user: None,
                connected: false,
                system_metrics: None,
                capabilities,
                remote_capabilities: None,
                reconnect_mode: None,
            },
        );
    }

    match start_local_terminal_for_tab(app, state, &tab_id, launch).await {
        Ok(startup) if is_split_pane => {
            finish_local_terminal_startup(app, &tab_id, startup, false).await;
        }
        Ok(startup) => {
            let startup_app = app.clone();
            let startup_tab_id = tab_id.clone();
            tauri::async_runtime::spawn(async move {
                finish_local_terminal_startup(&startup_app, &startup_tab_id, startup, true).await;
            });
        }
        Err(error) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "local",
                &tab_id,
                format!("PTY worker start failed error={error}"),
            );
            if is_split_pane {
                crate::sessions::terminal::set_terminal_state_without_snapshot(
                    app,
                    &tab_id,
                    error,
                    crate::services::WorkspaceTabStatus::Error,
                )
                .await;
            } else {
                crate::sessions::terminal::set_terminal_state(
                    app,
                    &tab_id,
                    error,
                    crate::services::WorkspaceTabStatus::Error,
                )
                .await;
            }
        }
    }

    tab_id
}

struct LocalTerminalStartup {
    runtime_id: String,
    ready: oneshot::Receiver<()>,
}

async fn finish_local_terminal_startup(
    app: &AppHandle,
    tab_id: &str,
    startup: LocalTerminalStartup,
    emit_snapshot: bool,
) {
    let startup_started_at = Instant::now();
    let readiness = match timeout(LOCAL_TERMINAL_STARTUP_READY_TIMEOUT, startup.ready).await {
        Ok(Ok(())) => "transport-ready",
        Ok(Err(_)) => "ready-channel-closed",
        Err(_) => "timeout",
    };
    let wait_ms = startup_started_at.elapsed().as_millis();
    crate::services::logging::session(
        app,
        "INFO",
        "local",
        tab_id,
        format!(
            "startup readiness={} wait_ms={} runtime={} emit_snapshot={}",
            readiness, wait_ms, startup.runtime_id, emit_snapshot
        ),
    );
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_current_runtime = state
        .local_terminal_runtime_ids
        .read()
        .await
        .get(tab_id)
        .is_some_and(|runtime_id| runtime_id == &startup.runtime_id);
    if !is_current_runtime {
        crate::services::logging::debug(
            app,
            "local",
            format!(
                "startup state update skipped tab={} runtime={} reason=runtime-replaced",
                tab_id, startup.runtime_id
            ),
        );
        return;
    }

    if readiness == "ready-channel-closed" {
        crate::services::logging::warn(
            app,
            "local",
            format!(
                "startup state update skipped tab={} runtime={} reason=worker-closed-before-ready",
                tab_id, startup.runtime_id
            ),
        );
        return;
    }

    if emit_snapshot {
        crate::sessions::terminal::set_terminal_state(
            app,
            tab_id,
            "Local shell started".to_string(),
            crate::services::WorkspaceTabStatus::Connected,
        )
        .await;
    } else {
        crate::sessions::terminal::set_terminal_state_without_snapshot(
            app,
            tab_id,
            "Local shell started".to_string(),
            crate::services::WorkspaceTabStatus::Connected,
        )
        .await;
    }
    crate::services::logging::info(
        app,
        "local",
        format!(
            "startup state connected tab={} runtime={} snapshot_emitted={}",
            tab_id, startup.runtime_id, emit_snapshot
        ),
    );
}

async fn start_local_terminal_for_tab(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    launch: crate::sessions::local_terminal::LocalTerminalLaunch,
) -> Result<LocalTerminalStartup, String> {
    let (worker_tx, worker_rx) = mpsc::channel(16);
    let (terminal_input_tx, terminal_input_rx) = mpsc::unbounded_channel();
    let worker_control = CancellationToken::new();
    let runtime_id = uuid::Uuid::new_v4().to_string();
    let runtime_gate = Arc::new(crate::services::workspace::LocalTerminalRuntimeGate::new());
    state
        .workers
        .write()
        .await
        .insert(tab_id.to_string(), worker_tx);
    state
        .terminal_inputs
        .write()
        .await
        .insert(tab_id.to_string(), terminal_input_tx);
    state
        .worker_controls
        .write()
        .await
        .insert(tab_id.to_string(), worker_control.clone());
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .insert(tab_id.to_string(), runtime_id.clone());
    state
        .local_terminal_runtime_gates
        .write()
        .await
        .insert(tab_id.to_string(), runtime_gate.clone());
    state
        .local_terminal_launches
        .write()
        .await
        .insert(tab_id.to_string(), launch.clone());

    crate::services::logging::session(
        app,
        "DEBUG",
        "local",
        tab_id,
        format!(
            "runtime registered runtime={} shell={} cwd={} args={} env_entries={}",
            runtime_id,
            launch.shell,
            launch.cwd,
            launch.args.len(),
            launch.env.len()
        ),
    );

    let startup_ready = match crate::sessions::local_terminal::start_local_terminal_worker(
        tab_id.to_string(),
        runtime_id.clone(),
        worker_rx,
        terminal_input_rx,
        app.clone(),
        worker_control,
        launch,
        runtime_gate,
    ) {
        Ok(startup_ready) => startup_ready,
        Err(error) => {
            state.workers.write().await.remove(tab_id);
            state.terminal_inputs.write().await.remove(tab_id);
            state.worker_controls.write().await.remove(tab_id);
            state
                .local_terminal_runtime_ids
                .write()
                .await
                .remove(tab_id);
            crate::sessions::local_terminal::deactivate_local_terminal_runtime(state, tab_id).await;
            return Err(error);
        }
    };

    crate::services::logging::session(
        app,
        "INFO",
        "local",
        tab_id,
        format!("PTY worker started runtime={runtime_id}"),
    );

    Ok(LocalTerminalStartup {
        runtime_id,
        ready: startup_ready,
    })
}

fn supports_split_panes(session_type: &str) -> bool {
    matches!(session_type, "ssh" | "local")
}

/// Atomically attach a newly created session to the current pane tree.
///
/// This function does not start or stop any session. Keeping the tree
/// mutation synchronous makes it possible for `app_split_tab` to distinguish
/// a successful attachment from a stale source/tree and roll the new session
/// back in the latter case.
fn attach_split_pane_to_tabs(
    tabs: &mut [crate::services::WorkspaceTab],
    source_tab_id: &str,
    new_tab_id: &str,
    split_direction: crate::services::SplitDirection,
) -> Result<String, AppError> {
    if source_tab_id == new_tab_id {
        return Err(AppError::Storage(
            "Source and new pane tab IDs must be different".to_string(),
        ));
    }
    if !tabs.iter().any(|tab| tab.id == new_tab_id) {
        return Err(AppError::Storage("New pane tab vanished".to_string()));
    }

    // 先找 source 是否已经是 root（有 paneRoot）。
    let root_idx = tabs
        .iter()
        .position(|tab| tab.id == source_tab_id && tab.pane_root.is_some());

    if let Some(idx) = root_idx {
        let root_tab = &mut tabs[idx];
        let pane_root = root_tab
            .pane_root
            .as_mut()
            .expect("root_idx only matches tabs with pane_root");
        let replacement = crate::services::PaneNode::Split {
            direction: split_direction,
            children: vec![
                crate::services::PaneNode::Leaf {
                    tab_id: source_tab_id.to_string(),
                },
                crate::services::PaneNode::Leaf {
                    tab_id: new_tab_id.to_string(),
                },
            ],
            weights: vec![0.5, 0.5],
        };
        if !pane_root.replace_leaf(source_tab_id, replacement) {
            return Err(AppError::Storage(
                "Source pane is not present in its root layout".to_string(),
            ));
        }
        return Ok(source_tab_id.to_string());
    }

    // source 可能是某个 root 的 leaf。
    if let Some(idx) = tabs.iter().position(|tab| {
        tab.pane_root
            .as_ref()
            .map(|root| root.leaf_tab_ids().iter().any(|id| id == source_tab_id))
            .unwrap_or(false)
    }) {
        let root_tab = &mut tabs[idx];
        let pane_root = root_tab
            .pane_root
            .as_mut()
            .expect("containing root always has pane_root");
        let replacement = crate::services::PaneNode::Split {
            direction: split_direction,
            children: vec![
                crate::services::PaneNode::Leaf {
                    tab_id: source_tab_id.to_string(),
                },
                crate::services::PaneNode::Leaf {
                    tab_id: new_tab_id.to_string(),
                },
            ],
            weights: vec![0.5, 0.5],
        };
        if !pane_root.replace_leaf(source_tab_id, replacement) {
            return Err(AppError::Storage(
                "Source pane disappeared from its root layout".to_string(),
            ));
        }
        return Ok(root_tab.id.clone());
    }

    // source 是独立 tab，变成新的 split root。
    let source_idx = tabs
        .iter()
        .position(|tab| tab.id == source_tab_id)
        .ok_or_else(|| AppError::Storage("Source tab vanished".to_string()))?;
    tabs[source_idx].pane_root = Some(crate::services::PaneNode::Split {
        direction: split_direction,
        children: vec![
            crate::services::PaneNode::Leaf {
                tab_id: source_tab_id.to_string(),
            },
            crate::services::PaneNode::Leaf {
                tab_id: new_tab_id.to_string(),
            },
        ],
        weights: vec![0.5, 0.5],
    });
    Ok(source_tab_id.to_string())
}
