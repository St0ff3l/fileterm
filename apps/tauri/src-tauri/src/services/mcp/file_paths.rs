const MCP_PATH_NOT_FOUND: &str = "FILETERM_PATH_NOT_FOUND";
const MCP_RECONNECT_REQUIRED: &str = "FILETERM_RECONNECT_REQUIRED";

fn file_namespace(session_type: &str, mode: &str) -> &'static str {
    match (session_type, mode) {
        ("ssh", "root") => "privileged-shell",
        ("ssh" | "sftp", _) => "sftp",
        _ => "file-view",
    }
}

fn sftp_volume_alias(path: &str, shell_root: Option<&str>) -> Option<String> {
    let shell_root = shell_root?;
    if crate::sessions::ssh::synology_volume_prefix(path) != Some(shell_root)
        || path
            .trim_end_matches('/')
            .split('/')
            .skip(1)
            .any(|part| matches!(part, "" | "." | ".."))
    {
        return None;
    }
    path.strip_prefix(shell_root)
        .filter(|alias| !alias.trim_matches('/').is_empty())
        .map(str::to_owned)
}

fn observed_sftp_volume<'a>(
    session_type: &str,
    mode: &str,
    shell_cwd: Option<&'a str>,
) -> Option<&'a str> {
    if session_type != "ssh" || mode != "user" {
        return None;
    }
    shell_cwd.and_then(crate::sessions::ssh::synology_volume_prefix)
}

struct McpFileView {
    namespace: &'static str,
    shell_root: Option<String>,
    revision: String,
    path: String,
}

async fn mcp_file_view(app: &AppHandle, tab_id: &str) -> Result<McpFileView, String> {
    let reconnect = || {
        format!(
            "{MCP_RECONNECT_REQUIRED}: the file session is unavailable. Refresh session context before reconnecting or opening the saved connection."
        )
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .map(|tab| tab.session_type.clone())
        .ok_or_else(reconnect)?;
    let sessions = state.sessions.read().await;
    let session = sessions
        .get(tab_id)
        .filter(|session| session.connected)
        .ok_or_else(reconnect)?;
    let namespace = file_namespace(&session_type, &session.file_access_mode);
    let shell_root = observed_sftp_volume(
        &session_type,
        &session.file_access_mode,
        session.shell_cwd.as_deref(),
    )
    .map(str::to_owned);
    Ok(McpFileView {
        namespace,
        shell_root,
        revision: session.ai_session_revision.clone(),
        path: session.remote_path.clone(),
    })
}

fn file_lookup_missed(error: &AppError) -> bool {
    match error {
        AppError::Storage(message) => message == crate::sessions::ssh::SFTP_PATH_NOT_FOUND,
        // create_download returns this before creating a transfer.
        AppError::Command(message) => {
            message == crate::services::transfers::DOWNLOAD_REMOTE_FILE_NOT_FOUND
        }
        _ => false,
    }
}

async fn with_mcp_file_path<T, F>(
    app: &AppHandle,
    tab_id: &str,
    path: Option<String>,
    mut operation: impl FnMut(String) -> F,
) -> Result<(T, String, &'static str), String>
where
    F: std::future::Future<Output = Result<T, AppError>>,
{
    let view = mcp_file_view(app, tab_id).await?;
    let alias = path
        .as_deref()
        .and_then(|path| sftp_volume_alias(path, view.shell_root.as_deref()));
    let path = path.unwrap_or_else(|| view.path.clone());
    let mut resolved = path.clone();
    let result = match (operation(path.clone()).await, alias) {
        (Err(error), Some(alias)) if file_lookup_missed(&error) => {
            let current = mcp_file_view(app, tab_id).await?;
            if current.namespace != view.namespace
                || current.revision != view.revision
                || current.shell_root != view.shell_root
            {
                return Err(format!(
                    "{MCP_RECONNECT_REQUIRED}: file session changed. Refresh session context before choosing a new path."
                ));
            }
            resolved = alias;
            operation(resolved.clone()).await
        }
        (result, _) => result,
    };
    match result {
        Ok(value) => Ok((value, resolved, view.namespace)),
        Err(error) if file_lookup_missed(&error) => Err(format!(
            "{MCP_PATH_NOT_FOUND}: {path:?} was not found in the {} namespace (last checked {resolved:?}). Use the path shown in the current file view.",
            view.namespace,
        )),
        Err(AppError::Storage(message)) if message == "Session not found" => Err(format!(
            "{MCP_RECONNECT_REQUIRED}: the file worker is unavailable. Refresh session context before reconnecting."
        )),
        Err(AppError::Command(message)) if message == "传输会话未连接" => Err(format!(
            "{MCP_RECONNECT_REQUIRED}: the transfer worker is unavailable. Refresh session context before reconnecting."
        )),
        Err(error) => Err(public_app_error(error)),
    }
}
