async fn set_ftp_state(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    status: WorkspaceTabStatus,
    remote_path: Option<String>,
    remote_files: Option<Vec<Value>>,
) {
    let connected = status.is_connected();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(tab) = state
        .tabs
        .write()
        .await
        .iter_mut()
        .find(|tab| tab.id == tab_id)
    {
        tab.status = status;
    }
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.summary = summary;
        session.connected = connected;
        if connected {
            session.remote_capabilities = Some(default_ftp_capabilities());
        } else {
            // A reconnecting/error tab must not keep advertising the previous
            // server's extensions or showing its stale directory snapshot.
            // The next successful connection repopulates both atomically
            // before the capability panel is rendered again.
            session.remote_capabilities = None;
            session.remote_files.clear();
            session.remote_files_loading = false;
        }
        if let Some(path) = remote_path {
            session.remote_path = path;
        }
        if let Some(files) = remote_files {
            session.remote_files = files;
        }
    }
    let operation_state = if connected {
        crate::services::connection_operations::ConnectionOperationState::Connected
    } else {
        crate::services::connection_operations::ConnectionOperationState::Failed {
            code: crate::services::connection_operations::FILETERM_CONNECTION_FAILED.to_string(),
        }
    };
    state
        .connection_operations
        .publish_for_tab(tab_id, operation_state)
        .await;
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

async fn set_ftp_capabilities(app: &AppHandle, tab_id: &str, capabilities: RemoteFileCapabilities) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.remote_capabilities = Some(capabilities);
    }
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

macro_rules! ftp_match {
    ($client:expr, $ftp:ident => $operation:expr) => {
        match $client {
            FtpClient::Plain($ftp) => $operation.await,
            FtpClient::Secure($ftp) => $operation.await,
        }
    };
}

async fn client_noop(client: &mut FtpClient) -> Result<(), String> {
    ftp_match!(client, ftp => ftp.noop()).map_err(|error| error.to_string())
}

async fn client_features(
    client: &mut FtpClient,
) -> Result<HashMap<String, Option<String>>, String> {
    ftp_match!(client, ftp => ftp.feat()).map_err(|error| error.to_string())
}

async fn client_custom_command(client: &mut FtpClient, command: &str) -> Result<String, String> {
    ftp_match!(client, ftp => ftp.custom_command(
        command,
        &[Status::File, Status::CommandOk, Status::RequestedFileActionOk],
    ))
    .map(|response| String::from_utf8_lossy(&response.body).into_owned())
    .map_err(|error| error.to_string())
}

fn ftp_capabilities_from_features(
    features: HashMap<String, Option<String>>,
) -> RemoteFileCapabilities {
    let mut capabilities = default_ftp_capabilities();
    let mut checksum_algorithms = Vec::new();
    for (name, value) in features {
        let name = name.trim().to_ascii_uppercase();
        if name.is_empty() {
            continue;
        }
        capabilities.extensions.push(name.clone());
        let value = value.unwrap_or_default().to_ascii_uppercase();
        let feature_text = format!("{name} {value}");
        for (needle, label) in [
            ("SHA-256", "SHA-256"),
            ("SHA256", "SHA-256"),
            ("SHA-1", "SHA-1"),
            ("SHA1", "SHA-1"),
            ("MD5", "MD5"),
            ("CRC", "CRC"),
        ] {
            if feature_text.contains(needle)
                && !checksum_algorithms.iter().any(|item| item == label)
            {
                checksum_algorithms.push(label.to_string());
            }
        }
    }
    capabilities.extensions.sort();
    capabilities.extensions.dedup();
    checksum_algorithms.sort();
    capabilities.checksum_algorithms = checksum_algorithms;
    capabilities
}

fn ftp_sha256_command(features: &HashMap<String, Option<String>>) -> Option<String> {
    for (name, value) in features {
        let name = name.trim().to_ascii_uppercase();
        let value = value.as_deref().unwrap_or("").to_ascii_uppercase();
        if name == "HASH" && (value.contains("SHA-256") || value.contains("SHA256")) {
            return Some("HASH".to_string());
        }
        if matches!(name.as_str(), "XSHA256" | "XSHA-256" | "SHA256") {
            return Some(name);
        }
    }
    None
}

fn parse_ftp_sha256_response(response: &str) -> Option<String> {
    response
        .split_whitespace()
        .rev()
        .find(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

fn ftp_hash_requires_algorithm_selection(command: &str) -> bool {
    command.eq_ignore_ascii_case("HASH")
}

async fn client_select_hash_sha256(client: &mut FtpClient) -> Result<(), String> {
    // The standardized HASH command uses the server's currently selected
    // algorithm. Select SHA-256 explicitly before every checksum request so
    // a server whose default is SHA-1/MD5 cannot be mistaken for SHA-256.
    ftp_match!(client, ftp => ftp.opts("HASH", Some("SHA-256"))).map_err(|error| error.to_string())
}

async fn client_sha256(
    client: &mut FtpClient,
    command: &str,
    remote_path: &str,
) -> Result<String, String> {
    if remote_path.is_empty()
        || remote_path.len() > 4096
        || remote_path.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err("FTP remote path contains invalid command characters".to_string());
    }
    if ftp_hash_requires_algorithm_selection(command) {
        client_select_hash_sha256(client).await?;
    }
    let response = client_custom_command(client, &format!("{command} {remote_path}")).await?;
    parse_ftp_sha256_response(&response)
        .ok_or_else(|| "FTP server returned no recognizable SHA-256 checksum".to_string())
}

async fn verify_ftp_transfer_checksum(
    client: &mut FtpClient,
    local_path: &str,
    remote_path: &str,
    io_timeout: Duration,
) -> Result<(), String> {
    let features = ftp_io_with_timeout(
        io_timeout,
        "read FTP checksum features",
        client_features(client),
    )
    .await?;
    let Some(command) = ftp_sha256_command(&features) else {
        return Ok(());
    };
    let local_hash = crate::sessions::file_integrity::sha256_file(local_path).await?;
    let remote_hash = ftp_io_with_timeout(
        io_timeout,
        "read FTP remote checksum",
        client_sha256(client, &command, remote_path),
    )
    .await?;
    if local_hash != remote_hash {
        return Err(format!(
            "FTP transfer checksum mismatch: local {local_hash}, remote {remote_hash}"
        ));
    }
    Ok(())
}

async fn client_list(
    client: &mut FtpClient,
    path: &str,
    state: &mut FtpListingState,
) -> Result<Vec<Value>, String> {
    ftp_match!(client, ftp => list_files_with_state(ftp, path, state))
}

async fn client_read(client: &mut FtpClient, path: &str, encoding: &str) -> Result<String, String> {
    ftp_match!(client, ftp => read_file(ftp, path, encoding))
}

async fn client_write(
    client: &mut FtpClient,
    path: &str,
    content: &str,
    encoding: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => write_file(ftp, path, content, encoding))
}

async fn client_ensure_dir(client: &mut FtpClient, path: &str) -> Result<(), String> {
    ftp_match!(client, ftp => ensure_dir(ftp, path))
}

async fn client_rename(
    client: &mut FtpClient,
    source: &str,
    destination: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => rename_file(ftp, source, destination))
}

async fn client_delete(
    client: &mut FtpClient,
    path: &str,
    target_type: &str,
    target_is_symlink: bool,
) -> Result<(), String> {
    let mut visited = HashSet::new();
    let mut entries = 0;
    match client {
        FtpClient::Plain(ftp) => {
            delete_path(
                ftp,
                path,
                target_type,
                target_is_symlink,
                0,
                &mut visited,
                &mut entries,
            )
            .await
        }
        FtpClient::Secure(ftp) => {
            delete_path(
                ftp,
                path,
                target_type,
                target_is_symlink,
                0,
                &mut visited,
                &mut entries,
            )
            .await
        }
    }
}

async fn client_chmod(client: &mut FtpClient, path: &str, permissions: u32) -> Result<(), String> {
    let mode = format!("{:o}", permissions & 0o7777);
    ftp_match!(client, ftp => chmod_file(ftp, path, &mode))
}

async fn client_stat(
    client: &mut FtpClient,
    path: &str,
) -> Result<Option<TransferFileStat>, String> {
    ftp_match!(client, ftp => stat_file(ftp, path))
}

#[allow(clippy::too_many_arguments)] // Transfer state and its response channel are kept explicit at the worker boundary.
async fn client_upload(
    client: &mut FtpClient,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    ftp_match!(client, ftp => upload_file(ftp, local_path, remote_path, resume_offset, transfer_id, cancel, Some(app), io_timeout))
}

#[allow(clippy::too_many_arguments)] // Transfer state and its response channel are kept explicit at the worker boundary.
async fn client_download(
    client: &mut FtpClient,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    ftp_match!(client, ftp => download_file(ftp, remote_path, local_path, resume_offset, transfer_id, cancel, app, io_timeout))
}

async fn client_replace(
    client: &mut FtpClient,
    partial: &str,
    destination: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => replace_file(ftp, partial, destination))
}

async fn client_remove(client: &mut FtpClient, path: &str) -> Result<(), String> {
    ftp_match!(client, ftp => remove_file(ftp, path))
}

async fn client_quit(client: &mut FtpClient) -> Result<(), String> {
    ftp_match!(client, ftp => quit(ftp))
}
