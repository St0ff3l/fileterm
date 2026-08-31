// Profile opening, pane layout, and background-session commands.
#[tauri::command]
pub async fn app_open_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;
    let profiles = read_json_array(&app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;

    // Match Electron's open lifecycle: recency is about the user's intent to
    // open a connection, not whether the later network handshake succeeds.
    crate::services::profile_ops::touch_profile(&app, &profile_id)?;

    let tab_id = spawn_session_for_profile(
        &app,
        &state,
        profile,
        &profile_id,
        None,
        None,
        SessionSpawnOptions::default(),
    )
    .await?;

    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(tab_id);
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// Open a saved profile for an external CLI/MCP caller and return the tab id
/// together with the initial workspace snapshot. Background sessions remain
/// attached to the App worker without appearing in the top-level tab bar;
/// callers that need a visible terminal can explicitly attach the session.
/// The operation id is attached before the worker starts, so a fast connection
/// cannot race the wait path.
pub async fn app_open_profile_with_operation(
    app: AppHandle,
    profile_id: String,
    connection_operation_id: String,
    is_background: bool,
    source: crate::services::WorkspaceSessionSource,
) -> Result<(String, serde_json::Value), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;
    let profiles = read_json_array(&app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;

    crate::services::profile_ops::touch_profile(&app, &profile_id)?;
    let tab_id = spawn_session_for_profile(
        &app,
        &state,
        profile,
        &profile_id,
        None,
        Some(&connection_operation_id),
        SessionSpawnOptions {
            is_background,
            source: Some(source),
        },
    )
    .await?;
    let snapshot = get_workspace_snapshot_and_emit(&app).await?;
    Ok((tab_id, snapshot))
}

/// 分屏：基于当前 profile 新建一个独立 session，并在 pane tree 中把 source leaf
/// 替换为 split(source_leaf, new_leaf)。
///
/// - `direction = "row"`：左右分（垂直分屏），新 pane 在右
/// - `direction = "column"`：上下分（水平分屏），新 pane 在下
///
/// 支持 SSH 与 Local Terminal session。两者都会创建独立 PTY / runtime；
/// FTP / Telnet / Serial 暂不支持分屏。
#[tauri::command]
pub async fn app_split_tab(
    app: AppHandle,
    source_tab_id: String,
    direction: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    let split_direction = match direction.as_str() {
        "row" => crate::services::SplitDirection::Row,
        "column" => crate::services::SplitDirection::Column,
        _ => {
            return Err(AppError::Storage(format!(
                "Invalid split direction: {}",
                direction
            )))
        }
    };

    // 找到 source tab，并解析其所属的顶层 workspace tab。分屏 leaf 始终
    // 归属一个 root，而不是第二个顶栏 tab。
    let (profile_id, session_type, pane_root_tab_id) = {
        let tabs = state.tabs.read().await;
        let source = tabs
            .iter()
            .find(|t| t.id == source_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Tab not found: {}", source_tab_id)))?;
        if !supports_split_panes(&source.session_type) {
            return Err(AppError::Storage(format!(
                "Split pane is only supported for SSH and local sessions, got: {}",
                source.session_type
            )));
        }
        let root_tab_id = source.pane_root_tab_id.clone().unwrap_or_else(|| {
            if source.pane_root.is_some() {
                source.id.clone()
            } else {
                tabs.iter()
                    .find(|tab| {
                        tab.pane_root
                            .as_ref()
                            .map(|root| root.leaf_tab_ids().iter().any(|id| id == &source_tab_id))
                            .unwrap_or(false)
                    })
                    .map(|tab| tab.id.clone())
                    .unwrap_or_else(|| source.id.clone())
            }
        });
        if let Some(root) = tabs.iter().find(|tab| tab.id == root_tab_id) {
            if let Some(pane_root) = &root.pane_root {
                let has_mixed_session_types = pane_root.leaf_tab_ids().iter().any(|leaf_tab_id| {
                    tabs.iter()
                        .find(|tab| &tab.id == leaf_tab_id)
                        .map(|tab| tab.session_type != source.session_type)
                        .unwrap_or(true)
                });
                if has_mixed_session_types {
                    return Err(AppError::Storage(
                        "Split pane tree contains incompatible session types".to_string(),
                    ));
                }
            }
        }
        (
            source.profile_id.clone(),
            source.session_type.clone(),
            root_tab_id,
        )
    };

    // 创建新 session（不 touch_profile，分屏不算独立打开）。本地终端复用当前
    // pane 的启动参数及已捕获 CWD，但始终新建 runtime、worker 与 PTY。
    let new_tab_id = match session_type.as_str() {
        "ssh" => {
            let profiles = read_json_array(&app, "profiles.json")?;
            let profile = profiles
                .iter()
                .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
                .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
            spawn_session_for_profile(
                &app,
                &state,
                profile,
                &profile_id,
                Some(pane_root_tab_id),
                None,
                SessionSpawnOptions::default(),
            )
            .await?
        }
        "local" => {
            let mut launch = state
                .local_terminal_launches
                .read()
                .await
                .get(&source_tab_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Storage("Local terminal launch settings are unavailable".to_string())
                })?;
            if let Some(cwd) = state
                .sessions
                .read()
                .await
                .get(&source_tab_id)
                .and_then(|session| session.shell_cwd.clone())
            {
                launch.cwd = cwd;
            }
            spawn_local_terminal_tab(&app, &state, launch, Some(pane_root_tab_id)).await
        }
        _ => unreachable!("session type is checked before creating a split pane"),
    };

    // 更新 paneRoot，并保留承载该分屏树的 root tab id。若 source 在异步
    // 创建新会话期间消失，必须回收刚创建的 worker/PTY，不能留下孤儿会话。
    let root_tab_id = {
        let mut tabs = state.tabs.write().await;
        match attach_split_pane_to_tabs(&mut tabs, &source_tab_id, &new_tab_id, split_direction) {
            Ok(root_tab_id) => {
                let new_tab = tabs
                    .iter_mut()
                    .find(|tab| tab.id == new_tab_id)
                    .expect("attach_split_pane_to_tabs validates the new tab");
                // The source may have been promoted into another root while
                // the new worker was starting. Persist the root that actually
                // accepted the pane instead of the root captured beforehand.
                new_tab.pane_root_tab_id = Some(root_tab_id.clone());
                root_tab_id
            }
            Err(error) => {
                drop(tabs);
                cleanup_unattached_session(&state, &new_tab_id).await;
                return Err(error);
            }
        }
    };

    // 无论当前 source 是 root、leaf 还是独立 tab，顶层都必须停留在 root。
    // 新 session 只作为 active pane，不得成为一个新的顶栏 tab。
    {
        let mut active_tab = state.active_tab_id.write().await;
        *active_tab = Some(root_tab_id.clone());

        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id.clone(), new_tab_id.clone());
    }
    state.touch_ai_session_revision(&source_tab_id).await;
    state.touch_ai_session_revision(&new_tab_id).await;

    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Debug, PartialEq, Eq)]
struct PaneCloseOutcome {
    root_tab_id: String,
    remaining_pane_tab_ids: Vec<String>,
    keeps_split: bool,
}

/// Remove a leaf from a split root while preserving the invariant that a
/// top-level tab is always backed by a live session. When the original root
/// leaf is closed, a surviving leaf is promoted to be the new root rather
/// than leaving a tree whose container points at a closed terminal.
fn remove_split_pane_from_tabs(
    tabs: &mut Vec<crate::services::WorkspaceTab>,
    root_tab_id: &str,
    pane_tab_id: &str,
) -> Result<PaneCloseOutcome, AppError> {
    let root_idx = tabs
        .iter()
        .position(|tab| {
            tab.id == root_tab_id
                || tab
                    .pane_root
                    .as_ref()
                    .map(|r| r.leaf_tab_ids().iter().any(|id| id == pane_tab_id))
                    .unwrap_or(false)
        })
        .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
    let actual_root_tab_id = tabs[root_idx].id.clone();
    let mut next_pane_root = tabs[root_idx]
        .pane_root
        .clone()
        .ok_or_else(|| AppError::Storage("Tab does not have a split pane layout".to_string()))?;
    let before = next_pane_root.leaf_tab_ids();
    if before.len() < 2 {
        return Err(AppError::Storage(
            "Cannot close the only pane through the split-pane command".to_string(),
        ));
    }
    if !before.iter().any(|id| id == pane_tab_id) {
        return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
    }

    next_pane_root.remove_leaf(pane_tab_id);
    let remaining_pane_tab_ids = next_pane_root.leaf_tab_ids();
    let keeps_split = remaining_pane_tab_ids.len() > 1;

    if pane_tab_id != actual_root_tab_id {
        let root_tab = &mut tabs[root_idx];
        root_tab.pane_root = keeps_split.then_some(next_pane_root);
        tabs.retain(|tab| tab.id != pane_tab_id);
        return Ok(PaneCloseOutcome {
            root_tab_id: actual_root_tab_id,
            remaining_pane_tab_ids,
            keeps_split,
        });
    }

    // The original root session is itself a leaf. If it is the pane being
    // closed, turn the first surviving session into the new top-level tab and
    // repoint all remaining leaves at it. This mirrors Ghostty's separation of
    // tab container and terminal surface without requiring us to rename a
    // running SSH worker.
    let promoted_root_tab_id = remaining_pane_tab_ids
        .first()
        .cloned()
        .ok_or_else(|| AppError::Storage("No surviving pane to promote as root tab".to_string()))?;

    let _root_tab = tabs.remove(root_idx);
    let promoted_idx = tabs
        .iter()
        .position(|tab| tab.id == promoted_root_tab_id)
        .ok_or_else(|| {
            AppError::Storage(format!(
                "Promoted pane tab not found: {promoted_root_tab_id}"
            ))
        })?;

    let mut promoted_tab = tabs.remove(promoted_idx);
    promoted_tab.pane_root_tab_id = None;
    promoted_tab.pane_root = keeps_split.then_some(next_pane_root);

    for tab in tabs.iter_mut() {
        if tab.pane_root_tab_id.as_deref() == Some(actual_root_tab_id.as_str()) {
            tab.pane_root_tab_id = Some(promoted_root_tab_id.clone());
        }
    }

    let insert_idx = root_idx.min(tabs.len());
    tabs.insert(insert_idx, promoted_tab);

    Ok(PaneCloseOutcome {
        root_tab_id: promoted_root_tab_id,
        remaining_pane_tab_ids,
        keeps_split,
    })
}

/// 关闭分屏中的单个 pane。若 pane tree 只剩一个 leaf，退化回普通 tab。
#[tauri::command]
pub async fn app_close_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    // Validate before stopping the session. Closing the final pane is a
    // top-level-tab action and must go through its confirmation flow instead.
    let resolved_root_tab_id = {
        let tabs = state.tabs.read().await;
        let root_tab = tabs
            .iter()
            .find(|tab| {
                tab.id == root_tab_id
                    || tab
                        .pane_root
                        .as_ref()
                        .map(|r| r.leaf_tab_ids().iter().any(|id| id == &pane_tab_id))
                        .unwrap_or(false)
            })
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
        let leaves = root_tab
            .pane_root
            .as_ref()
            .map(crate::services::PaneNode::leaf_tab_ids)
            .ok_or_else(|| {
                AppError::Storage("Tab does not have a split pane layout".to_string())
            })?;
        if leaves.len() < 2 {
            return Err(AppError::Storage(
                "Cannot close the only pane through the split-pane command".to_string(),
            ));
        }
        if !leaves.iter().any(|id| id == &pane_tab_id) {
            return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
        }
        root_tab.id.clone()
    };

    let previous_active_pane = state
        .active_pane_tab_id_by_root
        .read()
        .await
        .get(&resolved_root_tab_id)
        .cloned();

    // 暂停该 pane 关联的传输，并清理对应 worker。
    let _ = crate::services::transfers::pause_for_tab(
        &app,
        &pane_tab_id,
        "Pane 关闭后已暂停，可在重连后继续传输",
    )
    .await;
    stop_session_worker(&state, &pane_tab_id).await;
    crate::services::session_logs::stop_for_tab(&state, &pane_tab_id).await;
    state
        .serial_reconnect_attempts
        .write()
        .await
        .remove(&pane_tab_id);

    let outcome = {
        let mut tabs = state.tabs.write().await;
        remove_split_pane_from_tabs(&mut tabs, &resolved_root_tab_id, &pane_tab_id)?
    };

    state.sessions.write().await.remove(&pane_tab_id);
    state.remove_ai_session_revision(&pane_tab_id).await;

    {
        let mut active_tab = state.active_tab_id.write().await;
        if active_tab.as_deref() == Some(root_tab_id.as_str()) {
            *active_tab = Some(outcome.root_tab_id.clone());
        }
    }

    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.remove(&root_tab_id);
        if outcome.keeps_split {
            let next_active_pane = previous_active_pane
                .filter(|id| id != &pane_tab_id && outcome.remaining_pane_tab_ids.contains(id))
                .unwrap_or_else(|| outcome.remaining_pane_tab_ids[0].clone());
            active_panes.insert(outcome.root_tab_id, next_active_pane);
        }
    }
    for remaining_tab_id in &outcome.remaining_pane_tab_ids {
        state.touch_ai_session_revision(remaining_tab_id).await;
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// 设置分屏中的活跃 pane。
#[tauri::command]
pub async fn app_set_active_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let previous_pane_tab_id = {
        let active_panes = state.active_pane_tab_id_by_root.read().await;
        active_panes.get(&root_tab_id).cloned()
    };
    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id, pane_tab_id.clone());
    }
    if previous_pane_tab_id.as_deref() != Some(pane_tab_id.as_str()) {
        if let Some(previous_pane_tab_id) = previous_pane_tab_id {
            state.touch_ai_session_revision(&previous_pane_tab_id).await;
        }
        state.touch_ai_session_revision(&pane_tab_id).await;
    }
    get_workspace_snapshot(app).await
}

/// 持久化分屏 weights（拖拽 resize 结束时调用）。
#[tauri::command]
pub async fn app_set_pane_weights(
    app: AppHandle,
    root_tab_id: String,
    pane_path: Vec<usize>,
    weights: Vec<f32>,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut tabs = state.tabs.write().await;
        let root_idx = tabs
            .iter()
            .position(|t| t.id == root_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_tab_id)))?;
        let root_tab = &mut tabs[root_idx];
        if let Some(ref mut pane_root) = root_tab.pane_root {
            if !pane_root.set_split_weights_at_path(&pane_path, &weights) {
                return Err(AppError::Storage(
                    "Split pane path or weights are invalid".to_string(),
                ));
            }
        } else {
            return Err(AppError::Storage(
                "Tab does not have a split pane layout".to_string(),
            ));
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_activate_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    // A pane leaf owns a session but never owns a top-level tab. Normalizing
    // here makes the invariant hold even for stale UI events.
    let top_level_tab_id = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .and_then(|tab| tab.pane_root_tab_id.clone())
        .unwrap_or(tab_id);
    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(top_level_tab_id);
    }
    get_workspace_snapshot(app).await
}

fn attach_background_session_in_tabs(
    tabs: &mut [crate::services::WorkspaceTab],
    tab_id: &str,
) -> Result<String, AppError> {
    let top_level_tab_id = tabs
        .iter()
        .find(|tab| tab.id == tab_id)
        .map(|tab| {
            tab.pane_root_tab_id
                .clone()
                .unwrap_or_else(|| tab.id.clone())
        })
        .ok_or_else(|| AppError::Storage(format!("Session not found: {tab_id}")))?;
    let tab = tabs
        .iter_mut()
        .find(|tab| tab.id == top_level_tab_id)
        .ok_or_else(|| AppError::Storage(format!("Root session not found: {top_level_tab_id}")))?;
    tab.is_background = false;
    Ok(top_level_tab_id)
}

fn detach_session_to_background_in_tabs(
    tabs: &mut [crate::services::WorkspaceTab],
    tab_id: &str,
) -> Result<String, AppError> {
    let top_level_tab_id = tabs
        .iter()
        .find(|tab| tab.id == tab_id)
        .map(|tab| {
            tab.pane_root_tab_id
                .clone()
                .unwrap_or_else(|| tab.id.clone())
        })
        .ok_or_else(|| AppError::Storage(format!("Session not found: {tab_id}")))?;
    let tab = tabs
        .iter_mut()
        .find(|tab| tab.id == top_level_tab_id)
        .ok_or_else(|| AppError::Storage(format!("Root session not found: {top_level_tab_id}")))?;
    if tab.source.is_none() {
        return Err(AppError::Storage(
            "Only CLI or MCP sessions can be hidden in background".to_string(),
        ));
    }
    tab.is_background = true;
    Ok(top_level_tab_id)
}

fn next_visible_top_level_tab_id(
    tabs: &[crate::services::WorkspaceTab],
    hidden_tab_id: &str,
) -> Option<String> {
    tabs.iter()
        .rev()
        .find(|tab| !tab.is_background && tab.pane_root_tab_id.is_none() && tab.id != hidden_tab_id)
        .map(|tab| tab.id.clone())
}

/// Make an externally opened background session visible in the normal
/// workspace and focus it. The existing worker/session is reused; this only
/// changes presentation and active-tab routing.
#[tauri::command]
pub async fn app_attach_background_session(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let top_level_tab_id = {
        let mut tabs = state.tabs.write().await;
        attach_background_session_in_tabs(&mut tabs, &tab_id)?
    };

    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(top_level_tab_id);
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// Hide an externally opened session from the visible tab bar without
/// disconnecting its existing worker. The session remains available from the
/// Background Sessions page and can be attached again later.
#[tauri::command]
pub async fn app_detach_session_to_background(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let active_tab_id = state.active_tab_id.read().await.clone();
    let (top_level_tab_id, next_active_tab_id) = {
        let mut tabs = state.tabs.write().await;
        let top_level_tab_id = detach_session_to_background_in_tabs(&mut tabs, &tab_id)?;
        let next_active_tab_id = (active_tab_id.as_deref() == Some(top_level_tab_id.as_str()))
            .then(|| next_visible_top_level_tab_id(&tabs, &top_level_tab_id))
            .flatten();
        (top_level_tab_id, next_active_tab_id)
    };

    if active_tab_id.as_deref() == Some(top_level_tab_id.as_str()) {
        let mut active = state.active_tab_id.write().await;
        *active = next_active_tab_id;
    }

    get_workspace_snapshot_and_emit(&app).await
}
