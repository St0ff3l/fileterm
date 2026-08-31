pub fn default_launch() -> LocalTerminalLaunch {
    LocalTerminalLaunch {
        shell: default_shell(),
        title: None,
        cwd: default_working_directory(),
        args: Vec::new(),
        env: BTreeMap::new(),
    }
}

pub fn resolve_launch(
    options: Option<LocalTerminalLaunchOptions>,
) -> Result<LocalTerminalLaunch, String> {
    let defaults = default_launch();
    let options = options.unwrap_or_default();
    let launch = LocalTerminalLaunch {
        shell: options.shell.unwrap_or(defaults.shell),
        title: options
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty()),
        cwd: options.cwd.unwrap_or(defaults.cwd),
        args: options.args.unwrap_or_default(),
        env: options.env.unwrap_or_default(),
    };
    validate_launch(&launch)?;
    Ok(launch)
}

#[allow(clippy::too_many_arguments)]
pub fn start_local_terminal_worker(
    tab_id: String,
    runtime_id: String,
    worker_rx: mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: mpsc::UnboundedReceiver<String>,
    app: AppHandle,
    cancellation: CancellationToken,
    launch: LocalTerminalLaunch,
    runtime_gate: Arc<LocalTerminalRuntimeGate>,
) -> Result<oneshot::Receiver<()>, String> {
    crate::services::logging::session(
        &app,
        "INFO",
        "local",
        &tab_id,
        format!(
            "worker starting runtime={} shell={} cwd={} args={} env_entries={}",
            runtime_id,
            launch.shell,
            launch.cwd,
            launch.args.len(),
            launch.env.len()
        ),
    );
    if let Err(error) = validate_launch(&launch) {
        crate::services::logging::error(
            &app,
            "local",
            format!("launch validation failed tab={} error={error}", tab_id),
        );
        return Err(error);
    }
    let cwd = PathBuf::from(&launch.cwd);
    if !cwd.is_dir() {
        let error = format!(
            "Local terminal working directory does not exist: {}",
            launch.cwd
        );
        crate::services::logging::error(
            &app,
            "local",
            format!("launch rejected tab={} error={error}", tab_id),
        );
        return Err(error);
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| {
            let message = format!("Unable to allocate local PTY: {error}");
            crate::services::logging::error(
                &app,
                "local",
                format!("PTY allocation failed tab={} error={message}", tab_id),
            );
            message
        })?;
    let portable_pty::PtyPair { master, slave } = pair;

    let mut command = CommandBuilder::new(&launch.shell);
    command.cwd(cwd);
    for (name, value) in &launch.env {
        command.env(name, value);
    }
    configure_shell_command(&mut command, &launch.shell, &launch.args, &launch.env);

    let mut child = slave.spawn_command(command).map_err(|error| {
        let message = format!("Unable to start local shell {}: {error}", launch.shell);
        crate::services::logging::error(
            &app,
            "local",
            format!("shell spawn failed tab={} error={message}", tab_id),
        );
        message
    })?;
    crate::services::logging::info(
        &app,
        "local",
        format!(
            "shell spawned tab={} runtime={} pid={:?}",
            tab_id,
            runtime_id,
            child.process_id()
        ),
    );
    let process_tree = LocalProcessTree::attach(child.as_ref());
    let reader = master.try_clone_reader().map_err(|error| {
        let message = format!("Unable to read local PTY output: {error}");
        crate::services::logging::error(
            &app,
            "local",
            format!("PTY reader setup failed tab={} error={message}", tab_id),
        );
        message
    })?;
    let writer = match master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = child.kill();
            let message = format!("Unable to write to local PTY: {error}");
            crate::services::logging::error(
                &app,
                "local",
                format!("PTY writer setup failed tab={} error={message}", tab_id),
            );
            return Err(message);
        }
    };

    // Keep PTY reads independent from renderer IPC and transcript locking.
    // The bounded queue prevents a command such as `yes` or a verbose CLI
    // from growing an unbounded native-thread backlog, while the control
    // channel remains available for Ctrl+C and resize commands.
    let (control_tx, control_rx) = std_mpsc::channel::<LocalPtyCommand>();
    let (output_tx, mut output_rx) =
        mpsc::channel::<LocalOutputChunk>(LOCAL_OUTPUT_CHANNEL_CAPACITY);
    let (startup_ready_tx, startup_ready_rx) = oneshot::channel();
    let (output_done_tx, output_done_rx) = tokio::sync::oneshot::channel();
    let pump_app = app.clone();
    let pump_tab_id = tab_id.clone();
    let pump_runtime_id = runtime_id.clone();
    let pump_gate = runtime_gate.clone();
    tauri::async_runtime::spawn(async move {
        let mut cwd_tracker = LocalOsc7CwdTracker::default();
        let mut pending_chunk = None;
        while let Some(first_chunk) = match pending_chunk.take() {
            Some(chunk) => Some(chunk),
            None => output_rx.recv().await,
        } {
            let mut batch = String::new();
            append_local_output_chunk(&mut batch, &first_chunk);
            let deadline = tokio::time::sleep(LOCAL_OUTPUT_BATCH_WINDOW);
            tokio::pin!(deadline);

            while batch.len() < LOCAL_OUTPUT_BATCH_MAX_BYTES {
                tokio::select! {
                    _ = &mut deadline => break,
                    next_chunk = output_rx.recv() => match next_chunk {
                        Some(chunk) => {
                            let previous_len = batch.len();
                            append_local_output_chunk(&mut batch, &chunk);
                            if batch.len() > LOCAL_OUTPUT_BATCH_MAX_BYTES {
                                batch.truncate(previous_len);
                                pending_chunk = Some(chunk);
                                break;
                            }
                        }
                        None => break,
                    },
                }
            }

            if !emit_local_terminal_data(
                &pump_app,
                &pump_tab_id,
                &pump_runtime_id,
                &pump_gate,
                &batch,
            )
            .await
            {
                crate::services::logging::warn(
                    &pump_app,
                    "local",
                    format!(
                        "output publication stopped tab={} runtime={} bytes={}",
                        pump_tab_id,
                        pump_runtime_id,
                        batch.len()
                    ),
                );
                break;
            }
            if let Some(cwd) = cwd_tracker.observe(&batch) {
                let _ = update_local_terminal_cwd(
                    &pump_app,
                    &pump_tab_id,
                    &pump_runtime_id,
                    &pump_gate,
                    cwd,
                )
                .await;
            }
        }
        crate::services::logging::debug(
            &pump_app,
            "local",
            format!(
                "output pump stopped tab={} runtime={}",
                pump_tab_id, pump_runtime_id
            ),
        );
        let _ = output_done_tx.send(());
    });

    let relay_tx = control_tx.clone();
    tauri::async_runtime::spawn(async move {
        forward_terminal_commands(worker_rx, terminal_input_rx, cancellation, relay_tx).await;
    });

    let reader_app = app.clone();
    let reader_tab_id = tab_id.clone();
    let reader_gate = runtime_gate.clone();
    let reader_output_tx = output_tx.clone();
    let reader_control_tx = control_tx.clone();
    thread::Builder::new()
        .name("fileterm-local-pty-reader".to_string())
        .spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 8 * 1024];
            let mut decoder = Utf8StreamDecoder::default();
            let mut query_scanner = LocalTerminalQueryScanner::default();
            let mut output_drop_state = LocalOutputDropState::default();
            let send_query_replies = |replies: Vec<String>| {
                replies.into_iter().all(|reply| {
                    reader_control_tx
                        .send(LocalPtyCommand::Input(reply))
                        .is_ok()
                })
            };
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        crate::services::logging::info(
                            &reader_app,
                            "local",
                            format!("PTY reader reached EOF tab={}", reader_tab_id),
                        );
                        let decoded_tail = decoder.finish();
                        let (tail, replies) = query_scanner.consume(&decoded_tail);
                        let replies_sent = send_query_replies(replies);
                        if !tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                tail,
                                &mut output_drop_state,
                            );
                        }
                        let (pending_tail, _) = query_scanner.finish();
                        if !pending_tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                pending_tail,
                                &mut output_drop_state,
                            );
                        }
                        if !replies_sent {
                            crate::services::logging::debug(
                                &reader_app,
                                "local",
                                format!(
                                    "local terminal query reply channel closed tab={}",
                                    reader_tab_id
                                ),
                            );
                        }
                        flush_local_output_drop_notice(&reader_output_tx, &mut output_drop_state);
                        break;
                    }
                    Ok(size) => {
                        let decoded_chunk = decoder.decode(&buffer[..size]);
                        let (chunk, replies) = query_scanner.consume(&decoded_chunk);
                        let reply_count = replies.len();
                        if !send_query_replies(replies) {
                            crate::services::logging::debug(
                                &reader_app,
                                "local",
                                format!(
                                    "local terminal query reply channel closed tab={}",
                                    reader_tab_id
                                ),
                            );
                            break;
                        }
                        if reply_count > 0 {
                            crate::services::logging::debug(
                                &reader_app,
                                "local",
                                format!(
                                    "automatic terminal query responses tab={} count={}",
                                    reader_tab_id, reply_count
                                ),
                            );
                        }
                        if !chunk.is_empty()
                            && !queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                chunk,
                                &mut output_drop_state,
                            )
                        {
                            crate::services::logging::debug(
                                &reader_app,
                                "local",
                                format!("PTY output queue closed tab={}", reader_tab_id),
                            );
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        crate::services::logging::warn(
                            &reader_app,
                            "local",
                            format!("PTY reader failed tab={} error={error}", reader_tab_id),
                        );
                        let decoded_tail = decoder.finish();
                        let (tail, replies) = query_scanner.consume(&decoded_tail);
                        let replies_sent = send_query_replies(replies);
                        if !tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                tail,
                                &mut output_drop_state,
                            );
                        }
                        let (pending_tail, _) = query_scanner.finish();
                        if !pending_tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                pending_tail,
                                &mut output_drop_state,
                            );
                        }
                        if !replies_sent {
                            crate::services::logging::debug(
                                &reader_app,
                                "local",
                                format!(
                                    "local terminal query reply channel closed tab={}",
                                    reader_tab_id
                                ),
                            );
                        }
                        flush_local_output_drop_notice(&reader_output_tx, &mut output_drop_state);
                        break;
                    }
                }
            }
        })
        .map_err(|error| {
            process_tree.terminate(child.as_mut());
            let message = format!("Unable to start local PTY reader: {error}");
            crate::services::logging::error(
                &app,
                "local",
                format!(
                    "PTY reader thread setup failed tab={} error={message}",
                    tab_id
                ),
            );
            message
        })?;

    let worker_app = app.clone();
    let worker_tab_id = tab_id.clone();
    let worker_runtime_id = runtime_id.clone();
    thread::Builder::new()
        .name("fileterm-local-pty".to_string())
        .spawn(move || {
            let (summary, status) =
                run_pty_loop(control_rx, &mut child, master, writer, &process_tree);
            crate::services::logging::info(
                &worker_app,
                "local",
                format!(
                    "PTY worker finished tab={} runtime={} status={status:?} summary={summary}",
                    worker_tab_id, worker_runtime_id
                ),
            );
            tauri::async_runtime::block_on(async move {
                if tokio::time::timeout(LOCAL_OUTPUT_DRAIN_TIMEOUT, output_done_rx)
                    .await
                    .is_err()
                {
                    crate::services::logging::warn(
                        &worker_app,
                        "local",
                        format!(
                            "output drain timed out tab={} runtime={}",
                            worker_tab_id, worker_runtime_id
                        ),
                    );
                }
                if cleanup_local_terminal_runtime(&worker_app, &worker_tab_id, &worker_runtime_id)
                    .await
                {
                    set_terminal_state(&worker_app, &worker_tab_id, summary, status).await;
                } else {
                    crate::services::logging::debug(
                        &worker_app,
                        "local",
                        format!(
                            "PTY worker state update skipped tab={} runtime={} reason=runtime-replaced",
                            worker_tab_id, worker_runtime_id
                        ),
                    );
                }
            });
        })
        .map_err(|error| {
            let message = format!("Unable to start local PTY worker: {error}");
            crate::services::logging::error(
                &app,
                "local",
                format!("PTY worker thread setup failed tab={} error={message}", tab_id),
            );
            message
        })?;

    crate::services::logging::info(
        &app,
        "local",
        format!(
            "PTY transport ready tab={} runtime={} (reader, worker, and output pump started)",
            tab_id, runtime_id
        ),
    );
    if startup_ready_tx.send(()).is_err() {
        crate::services::logging::debug(
            &app,
            "local",
            format!("startup readiness receiver dropped tab={}", tab_id),
        );
    }

    crate::services::logging::debug(
        &app,
        "local",
        format!(
            "PTY reader and worker threads started tab={} runtime={}",
            tab_id, runtime_id
        ),
    );

    Ok(startup_ready_rx)
}


async fn forward_terminal_commands(
    mut worker_rx: mpsc::Receiver<WorkerCmd>,
    mut terminal_input_rx: mpsc::UnboundedReceiver<String>,
    cancellation: CancellationToken,
    control_tx: std_mpsc::Sender<LocalPtyCommand>,
) {
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = control_tx.send(LocalPtyCommand::Shutdown);
                break;
            }
            input = terminal_input_rx.recv() => match input {
                Some(data) => {
                    if control_tx.send(LocalPtyCommand::Input(data)).is_err() {
                        break;
                    }
                }
                None => {
                    let _ = control_tx.send(LocalPtyCommand::Shutdown);
                    break;
                }
            },
            command = worker_rx.recv() => match command {
                Some(WorkerCmd::ResizeTerminal { cols, rows, width, height }) => {
                    if control_tx.send(LocalPtyCommand::Resize { cols, rows, width, height }).is_err() {
                        break;
                    }
                }
                Some(WorkerCmd::Disconnect) | None => {
                    let _ = control_tx.send(LocalPtyCommand::Shutdown);
                    break;
                }
                Some(_) => {
                    // The local terminal has no remote filesystem, transfer, or tunnel surface.
                }
            }
        }
    }
}

fn run_pty_loop(
    control_rx: std_mpsc::Receiver<LocalPtyCommand>,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    mut writer: Box<dyn Write + Send>,
    process_tree: &LocalProcessTree,
) -> (String, WorkspaceTabStatus) {
    let mut last_size: Option<PtySize> = None;
    loop {
        match control_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
            Ok(LocalPtyCommand::Input(data)) => {
                if let Err(error) = writer
                    .write_all(data.as_bytes())
                    .and_then(|()| writer.flush())
                {
                    process_tree.terminate(child.as_mut());
                    return (
                        format!("Local shell input failed: {error}"),
                        WorkspaceTabStatus::Error,
                    );
                }
            }
            Ok(LocalPtyCommand::Resize {
                cols,
                rows,
                width,
                height,
            }) => {
                let size = PtySize {
                    cols: clamp_u16(cols, DEFAULT_COLS),
                    rows: clamp_u16(rows, DEFAULT_ROWS),
                    pixel_width: clamp_u16(width, 0),
                    pixel_height: clamp_u16(height, 0),
                };
                if last_size != Some(size) {
                    let _ = master.resize(size);
                    last_size = Some(size);
                }
            }
            Ok(LocalPtyCommand::Shutdown) | Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                process_tree.terminate(child.as_mut());
                return (
                    "Local shell stopped".to_string(),
                    WorkspaceTabStatus::Closed,
                );
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                process_tree.terminate(child.as_mut());
                return (
                    local_shell_exit_summary(&status),
                    WorkspaceTabStatus::Closed,
                );
            }
            Ok(None) => {}
            Err(error) => {
                process_tree.terminate(child.as_mut());
                return (
                    format!("Unable to observe local shell: {error}"),
                    WorkspaceTabStatus::Error,
                );
            }
        }
    }
}

fn local_shell_exit_summary(status: &portable_pty::ExitStatus) -> String {
    if status.success() {
        format!("Local shell exited with code {}", status.exit_code())
    } else {
        format!("Local shell exited: {status}")
    }
}

pub async fn deactivate_local_terminal_runtime(state: &WorkspaceState, tab_id: &str) {
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .remove(tab_id);
    let gate = state
        .local_terminal_runtime_gates
        .write()
        .await
        .remove(tab_id);
    if let Some(gate) = gate {
        gate.deactivate().await;
    }
}

async fn cleanup_local_terminal_runtime(app: &AppHandle, tab_id: &str, runtime_id: &str) -> bool {
    let state = app.state::<WorkspaceState>();
    let gate = {
        let mut runtime_ids = state.local_terminal_runtime_ids.write().await;
        if runtime_ids
            .get(tab_id)
            .is_none_or(|current_id| current_id != runtime_id)
        {
            crate::services::logging::debug(
                app,
                "local",
                format!(
                    "runtime cleanup skipped tab={} runtime={} reason=runtime-replaced",
                    tab_id, runtime_id
                ),
            );
            return false;
        }
        runtime_ids.remove(tab_id);
        state
            .local_terminal_runtime_gates
            .write()
            .await
            .remove(tab_id)
    };
    if let Some(gate) = gate {
        gate.deactivate().await;
    }
    state.terminal_inputs.write().await.remove(tab_id);
    state.workers.write().await.remove(tab_id);
    state.worker_controls.write().await.remove(tab_id);
    crate::services::logging::debug(
        app,
        "local",
        format!("runtime cleaned up tab={} runtime={}", tab_id, runtime_id),
    );
    true
}
