// Desktop-owned loopback runtime listener and bridge response handling.
/// Starts the desktop-owned endpoint and publishes only a per-launch,
/// owner-readable descriptor. The descriptor deliberately contains no
/// connection profile information or credentials.
pub fn start_runtime(app: &AppHandle) -> Result<(), AppError> {
    let path = crate::storage::workspace_file(app, MCP_RUNTIME_FILE)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|error| AppError::Storage(error.to_string()))?;
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|error| {
        AppError::Command(format!("Unable to start FileTerm MCP bridge: {error}"))
    })?;
    listener.set_nonblocking(true).map_err(|error| {
        AppError::Command(format!("Unable to configure FileTerm MCP bridge: {error}"))
    })?;
    let address = listener.local_addr().map_err(|error| {
        AppError::Command(format!("Unable to inspect FileTerm MCP bridge: {error}"))
    })?;
    let descriptor = RuntimeDescriptor {
        protocol_version: MCP_PROTOCOL_VERSION,
        address: address.to_string(),
        token: format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    };
    let content = serde_json::to_vec(&descriptor)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&path, &content)?;

    let app_handle = app.clone();
    let limiter = Arc::new(Semaphore::new(MCP_MAX_CONCURRENT_CLIENTS));
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                crate::services::logging::error(
                    &app_handle,
                    "mcp",
                    format!("unable to activate local MCP bridge: {error}"),
                );
                remove_runtime_descriptor(&app_handle);
                return;
            }
        };
        run_runtime_listener(listener, app_handle, descriptor, limiter).await;
    });
    crate::services::logging::info(app, "mcp", "local MCP bridge started");
    Ok(())
}

/// Removes the per-launch authentication descriptor once the desktop process
/// exits. A stale descriptor is harmless (the random listener is gone), but
/// removing it gives CLI clients an immediate and clear "app is not running"
/// result after a normal quit.
pub fn remove_runtime_descriptor(app: &AppHandle) {
    let Ok(path) = crate::storage::workspace_file(app, MCP_RUNTIME_FILE) else {
        return;
    };
    if let Err(error) = fs::remove_file(path) {
        if error.kind() != io::ErrorKind::NotFound {
            crate::services::logging::warn(app, "mcp", "unable to remove local MCP descriptor");
        }
    }
}

async fn run_runtime_listener(
    listener: TcpListener,
    app: AppHandle,
    descriptor: RuntimeDescriptor,
    limiter: Arc<Semaphore>,
) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            break;
        };
        let app = app.clone();
        let descriptor = descriptor.clone();
        let limiter = limiter.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = limiter.try_acquire_owned() else {
                let _ = write_bridge_response(stream, BridgeResponse::busy()).await;
                return;
            };
            if let Err(error) =
                handle_runtime_connection(stream, peer, app.clone(), descriptor).await
            {
                crate::services::logging::warn(
                    &app,
                    "mcp",
                    format!("local MCP request rejected: {error}"),
                );
            }
        });
    }
}

async fn handle_runtime_connection(
    stream: TcpStream,
    peer: SocketAddr,
    app: AppHandle,
    descriptor: RuntimeDescriptor,
) -> Result<(), String> {
    if !peer.ip().is_loopback() {
        return Err("non-loopback MCP client was rejected".to_string());
    }

    let (reader, mut writer) = stream.into_split();
    let mut reader = AsyncBufReader::new(reader);
    let line = read_bridge_line(&mut reader).await?;
    let envelope: BridgeEnvelope = serde_json::from_str(&line)
        .map_err(|_| "invalid FileTerm MCP bridge request".to_string())?;
    if !bool::from(descriptor.token.as_bytes().ct_eq(envelope.token.as_bytes())) {
        write_bridge_response_to_writer(
            &mut writer,
            BridgeResponse::error("FileTerm MCP authentication failed"),
        )
        .await
        .map_err(|error| error.to_string())?;
        return Err("invalid MCP bridge token".to_string());
    }

    let request_timeout = bridge_request_timeout(&envelope.request);
    let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();
    let dispatch = dispatch_bridge_request(&app, envelope.request, Some(progress_sender));
    tokio::pin!(dispatch);
    let response = match timeout(request_timeout, async {
        let mut progress_open = true;
        loop {
            tokio::select! {
                result = &mut dispatch => {
                    let response = match result {
                        Ok(result) => BridgeResponse::success(result),
                        Err(error) => BridgeResponse::error(error),
                    };
                    while let Ok(progress) = progress_receiver.try_recv() {
                        write_bridge_progress_to_writer(&mut writer, progress)
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    break Ok(response);
                }
                progress = progress_receiver.recv(), if progress_open => {
                    match progress {
                        Some(progress) => write_bridge_progress_to_writer(&mut writer, progress)
                            .await
                            .map_err(|error| error.to_string())?,
                        None => progress_open = false,
                    }
                }
            }
        }
    })
    .await
    {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => return Err(error),
        Err(_) => BridgeResponse::error(
            "FileTerm MCP request timed out; retry after checking the session",
        ),
    };
    write_bridge_response_to_writer(&mut writer, response)
        .await
        .map_err(|error| error.to_string())
}

fn bridge_request_timeout(request: &BridgeRequest) -> Duration {
    if request.action == "wait_for_transfer" {
        // This is a read-only, bounded observation call. Keep its bridge
        // timeout slightly above the public 120-second wait ceiling so the
        // stdio client receives the final task snapshot instead of a socket
        // timeout at the boundary.
        MCP_TRANSFER_WAIT_TIMEOUT
    } else if request.action == "wait_for_connection" {
        MCP_CONNECTION_WAIT_TIMEOUT
    } else if request.action == "open_connection" {
        // MCP approval happens before the connection worker starts. Reserve
        // both windows so an approved SSH profile can still reach its secure
        // credential prompt and return a final connection result.
        ACTION_APPROVAL_TIMEOUT + MCP_CONNECTION_WAIT_TIMEOUT
    } else if request.action == "execute_remote_command"
        || action_requires_approval(&request.action, &request.params)
    {
        // A command may first wait for the external-operation approval, then
        // for a foreground sudo/su password, and finally for the bounded SSH
        // exec itself. The loopback bridge must outlive all three windows or
        // the UI can finish successfully after the MCP caller has already
        // received a socket timeout.
        ACTION_APPROVAL_TIMEOUT
            + PRIVILEGED_PASSWORD_PROMPT_TIMEOUT
            + Duration::from_secs(120)
            + MCP_BRIDGE_TIMEOUT
    } else {
        MCP_BRIDGE_TIMEOUT
    }
}

async fn read_bridge_line(
    reader: &mut AsyncBufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<String, String> {
    let mut line = String::new();
    let count = timeout(MCP_BRIDGE_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| "FileTerm MCP bridge request timed out".to_string())?
        .map_err(|_| "Unable to read FileTerm MCP bridge request".to_string())?;
    if count == 0 {
        return Err("FileTerm MCP bridge client closed without a request".to_string());
    }
    if line.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP bridge request exceeds the size limit".to_string());
    }
    Ok(line)
}

async fn write_bridge_response(stream: TcpStream, response: BridgeResponse) -> io::Result<()> {
    let (_, mut writer) = stream.into_split();
    write_bridge_response_to_writer(&mut writer, response).await
}

async fn write_bridge_response_to_writer(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    response: BridgeResponse,
) -> io::Result<()> {
    let payload =
        serde_json::to_string(&response).map_err(|error| io::Error::other(error.to_string()))?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FileTerm MCP bridge response exceeds the size limit",
        ));
    }
    timeout(MCP_BRIDGE_TIMEOUT, async {
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "FileTerm MCP bridge response timed out",
        )
    })?
}

async fn write_bridge_progress_to_writer(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    progress: BridgeProgress,
) -> io::Result<()> {
    let payload =
        serde_json::to_string(&progress).map_err(|error| io::Error::other(error.to_string()))?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FileTerm MCP bridge progress exceeds the size limit",
        ));
    }
    timeout(MCP_BRIDGE_TIMEOUT, async {
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "FileTerm MCP bridge progress timed out",
        )
    })?
}

impl BridgeResponse {
    fn success(result: Value) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(message.into()),
        }
    }

    fn busy() -> Self {
        Self::error("FileTerm MCP bridge is busy; retry shortly")
    }
}
