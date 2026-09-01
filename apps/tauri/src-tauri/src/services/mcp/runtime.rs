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
    let connection_limiter = Arc::new(Semaphore::new(MCP_MAX_CONCURRENT_CLIENTS));
    let request_limiter = Arc::new(Semaphore::new(MCP_MAX_CONCURRENT_CLIENTS));
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
        run_runtime_listener(
            listener,
            app_handle,
            descriptor,
            connection_limiter,
            request_limiter,
        )
        .await;
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
    connection_limiter: Arc<Semaphore>,
    request_limiter: Arc<Semaphore>,
) {
    loop {
        let Ok((stream, peer)) = listener.accept().await else {
            break;
        };
        let app = app.clone();
        let descriptor = descriptor.clone();
        let connection_limiter = connection_limiter.clone();
        let request_limiter = request_limiter.clone();
        tauri::async_runtime::spawn(async move {
            let Ok(_permit) = connection_limiter.try_acquire_owned() else {
                let _ = write_bridge_frame(
                    stream,
                    BridgeFrame::HelloAck {
                        protocol_version: MCP_PROTOCOL_VERSION,
                        session_id: String::new(),
                        error: Some("FileTerm MCP bridge is busy; retry shortly".to_string()),
                    },
                )
                .await;
                return;
            };
            if let Err(error) = handle_runtime_connection(
                stream,
                peer,
                app.clone(),
                descriptor,
                request_limiter,
            )
            .await
            {
                crate::services::logging::warn(
                    &app,
                    "mcp",
                    format!("local MCP bridge session ended: {error}"),
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
    request_limiter: Arc<Semaphore>,
) -> Result<(), String> {
    if !peer.ip().is_loopback() {
        return Err("non-loopback MCP client was rejected".to_string());
    }

    let (reader, mut writer) = stream.into_split();
    let mut reader = AsyncBufReader::new(reader);
    let line = read_bridge_line(&mut reader).await?;
    let hello: BridgeFrame = serde_json::from_str(&line)
        .map_err(|_| "invalid FileTerm MCP bridge handshake".to_string())?;
    let (protocol_version, token, client_id) = match hello {
        BridgeFrame::Hello {
            protocol_version,
            token,
            client_id,
        } => (protocol_version, token, client_id),
        _ => {
            write_bridge_frame_to_writer(
                &mut writer,
                BridgeFrame::HelloAck {
                    protocol_version: MCP_PROTOCOL_VERSION,
                    session_id: String::new(),
                    error: Some("FileTerm MCP bridge requires a hello frame".to_string()),
                },
            )
            .await
            .map_err(|error| error.to_string())?;
            return Err("client did not send a bridge hello".to_string());
        }
    };
    if client_id.is_empty() || client_id.len() > MCP_MAX_BRIDGE_REQUEST_ID_BYTES {
        write_bridge_frame_to_writer(
            &mut writer,
            BridgeFrame::HelloAck {
                protocol_version: MCP_PROTOCOL_VERSION,
                session_id: String::new(),
                error: Some("FileTerm MCP bridge client id is invalid".to_string()),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        return Err("invalid bridge client id".to_string());
    }
    if protocol_version != MCP_PROTOCOL_VERSION
        || !bool::from(descriptor.token.as_bytes().ct_eq(token.as_bytes()))
    {
        write_bridge_frame_to_writer(
            &mut writer,
            BridgeFrame::HelloAck {
                protocol_version: MCP_PROTOCOL_VERSION,
                session_id: String::new(),
                error: Some("FileTerm MCP authentication or protocol negotiation failed".to_string()),
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        return Err("invalid MCP bridge hello".to_string());
    }

    let session_id = format!("bridge-{}", uuid::Uuid::new_v4().simple());
    write_bridge_frame_to_writer(
        &mut writer,
        BridgeFrame::HelloAck {
            protocol_version: MCP_PROTOCOL_VERSION,
            session_id,
            error: None,
        },
    )
    .await
    .map_err(|error| error.to_string())?;

    let (writer_sender, mut writer_receiver) =
        mpsc::channel::<BridgeFrame>(MCP_BRIDGE_WRITER_QUEUE_SIZE);
    let writer_task = tauri::async_runtime::spawn(async move {
        while let Some(frame) = writer_receiver.recv().await {
            let close = matches!(frame, BridgeFrame::Close);
            write_bridge_frame_to_writer(&mut writer, frame)
                .await
                .map_err(|error| error.to_string())?;
            if close {
                break;
            }
        }
        Ok::<(), String>(())
    });

    let active_requests = Arc::new(tokio::sync::Mutex::new(HashMap::<
        String,
        CancellationToken,
    >::new()));
    let mut request_tasks = tokio::task::JoinSet::new();
    let session_result = loop {
        let line = match read_bridge_session_line(&mut reader).await {
            Ok(Some(line)) => line,
            Ok(None) => break Ok(()),
            Err(error) => break Err(error),
        };
        let frame: BridgeFrame = match serde_json::from_str(&line) {
            Ok(frame) => frame,
            Err(_) => break Err("invalid FileTerm MCP bridge frame".to_string()),
        };
        match frame {
            BridgeFrame::Request {
                request_id,
                request,
            } => {
                if !valid_bridge_request_id(&request_id) {
                    let _ = writer_sender
                        .send(BridgeFrame::Response {
                            request_id,
                            response: BridgeResponse::error(
                                "FileTerm MCP bridge request id is invalid",
                            ),
                        })
                        .await;
                    continue;
                }
                let request_permit = match request_limiter.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        let _ = writer_sender
                            .send(BridgeFrame::Response {
                                request_id,
                                response: BridgeResponse::busy(),
                            })
                            .await;
                        continue;
                    }
                };
                let cancellation = CancellationToken::new();
                let duplicate = {
                    let mut active = active_requests.lock().await;
                    if active.contains_key(&request_id) {
                        true
                    } else {
                        active.insert(request_id.clone(), cancellation.clone());
                        false
                    }
                };
                if duplicate {
                    let _ = writer_sender
                        .send(BridgeFrame::Response {
                            request_id,
                            response: BridgeResponse::error(
                                "FileTerm MCP bridge request id is already active",
                            ),
                        })
                        .await;
                    continue;
                }
                request_tasks.spawn(run_bridge_request(
                    app.clone(),
                    request_id,
                    request,
                    cancellation,
                    active_requests.clone(),
                    writer_sender.clone(),
                    request_permit,
                ));
            }
            BridgeFrame::Cancel { request_id } => {
                if let Some(cancellation) = active_requests.lock().await.get(&request_id) {
                    cancellation.cancel();
                }
            }
            BridgeFrame::Ping { nonce } => {
                writer_sender
                    .try_send(BridgeFrame::Pong { nonce })
                    .map_err(|_| "FileTerm MCP bridge writer queue is full".to_string())?;
            }
            BridgeFrame::Close => break Ok(()),
            _ => break Err("invalid FileTerm MCP bridge session frame".to_string()),
        }
    };

    for cancellation in active_requests.lock().await.values() {
        cancellation.cancel();
    }
    let drain = async {
        while request_tasks.join_next().await.is_some() {}
    };
    if timeout(MCP_BRIDGE_CANCEL_DRAIN_TIMEOUT, drain).await.is_err() {
        request_tasks.abort_all();
        while request_tasks.join_next().await.is_some() {}
    }
    drop(writer_sender);
    let _ = writer_task.await;
    session_result
}

async fn run_bridge_request(
    app: AppHandle,
    request_id: String,
    request: BridgeRequest,
    cancellation: CancellationToken,
    active_requests: Arc<tokio::sync::Mutex<HashMap<String, CancellationToken>>>,
    writer_sender: mpsc::Sender<BridgeFrame>,
    _request_permit: tokio::sync::OwnedSemaphorePermit,
) {
    let request_timeout = bridge_request_timeout(&request);
    let (progress_sender, mut progress_receiver) = mpsc::unbounded_channel();
    let dispatch = dispatch_bridge_request(
        &app,
        request,
        Some(progress_sender),
        Some(cancellation.clone()),
    );
    tokio::pin!(dispatch);
    let result = timeout(request_timeout, async {
        let mut progress_open = true;
        loop {
            tokio::select! {
                result = &mut dispatch => {
                    return Some(match result {
                        Ok(result) => BridgeResponse::success(result),
                        Err(error) => BridgeResponse::error(error),
                    });
                }
                progress = progress_receiver.recv(), if progress_open => {
                    match progress {
                        Some(progress) => match writer_sender.try_send(BridgeFrame::Progress {
                            request_id: request_id.clone(),
                            progress,
                        }) {
                            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
                            Err(mpsc::error::TrySendError::Closed(_)) => return None,
                        },
                        None => progress_open = false,
                    }
                }
                _ = cancellation.cancelled() => {
                    cancellation.cancel();
                    let _ = timeout(MCP_BRIDGE_CANCEL_DRAIN_TIMEOUT, &mut dispatch).await;
                    return Some(BridgeResponse::error("FileTerm MCP request cancelled"));
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        cancellation.cancel();
        Some(BridgeResponse::error(
            "FileTerm MCP request timed out; retry after checking the session",
        ))
    });

    if let Some(response) = result {
        let _ = writer_sender
            .send(BridgeFrame::Response {
                request_id: request_id.clone(),
                response,
            })
            .await;
    }
    active_requests.lock().await.remove(&request_id);
}

fn valid_bridge_request_id(request_id: &str) -> bool {
    !request_id.is_empty()
        && request_id.len() <= MCP_MAX_BRIDGE_REQUEST_ID_BYTES
        && !request_id.chars().any(char::is_control)
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
    } else if request.action == "read_remote_command" {
        // The public read tool may long-poll for up to 30 seconds while the
        // background SSH channel produces more output. Leave a small margin
        // for serialization and the final loopback write.
        MCP_BRIDGE_TIMEOUT + Duration::from_secs(30)
    } else if request.action == "open_connection" {
        // MCP approval happens before the connection worker starts. Reserve
        // both windows so an approved SSH profile can still reach its secure
        // credential prompt and return a final connection result.
        ACTION_APPROVAL_TIMEOUT + MCP_CONNECTION_WAIT_TIMEOUT
    } else if request.action == "execute_remote_command"
        || request.action == "start_remote_command"
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

async fn read_bridge_session_line(
    reader: &mut AsyncBufReader<tokio::net::tcp::OwnedReadHalf>,
) -> Result<Option<String>, String> {
    let mut line = String::new();
    let count = reader
        .read_line(&mut line)
        .await
        .map_err(|_| "Unable to read FileTerm MCP bridge frame".to_string())?;
    if count == 0 {
        return Ok(None);
    }
    if line.len() > MCP_MAX_MESSAGE_BYTES {
        return Err("FileTerm MCP bridge frame exceeds the size limit".to_string());
    }
    Ok(Some(line))
}

async fn write_bridge_frame(stream: TcpStream, frame: BridgeFrame) -> io::Result<()> {
    let (_, mut writer) = stream.into_split();
    write_bridge_frame_to_writer(&mut writer, frame).await
}

async fn write_bridge_frame_to_writer(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    frame: BridgeFrame,
) -> io::Result<()> {
    let payload =
        serde_json::to_string(&frame).map_err(|error| io::Error::other(error.to_string()))?;
    if payload.len() > MCP_MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "FileTerm MCP bridge frame exceeds the size limit",
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
