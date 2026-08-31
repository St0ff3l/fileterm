// Workspace mutation and native window commands.
#[tauri::command]
pub async fn app_workspace_mutation(
    app: AppHandle,
    operation: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    match operation.as_str() {
        "create-profile" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_profile(&app, input)?;
            }
        }
        "create-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_folder(&app, name, parent_id)?;
        }
        "create-command-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建命令分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_command_folder(&app, name, parent_id)?;
        }
        "create-command" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_command_template(&app, input)?;
            }
        }
        _ => {
            return Err(AppError::Command(format!(
                "Unsupported operation: {operation}"
            )))
        }
    }
    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWindowInput {
    pub kind: String,
    pub mode: Option<String>,
    pub profile_id: Option<String>,
    pub command_id: Option<String>,
    pub folder_id: Option<String>,
    pub command: Option<String>,
    pub source: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub tab_id: Option<String>,
    pub encoding: Option<String>,
}

#[tauri::command]
pub async fn app_open_window(app: AppHandle, input: OpenWindowInput) -> Result<(), AppError> {
    // WebView2 can deadlock when WebviewWindowBuilder is used from a
    // synchronous Tauri command on Windows. Keep the command asynchronous and
    // perform the blocking builder call on a worker thread so the native event
    // loop remains able to finish WebView2 initialization and service every
    // other invoke request.
    tauri::async_runtime::spawn_blocking(move || crate::open_child_window(&app, input))
        .await
        .map_err(|error| AppError::Window(format!("子窗口创建任务失败: {error}")))?
}

fn renderer_approved_close_should_destroy(window_label: &str) -> bool {
    window_label != "main"
}

fn destroy_child_window_after_invoke_reply(window: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CHILD_WINDOW_DESTROY_DELAY).await;
        let _ = window.destroy();
    });
}

#[tauri::command]
pub async fn app_window_action(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), AppError> {
    match action.as_str() {
        "show" => {
            if let Err(error) = window.unminimize() {
                crate::services::logging::warn(
                    &app,
                    "window",
                    format!(
                        "unminimize before show failed label={}: {error}",
                        window.label()
                    ),
                );
            }
            window
                .show()
                .map_err(|error| AppError::Window(error.to_string()))?;
            window
                .set_focus()
                .map_err(|error| AppError::Window(error.to_string()))?;
        }
        "minimize" => {
            let _ = window.minimize();
        }
        "toggle-maximize" => {
            if let Ok(true) = window.is_maximized() {
                let _ = window.unmaximize();
            } else {
                let _ = window.maximize();
            }
            let _ = app.emit(
                "app:window-maximized-change",
                window.is_maximized().unwrap_or(false),
            );
        }
        "close" => {
            if !renderer_approved_close_should_destroy(window.label()) {
                // Match Electron: closing the last workspace item requests a
                // normal main-window close. The CloseRequested guard decides
                // whether to hide to tray, quit, or cancel.
                let _ = window.close();
            } else {
                // A child renderer has already approved this close. Destroy it
                // after this command's invoke reply so WebView2 does not try to
                // resolve the callback in an already-destroyed renderer.
                crate::resolve_file_editor_close(&app, &window);
                destroy_child_window_after_invoke_reply(window);
            }
        }
        "hide" => {
            crate::hide_main_window_and_children(&app);
        }
        "request-quit" => {
            crate::request_main_window_close(&app, true);
        }
        "reload" => {
            window
                .reload()
                .map_err(|error| AppError::Window(error.to_string()))?;
        }
        "toggle-devtools" => {
            #[cfg(debug_assertions)]
            {
                if window.is_devtools_open() {
                    window.close_devtools();
                } else {
                    window.open_devtools();
                }
            }
            #[cfg(not(debug_assertions))]
            {
                let _ = window;
            }
        }
        "request-close-window" => crate::request_close_focused_window(&app),
        "quit" => {
            let quit_registry = app.state::<crate::QuitPreparationRegistry>();
            if !quit_registry.try_begin() {
                return Ok(());
            }
            let editors_approved = match crate::request_file_editors_for_quit(&app).await {
                Ok(approved) => approved,
                Err(error) => {
                    quit_registry.cancel();
                    return Err(error);
                }
            };
            if !editors_approved {
                quit_registry.cancel();
                return Ok(());
            }
            // Quit the entire app. Used by the renderer when the user
            // confirms a Cmd+Q / tray-quit request. Persist paused transfer
            // checkpoints before exiting so a restart never silently loses a
            // resumable file.
            if let Err(error) = crate::services::transfers::shutdown(&app).await {
                quit_registry.cancel();
                return Err(error);
            }
            shutdown_session_workers(&app).await;
            // 清除持久化的 home tab UI 状态，确保下次启动只恢复一个默认新建标签页，
            // 而不是上一次退出前残留的所有 home tab。失败仅记录警告，不阻断退出。
            if let Err(error) = app_remove_ui_state_item(app.clone(), "main.tab-ui".to_string()) {
                crate::services::logging::warn(
                    &app,
                    "workspace",
                    format!("failed to clear home tab ui state on quit: {error}"),
                );
            }
            app.exit(0);
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub fn app_is_window_maximized(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

#[tauri::command]
pub fn app_cancel_file_editor_close(app: AppHandle, window: WebviewWindow) {
    crate::cancel_file_editor_close(&app, &window);
}

#[tauri::command]
pub fn app_show_window_menu(
    app: AppHandle,
    window: WebviewWindow,
    menu_type: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let kind = crate::WindowMenuKind::try_from(menu_type.as_str())?;
    crate::show_window_context_menu(&app, &window, kind, x, y)
}

// ==========================================
// Phase 3 commands implementation
// ==========================================

pub(crate) async fn get_workspace_snapshot_unlocked(
    app: AppHandle,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _snapshot_guard = state.workspace_snapshot_lock.lock().await;
    let workspace_revision = state
        .next_workspace_snapshot_revision
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .saturating_add(1);

    let tabs = state.tabs.read().await.clone();
    let active_tab_id = state.active_tab_id.read().await.clone();
    let mut sessions = state.sessions.read().await.clone();
    let ai_session_revisions = state.ai_session_revisions.read().await.clone();
    for (tab_id, session) in &mut sessions {
        session.ai_session_revision = ai_session_revisions
            .get(tab_id)
            .copied()
            .unwrap_or_default()
            .to_string();
    }
    let transfers = state.transfers.read().await.clone();
    let active_pane_tab_id_by_root = state.active_pane_tab_id_by_root.read().await.clone();

    // Read + heal profiles, then strip secrets before exposing in snapshot.
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles: Vec<serde_json::Value> = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect();
    let (command_folders, commands) =
        crate::services::profile_ops::read_and_heal_command_library(&app)?;

    Ok(serde_json::json!({
        "workspaceRevision": workspace_revision,
        "profiles": profiles,
        "folders": folders,
        "commandFolders": command_folders,
        "commandTemplates": commands,
        "tabs": tabs,
        "activeTabId": active_tab_id,
        "transfers": transfers,
        "sessions": sessions,
        "activePaneTabIdByRoot": active_pane_tab_id_by_root,
    }))
}

