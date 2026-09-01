// Remote path and permission commands.
#[tauri::command]
pub async fn app_open_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
) -> Result<serde_json::Value, AppError> {
    refresh_remote_files(&app, &tab_id, &target_path).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let target_changed = {
        let sessions = state.sessions.read().await;
        sessions
            .get(&tab_id)
            .map(|session| session.shell_cwd.is_none() && session.remote_path != target_path)
            .unwrap_or(false)
    };
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.remote_path = target_path;
        }
    }
    if target_changed {
        state.touch_ai_session_revision(&tab_id).await;
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    enabled: bool,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_local_terminal = state
        .tabs
        .read()
        .await
        .iter()
        .any(|tab| tab.id == tab_id && tab.session_type == "local");
    if is_local_terminal {
        {
            let mut sessions = state.sessions.write().await;
            if let Some(session) = sessions.get_mut(&tab_id) {
                session.follow_shell_cwd = enabled;
                if enabled {
                    if let Some(cwd) = session.shell_cwd.clone() {
                        session.remote_path = cwd;
                    }
                }
            }
        }
        return get_workspace_snapshot(app).await;
    }

    let cwd_to_follow = {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            // Network-device and exec-disabled SSH sessions deliberately do
            // not expose shell integration. Keep this command-side gate in
            // addition to the renderer capability checks so a stale/legacy
            // FileManager request cannot re-enable CWD tracking after the
            // worker has classified the session.
            let effective_enabled = enabled && session.capabilities.shell_integration;
            session.follow_shell_cwd = effective_enabled;
            if effective_enabled
                && session.shell_cwd.as_deref() != Some(session.remote_path.as_str())
            {
                session
                    .shell_cwd
                    .clone()
                    .map(|cwd| (cwd, session.file_access_mode != "root"))
            } else {
                None
            }
        } else {
            None
        }
    };

    // Match Electron's recovery behaviour: enabling follow must immediately
    // catch the file pane up to the most recently reported shell directory.
    // Waiting for another `cd` leaves the toggle active while the pane remains
    // stale forever when the initial listing happened to fail.
    if let Some((cwd, use_sftp_namespace)) = cwd_to_follow {
        match refresh_remote_files_for_shell_cwd(&app, &tab_id, &cwd, use_sftp_namespace).await {
            Ok(resolved_path) => {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    if session.follow_shell_cwd
                        && session.shell_cwd.as_deref() == Some(cwd.as_str())
                    {
                        session.remote_path = resolved_path;
                    }
                }
            }
            Err(error) => {
                // CWD reporting is best-effort in Electron too. A directory
                // the SFTP user cannot read must not make the toggle itself
                // fail or interfere with the interactive terminal.
                crate::services::logging::ssh_debug(
                    &app,
                    &tab_id,
                    format!("CWD follow recovery failed for {cwd}: {error}"),
                );
            }
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_read_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::ReadRemoteFile {
            path: target_path,
            encoding: enc,
            cancellation,
            respond_to: tx,
        }
    })
    .await
}

#[tauri::command]
pub async fn app_write_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::WriteRemoteFile {
            path: target_path.clone(),
            content,
            encoding: enc,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_directory(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::CreateRemoteDirectory {
            parent_path: parent_path.clone(),
            name,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_file(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::CreateRemoteFile {
            parent_path: parent_path.clone(),
            name,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_copy_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::CopyRemotePath {
            target_path,
            destination_path: destination_path.clone(),
            target_type,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_move_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::MoveRemotePath {
            target_path: target_path.clone(),
            destination_path: destination_path.clone(),
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent_src = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let parent_dest = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());

    let _ = refresh_remote_files(&app, &tab_id, &parent_src).await;
    if parent_src != parent_dest {
        let _ = refresh_remote_files(&app, &tab_id, &parent_dest).await;
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_rename_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::RenameRemotePath {
            target_path: target_path.clone(),
            new_name,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_delete_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    target_type: String,
    target_is_symlink: bool,
) -> Result<serde_json::Value, AppError> {
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::DeleteRemotePath {
            target_path: target_path.clone(),
            target_type,
            target_is_symlink,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PermissionApplyTarget {
    All,
    Files,
    Directories,
}

impl PermissionApplyTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Directories => "directories",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePermissionChangeOptions {
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: Option<PermissionApplyTarget>,
}

fn parse_remote_permission_mode(mode: &str) -> Result<u32, AppError> {
    let trimmed = mode.trim();
    if !(3..=4).contains(&trimmed.len())
        || !trimmed
            .chars()
            .all(|character| matches!(character, '0'..='7'))
    {
        return Err(AppError::Command(
            "权限值必须是 3 到 4 位八进制数字，例如 755".to_string(),
        ));
    }
    u32::from_str_radix(trimmed, 8).map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_change_remote_permissions(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    options: RemotePermissionChangeOptions,
) -> Result<serde_json::Value, AppError> {
    let permissions = parse_remote_permission_mode(&options.mode)?;
    let recursive = options.recursive;
    let apply_to = options
        .apply_to
        .unwrap_or(PermissionApplyTarget::All)
        .as_str()
        .to_string();
    send_worker_file_cmd(&app, &tab_id, |tx, cancellation| {
        WorkerCmd::ChangeRemotePermissions {
            target_path: target_path.clone(),
            permissions,
            recursive,
            apply_to,
            cancellation,
            respond_to: tx,
        }
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_remote_file_access_mode(
    app: AppHandle,
    tab_id: String,
    mode: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let sudo_user = options
        .as_ref()
        .and_then(|o| o.get("sudoUser"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let sudo_password = options
        .as_ref()
        .and_then(|o| o.get("sudoPassword"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let root_access_method = options
        .as_ref()
        .and_then(|o| o.get("rootAccessMethod"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let use_saved_password = options
        .as_ref()
        .and_then(|o| o.get("useSavedPassword"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::SetRemoteFileAccessMode {
        mode,
        root_access_method,
        sudo_user,
        sudo_password,
        use_saved_password,
        respond_to: tx,
    })
    .await?;

    get_workspace_snapshot(app).await
}
