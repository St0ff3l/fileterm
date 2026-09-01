// MCP stdio and persistent CLI JSONL runtime.
struct McpStdioJob {
    request: Value,
    id: Value,
    cancellation: Arc<AtomicBool>,
    controls: CliJsonlRequestControls,
}

/// Entry point for `fileterm mcp`. This is deliberately dependency-free: MCP
/// uses newline-delimited JSON-RPC over stdio while the local desktop bridge
/// uses the authenticated socket above.
pub fn run_stdio(arguments: &[String]) -> Result<(), String> {
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("Usage: fileterm mcp\n\nRun the FileTerm MCP server over stdio. FileTerm must be running.");
        return Ok(());
    }

    let stdout = Arc::new(Mutex::new(io::BufWriter::new(io::stdout())));
    let controls = CliJsonlRequestControls::default();
    let (job_sender, job_receiver) =
        std::sync::mpsc::sync_channel::<Option<McpStdioJob>>(MCP_MAX_QUEUED_REQUESTS);
    let job_receiver = Arc::new(Mutex::new(job_receiver));
    let mut workers = Vec::with_capacity(MCP_MAX_CONCURRENT_CLIENTS);

    for index in 0..MCP_MAX_CONCURRENT_CLIENTS {
        let job_receiver = Arc::clone(&job_receiver);
        let stdout = Arc::clone(&stdout);
        let worker = thread::Builder::new()
            .name(format!("fileterm-mcp-stdio-{index}"))
            .spawn(move || loop {
                let job = {
                    let receiver = match job_receiver.lock() {
                        Ok(receiver) => receiver,
                        Err(_) => break,
                    };
                    receiver.recv()
                };
                let Ok(Some(job)) = job else {
                    break;
                };
                process_mcp_stdio_request(job, &stdout);
            })
            .map_err(|error| format!("Unable to start FileTerm MCP worker: {error}"))?;
        workers.push(worker);
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("Unable to read MCP input: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MCP_MAX_MESSAGE_BYTES {
            let response = jsonrpc_error(Value::Null, -32600, "Request exceeds the size limit");
            write_mcp_stdio_value(&stdout, &response)
                .map_err(|error| format!("Unable to write MCP response: {error}"))?;
            continue;
        }
        let request = match serde_json::from_str::<Value>(&line) {
            Ok(request) => request,
            Err(_) => {
                let response = jsonrpc_error(Value::Null, -32700, "Parse error");
                write_mcp_stdio_value(&stdout, &response)
                    .map_err(|error| format!("Unable to write MCP response: {error}"))?;
                continue;
            }
        };
        if let Some(target_id) = mcp_cancel_request_id(&request) {
            // MCP cancellation is a notification: it gets no response of its
            // own. The worker holding the target request emits its normal
            // cancelled result (or observes the closed bridge) independently.
            let _ = controls.cancel(&target_id);
            continue;
        }
        let Some(id) = request
            .get("id")
            .filter(|id| !id.is_null())
            .cloned()
        else {
            // Notifications such as notifications/initialized do not create a
            // worker and must not produce a JSON-RPC response.
            continue;
        };
        let cancellation = match controls.register(&id) {
            Ok(cancellation) => cancellation,
            Err(error) => {
                let response = jsonrpc_error(id, -32600, &error);
                write_mcp_stdio_value(&stdout, &response)
                    .map_err(|error| format!("Unable to write MCP response: {error}"))?;
                continue;
            }
        };
        let job = McpStdioJob {
            request,
            id: id.clone(),
            cancellation,
            controls: controls.clone(),
        };
        match job_sender.try_send(Some(job)) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                controls.remove(&id);
                let response = jsonrpc_error(
                    id,
                    MCP_SERVER_BUSY_ERROR_CODE,
                    "FileTerm MCP request queue is full; retry shortly",
                );
                write_mcp_stdio_value(&stdout, &response).map_err(|error| {
                    format!("Unable to write MCP response: {error}")
                })?;
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                controls.remove(&id);
                return Err("FileTerm MCP workers stopped unexpectedly".to_string());
            }
        }
    }
    controls.cancel_all();
    drop(job_sender);
    for worker in workers {
        worker
            .join()
            .map_err(|_| "FileTerm MCP worker panicked".to_string())?;
    }
    Ok(())
}

fn process_mcp_stdio_request(
    job: McpStdioJob,
    stdout: &Arc<Mutex<io::BufWriter<io::Stdout>>>,
) {
    let McpStdioJob {
        request,
        id,
        cancellation,
        controls,
    } = job;
    let request_id = id.clone();
    let mut on_progress = |progress: &BridgeProgress| {
        if cancellation.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut writer) = stdout.lock() {
            let _ = write_mcp_progress(&mut *writer, progress);
            let _ = writer.flush();
        }
    };
    let response = handle_jsonrpc_request_with_progress_and_cancellation(
        request,
        &mut on_progress,
        Some(&cancellation),
    );
    if let Some(response) = response {
        let _ = write_mcp_stdio_value(stdout, &response);
    }
    controls.remove(&request_id);
}

fn write_mcp_stdio_value(
    stdout: &Arc<Mutex<io::BufWriter<io::Stdout>>>,
    value: &Value,
) -> io::Result<()> {
    let mut writer = stdout
        .lock()
        .map_err(|_| io::Error::other("MCP stdout is unavailable"))?;
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

/// Entry point for the persistent JSONL mode of `fileterm cli`. Unlike the
/// one-shot CLI, this process reads request/response JSONL and keeps a bounded
/// worker pool alive, so an external Agent can send several concurrent actions
/// through one process. Each request still uses the authenticated desktop
/// bridge and the same Rust-side policy evaluator as MCP and one-shot CLI.
pub fn run_cli_jsonl(arguments: &[String]) -> Result<(), String> {
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "Usage: fileterm cli --jsonl\n\nRun the persistent FileTerm CLI JSONL bridge over stdin/stdout. FileTerm must be running.\n\nRequest:\n  {{\"id\":\"request-1\",\"action\":\"list_connections\",\"params\":{{}}}}\n\nCancel a pending request:\n  {{\"id\":\"cancel-1\",\"action\":\"cancel_request\",\"params\":{{\"request_id\":\"request-1\"}}}}\n\nResponse:\n  {{\"id\":\"request-1\",\"ok\":true,\"result\":{{...}}}}\n\nProgress events use the same id and are emitted before the final response. CLI JSONL requests always use the in-app approval policy; the incoming requiresApproval field cannot disable approval. Cancellation stops waiting for the request result, but cannot roll back work already accepted by the desktop app. The process accepts up to {MCP_MAX_CONCURRENT_CLIENTS} concurrent requests and {MCP_MAX_QUEUED_REQUESTS} queued requests; it cancels pending work when stdin closes."
        );
        return Ok(());
    }

    if !arguments.is_empty() {
        return Err(
            "fileterm cli --jsonl accepts no command arguments; use --help for the JSONL contract"
                .to_string(),
        );
    }

    let stdout = Arc::new(Mutex::new(io::BufWriter::new(io::stdout())));
    let controls = CliJsonlRequestControls::default();
    let (job_sender, job_receiver) =
        std::sync::mpsc::sync_channel::<Option<CliJsonlJob>>(MCP_MAX_QUEUED_REQUESTS);
    let job_receiver = Arc::new(Mutex::new(job_receiver));
    let mut workers = Vec::with_capacity(MCP_MAX_CONCURRENT_CLIENTS);

    for index in 0..MCP_MAX_CONCURRENT_CLIENTS {
        let job_receiver = Arc::clone(&job_receiver);
        let stdout = Arc::clone(&stdout);
        let worker = thread::Builder::new()
            .name(format!("fileterm-cli-jsonl-{index}"))
            .spawn(move || loop {
                let job = {
                    let receiver = match job_receiver.lock() {
                        Ok(receiver) => receiver,
                        Err(_) => break,
                    };
                    receiver.recv()
                };
                let Ok(Some(job)) = job else {
                    break;
                };
                process_cli_jsonl_request(job, &stdout);
            })
            .map_err(|error| format!("Unable to start FileTerm CLI JSONL worker: {error}"))?;
        workers.push(worker);
    }

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line =
            line.map_err(|error| format!("Unable to read FileTerm CLI JSONL input: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MCP_MAX_MESSAGE_BYTES {
            write_cli_jsonl_value(
                &stdout,
                &json!({
                    "id": Value::Null,
                    "ok": false,
                    "error": "FileTerm CLI JSONL request exceeds the size limit"
                }),
            )
            .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
            continue;
        }
        let request = match serde_json::from_str::<CliJsonlRequest>(&line) {
            Ok(request) => request,
            Err(_) => {
                write_cli_jsonl_value(
                    &stdout,
                    &json!({
                        "id": Value::Null,
                        "ok": false,
                        "error": "Invalid FileTerm CLI JSONL request"
                    }),
                )
                .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
                continue;
            }
        };
        if let Err(error) = validate_cli_jsonl_request(&request) {
            write_cli_jsonl_value(
                &stdout,
                &json!({ "id": request.id, "ok": false, "error": error }),
            )
            .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
            continue;
        }
        if request.action == "cancel_request" {
            let target_id = match validate_cli_jsonl_cancel_params(&request.params) {
                Ok(target_id) => target_id,
                Err(error) => {
                    write_cli_jsonl_value(
                        &stdout,
                        &json!({ "id": request.id, "ok": false, "error": error }),
                    )
                    .map_err(|error| {
                        format!("Unable to write FileTerm CLI JSONL response: {error}")
                    })?;
                    continue;
                }
            };
            let cancel_request_id = request.id.clone();
            if let Err(error) = controls.register(&cancel_request_id) {
                write_cli_jsonl_value(
                    &stdout,
                    &json!({ "id": request.id, "ok": false, "error": error }),
                )
                .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
                continue;
            }
            let cancelled = controls.cancel(&target_id)?;
            write_cli_jsonl_value(
                &stdout,
                &json!({
                    "id": request.id,
                    "ok": true,
                    "result": { "requestId": target_id, "cancelled": cancelled }
                }),
            )
            .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
            controls.remove(&cancel_request_id);
            continue;
        }

        let request_id = request.id.clone();
        let cancellation = match controls.register(&request.id) {
            Ok(cancellation) => cancellation,
            Err(error) => {
                write_cli_jsonl_value(
                    &stdout,
                    &json!({ "id": request.id, "ok": false, "error": error }),
                )
                .map_err(|error| format!("Unable to write FileTerm CLI JSONL response: {error}"))?;
                continue;
            }
        };
        let job = CliJsonlJob {
            request,
            cancellation,
            controls: controls.clone(),
        };
        match job_sender.try_send(Some(job)) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                controls.remove(&request_id);
                write_cli_jsonl_value(
                    &stdout,
                    &json!({
                        "id": request_id,
                        "ok": false,
                        "error": FILETERM_REQUEST_QUEUE_FULL
                    }),
                )
                .map_err(|error| {
                    format!("Unable to write FileTerm CLI JSONL response: {error}")
                })?;
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                controls.remove(&request_id);
                return Err("FileTerm CLI JSONL workers stopped unexpectedly".to_string());
            }
        }
    }
    // Closing stdin is the JSONL connection lifecycle's disconnect event. Do
    // not wait for a pending bridge timeout (which can be several minutes):
    // propagate cancellation before joining workers so SSH execs, approvals,
    // and bridge reads can clean themselves up promptly.
    controls.cancel_all();
    drop(job_sender);

    for worker in workers {
        worker
            .join()
            .map_err(|_| "FileTerm CLI JSONL worker panicked".to_string())?;
    }
    Ok(())
}

fn validate_cli_jsonl_request(request: &CliJsonlRequest) -> Result<(), String> {
    cli_jsonl_request_key(&request.id)?;
    if request.action.trim().is_empty() || request.action.len() > 256 {
        return Err("FileTerm CLI JSONL request requires a valid action".to_string());
    }
    if !request.params.is_object() {
        return Err("FileTerm CLI JSONL params must be a JSON object".to_string());
    }
    Ok(())
}

fn cli_jsonl_bridge_request(request: &CliJsonlRequest) -> BridgeRequest {
    // Read the compatibility field deliberately, but ignore its value: an
    // external caller cannot opt out of the desktop approval policy.
    let _caller_requested_approval = request.requires_approval;
    BridgeRequest {
        action: request.action.clone(),
        params: request.params.clone(),
        source: WorkspaceSessionSource::Cli,
        // CLI JSONL requests are always subject to the desktop approval policy.
        // Keep the incoming field for wire compatibility, but never trust a
        // caller to turn the approval gate off.
        requires_approval: true,
        progress_token: request.progress_token.clone(),
    }
}

fn process_cli_jsonl_request(job: CliJsonlJob, stdout: &Arc<Mutex<io::BufWriter<io::Stdout>>>) {
    let CliJsonlJob {
        request,
        cancellation,
        controls,
    } = job;
    let id = request.id.clone();
    let request_id = id.clone();
    if cancellation.load(Ordering::Acquire) {
        let _ = write_cli_jsonl_value(
            stdout,
            &json!({ "id": id, "ok": false, "error": FILETERM_CLI_JSONL_REQUEST_CANCELLED }),
        );
        controls.remove(&request_id);
        return;
    }
    let bridge_request = cli_jsonl_bridge_request(&request);
    let mut on_progress = |progress: &BridgeProgress| {
        if cancellation.load(Ordering::Acquire) {
            return;
        }
        let mut value = serde_json::to_value(progress).unwrap_or_else(|_| {
            json!({
                "kind": "progress",
                "event": "request-progress",
                "status": "working",
                "code": "FILETERM_CLI_JSONL_PROGRESS",
                "message": "FileTerm CLI JSONL request is still running"
            })
        });
        if let Some(object) = value.as_object_mut() {
            object.insert("id".to_string(), request_id.clone());
        }
        let _ = write_cli_jsonl_value(stdout, &value);
    };
    let response = match call_desktop_bridge_with_progress_and_cancellation(
        bridge_request,
        &mut on_progress,
        Some(&cancellation),
    ) {
        Ok(result) if !cancellation.load(Ordering::Acquire) => {
            json!({ "id": id, "ok": true, "result": result })
        }
        Err(_) if cancellation.load(Ordering::Acquire) => {
            json!({ "id": id, "ok": false, "error": FILETERM_CLI_JSONL_REQUEST_CANCELLED })
        }
        Ok(_) => json!({ "id": id, "ok": false, "error": FILETERM_CLI_JSONL_REQUEST_CANCELLED }),
        Err(error) => json!({ "id": id, "ok": false, "error": error }),
    };
    let _ = write_cli_jsonl_value(stdout, &response);
    controls.remove(&request_id);
}
