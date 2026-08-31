pub fn start_serial_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    crate::services::logging::session(&app, "INFO", "serial", &tab_id, "worker starting");
    tauri::async_runtime::spawn(async move {
        let worker_cancellation = cancellation.clone();
        let should_schedule_reconnect =
            match run_serial_worker(&tab_id, &profile, command_rx, &app, worker_cancellation).await
            {
                Ok(SerialWorkerExit::Requested) => false,
                Ok(SerialWorkerExit::DeviceDisconnected) => {
                    crate::services::logging::session(
                        &app,
                        "WARN",
                        "serial",
                        &tab_id,
                        "worker exited because the device disconnected",
                    );
                    true
                }
                Err(error) if cancellation.is_cancelled() => {
                    crate::services::logging::session(
                        &app,
                        "INFO",
                        "serial",
                        &tab_id,
                        format!("worker canceled: {error}"),
                    );
                    false
                }
                Err(error) => {
                    crate::services::logging::session(
                        &app,
                        "ERROR",
                        "serial",
                        &tab_id,
                        error.message.as_str(),
                    );
                    emit_terminal_data(&app, &tab_id, &format!("\r\n[串口] {error}\r\n")).await;
                    set_terminal_state(
                        &app,
                        &tab_id,
                        format!("串口错误：{error}"),
                        WorkspaceTabStatus::Error,
                    )
                    .await;
                    error.retryable
                }
            };
        if should_schedule_reconnect && !cancellation.is_cancelled() {
            schedule_auto_reconnect(&app, &tab_id, &profile, cancellation).await;
        }
    });
}

async fn run_serial_worker(
    tab_id: &str,
    profile: &Value,
    mut command_rx: mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<SerialWorkerExit, SerialWorkerError> {
    let configured_device_path = profile
        .get("devicePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| SerialWorkerError::fatal("串口设备路径不能为空"))?;
    let device_path = resolve_serial_device(profile, configured_device_path).await?;
    let has_saved_identity = profile
        .get("deviceSerialNumber")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
        || profile
            .get("deviceVendorId")
            .and_then(Value::as_u64)
            .is_some()
        || profile
            .get("deviceProductId")
            .and_then(Value::as_u64)
            .is_some();
    if !has_saved_identity {
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!(
                "serial device is path-bound; select a detected USB port to persist VID/PID/serial (path={configured_device_path})"
            ),
        );
    }
    let baud_rate_value = profile
        .get("baudRate")
        .and_then(Value::as_u64)
        .unwrap_or(115_200);
    let baud_rate = serial_baud_rate(baud_rate_value).map_err(SerialWorkerError::fatal)?;
    let encoding = profile
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-8")
        .to_string();
    let newline_mode = profile
        .get("newlineMode")
        .and_then(Value::as_str)
        .unwrap_or("none")
        .to_string();
    let input_mode = profile
        .get("inputMode")
        .and_then(Value::as_str)
        .unwrap_or("text")
        .to_string();
    let line_mode = profile
        .get("lineMode")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let output_mode = profile
        .get("outputMode")
        .and_then(Value::as_str)
        .unwrap_or("text")
        .to_string();
    validate_serial_modes(&newline_mode, &input_mode, &output_mode)
        .map_err(SerialWorkerError::fatal)?;
    if cancellation.is_cancelled() {
        return Ok(SerialWorkerExit::Requested);
    }
    let local_echo = profile
        .get("localEcho")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pacing = SerialPacing::from_profile(profile).map_err(SerialWorkerError::fatal)?;
    let control_state = SerialControlState::from_profile(profile);
    let serial_parity = parity(
        profile
            .get("parity")
            .and_then(Value::as_str)
            .unwrap_or("none"),
    )
    .map_err(SerialWorkerError::fatal)?;
    let data_bits_value = profile.get("dataBits").and_then(Value::as_u64).unwrap_or(8);
    let stop_bits_value = profile.get("stopBits").and_then(Value::as_u64).unwrap_or(1);
    let data_bits_value_u8 = u8::try_from(data_bits_value)
        .map_err(|_| SerialWorkerError::fatal("串口数据位超出支持范围"))?;
    let stop_bits_value_u8 = u8::try_from(stop_bits_value)
        .map_err(|_| SerialWorkerError::fatal("串口停止位超出支持范围"))?;
    let parity_wire_mode = serial_parity_wire_mode(serial_parity, data_bits_value_u8)
        .map_err(SerialWorkerError::fatal)?;
    let transfer_timing = SerialTransferTiming::from_profile(
        profile,
        baud_rate,
        data_bits_value_u8,
        stop_bits_value_u8,
        !matches!(
            serial_parity,
            crate::sessions::serial::config::SerialParity::None
        ),
    )
    .map_err(SerialWorkerError::fatal)?;
    let transfer_limits =
        SerialTransferLimits::from_profile(profile).map_err(SerialWorkerError::fatal)?;
    let configured_flow_control = profile
        .get("flowControl")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let configured_rs485_mode = profile
        .get("rs485Mode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if configured_flow_control == "hardware" && configured_rs485_mode == "half-duplex" {
        return Err(SerialWorkerError::fatal(
            "串口 RS-485 半双工不能与硬件流控同时启用",
        ));
    }
    let wire_data_bits_value = wire_data_bits(data_bits_value, parity_wire_mode);
    let mut builder = tokio_serial::new(&device_path, baud_rate)
        .data_bits(data_bits(wire_data_bits_value).map_err(SerialWorkerError::fatal)?)
        .stop_bits(stop_bits(stop_bits_value).map_err(SerialWorkerError::fatal)?)
        .parity(serial_parity.tokio_value())
        .flow_control(flow_control(configured_flow_control).map_err(SerialWorkerError::fatal)?);
    if let Some(dtr_on_open) = control_state.dtr {
        builder = builder.dtr_on_open(dtr_on_open);
    }
    let mut stream = builder
        .open_native_async()
        .map_err(|error| SerialWorkerError::retryable(serial_error(&device_path, error)))?;
    if matches!(parity_wire_mode, SerialParityWireMode::Native) {
        if let Err(error) = apply_platform_parity(&stream, serial_parity) {
            close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
            return Err(SerialWorkerError::fatal(error));
        }
    }
    let rs485_delay_before_send = profile
        .get("rs485DelayRtsBeforeSendMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let rs485_delay_after_send = profile
        .get("rs485DelayRtsAfterSendMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if rs485_delay_before_send > 60_000 || rs485_delay_after_send > 60_000 {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Err(SerialWorkerError::fatal(
            "串口 RS-485 RTS 延迟必须在 0 到 60000 毫秒之间",
        ));
    }
    let rs485_mode = match apply_rs485(
        &mut stream,
        configured_rs485_mode,
        profile
            .get("rs485RtsOnSend")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        rs485_delay_before_send as u32,
        rs485_delay_after_send as u32,
    ) {
        Ok(mode) => mode,
        Err(error) => {
            close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
            return Err(SerialWorkerError::fatal(error));
        }
    };
    let mut control_state = control_state;
    if let Err(error) = apply_initial_lines(&mut stream, control_state) {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Err(SerialWorkerError::retryable(serial_error(
            &device_path,
            error,
        )));
    }
    if cancellation.is_cancelled() {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Ok(SerialWorkerExit::Requested);
    }
    let mut stream = SerialIo::new(stream, parity_wire_mode, rs485_mode);
    // Keep the physical-byte logger attached for the whole session. This is
    // important on macOS mark/space emulation, where the logical byte seen by
    // the terminal is not the 8-bit value placed on the wire.
    stream.set_wire_log(crate::services::session_logs::serial_log_sink(app, tab_id).await);
    let (mut reader, mut writer) = tokio::io::split(stream);
    crate::services::logging::session(
        app,
        "INFO",
        "serial",
        tab_id,
        format!("connected baud_rate={baud_rate}"),
    );
    set_terminal_state(
        app,
        tab_id,
        format!("串口 {device_path} @ {baud_rate}"),
        WorkspaceTabStatus::Connected,
    )
    .await;
    if cancellation.is_cancelled() {
        let mut stream = reader.unsplit(writer);
        close_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Ok(SerialWorkerExit::Requested);
    }
    app.state::<crate::services::workspace::WorkspaceState>()
        .serial_reconnect_attempts
        .write()
        .await
        .remove(tab_id);
    emit_terminal_data(app, tab_id, "串口已连接\r\n").await;
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut hex_input_buffer = String::new();
    let mut hex_input_pending_lf = false;
    let mut line_input_buffer = String::new();
    let mut line_input_pending_lf = false;
    let mut text_decoder = SerialTextDecoder::new(&encoding);

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                crate::services::logging::session(app, "INFO", "serial", tab_id, "worker canceled");
                let mut stream = reader.unsplit(writer);
                close_serial_stream(&mut stream, control_state, app, tab_id).await;
                return Ok(SerialWorkerExit::Requested);
            }
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCmd::WriteTerminal(data)) => {
                        let inputs = if input_mode == "hex" {
                            consume_serial_hex_input(
                                &mut hex_input_buffer,
                                &data,
                                &mut hex_input_pending_lf,
                            )
                        } else if line_mode {
                            consume_serial_line_input(
                                &mut line_input_buffer,
                                &data,
                                &mut line_input_pending_lf,
                            )
                        } else {
                            vec![SerialInputChunk::Line {
                                value: data,
                                terminator: String::new(),
                            }]
                        };
                        for input in inputs {
                            let encoded = match input {
                                SerialInputChunk::Immediate(byte) => vec![byte],
                                SerialInputChunk::LineContinuation(terminator) => {
                                    if newline_mode == "none" {
                                        terminator.into_bytes()
                                    } else {
                                        Vec::new()
                                    }
                                }
                                SerialInputChunk::Line { value, terminator } => {
                                    let input = format!("{value}{terminator}");
                                    match encode_serial_input(
                                        &input,
                                        &encoding,
                                        &input_mode,
                                        &newline_mode,
                                    ) {
                                        Ok(encoded) => encoded,
                                        Err(error) if input_mode == "hex" => {
                                            emit_terminal_data(
                                                app,
                                                tab_id,
                                                &format!("\r\n[Hex] {error}\r\n"),
                                            )
                                            .await;
                                            continue;
                                        }
                                        Err(error) => {
                                            close_serial_halves(
                                                reader,
                                                writer,
                                                control_state,
                                                app,
                                                tab_id,
                                            )
                                            .await;
                                            return Err(SerialWorkerError::fatal(error));
                                        }
                                    }
                                }
                            };
                            let written = match write_serial_bytes(
                                &mut writer,
                                &encoded,
                                &cancellation,
                                pacing,
                            )
                            .await
                            {
                                Ok(written) => written,
                                Err(error)
                                    if error.kind() == std::io::ErrorKind::TimedOut =>
                                {
                                    let worker_error = SerialWorkerError::retryable(format!(
                                        "串口 {device_path} 写入等待硬件流控超时，可能已发送部分数据；连接已重置"
                                    ));
                                    emit_terminal_data(
                                        app,
                                        tab_id,
                                        "\r\n[串口] 发送等待 CTS/硬件流控超时，可能已发送部分数据，连接将重置\r\n",
                                    )
                                    .await;
                                    close_serial_halves(
                                        reader,
                                        writer,
                                        control_state,
                                        app,
                                        tab_id,
                                    )
                                    .await;
                                    return Err(worker_error);
                                }
                                Err(error) => {
                                    let worker_error = SerialWorkerError::retryable(
                                        serial_error(&device_path, error),
                                    );
                                    close_serial_halves(
                                        reader,
                                        writer,
                                        control_state,
                                        app,
                                        tab_id,
                                    )
                                    .await;
                                    return Err(worker_error);
                                }
                            };
                            if !written {
                                close_serial_halves(reader, writer, control_state, app, tab_id)
                                    .await;
                                return Ok(SerialWorkerExit::Requested);
                            }
                            crate::services::session_logs::append_serial_bytes(
                                app,
                                tab_id,
                                crate::services::session_logs::SerialLogDirection::Tx,
                                &encoded,
                                None,
                                &encoding,
                            )
                                .await;
                            if local_echo && !encoded.is_empty() {
                                let echoed = match serial_display(&encoded, &encoding, &output_mode)
                                {
                                    Ok(echoed) => echoed,
                                    Err(error) => {
                                        close_serial_halves(
                                            reader,
                                            writer,
                                            control_state,
                                            app,
                                            tab_id,
                                        )
                                        .await;
                                        return Err(SerialWorkerError::fatal(error));
                                    }
                                };
                                emit_terminal_data(app, tab_id, &echoed).await;
                            }
                        }
                    }
                    Some(WorkerCmd::SerialControl {
                        action,
                        value,
                        duration_ms,
                        respond_to,
                    }) => {
                        let mut stream = reader.unsplit(writer);
                        let result = execute_serial_control(
                            stream.serial_mut(),
                            action,
                            value,
                            duration_ms,
                            &mut control_state,
                            &cancellation,
                        )
                        .await;
                        let (next_reader, next_writer) = tokio::io::split(stream);
                        reader = next_reader;
                        writer = next_writer;
                        let _ = respond_to.send(result);
                    }
                    Some(WorkerCmd::SerialTransfer {
                        request,
                        cancellation: transfer_cancellation,
                        respond_to,
                    }) => {
                        if parity_wire_mode.is_emulated() {
                            let _ = respond_to.send(Err(
                                "macOS 标记/空格校验模拟是 7 位数据通道，不能用于二进制串口文件传输，请改用无校验或原生支持的平台"
                                    .to_string(),
                            ));
                            continue;
                        }
                        if configured_flow_control == "software" {
                            let _ = respond_to.send(Err(
                                "软件流控可能吞掉串口文件传输的二进制控制字节，请改用无流控或硬件流控"
                                    .to_string(),
                            ));
                            continue;
                        }
                        let mut reporter = SerialTransferReporter::new(
                            app,
                            tab_id,
                            request.direction,
                            request.mode,
                            &request.local_path,
                            None,
                        );
                        let transfer_log =
                            crate::services::session_logs::serial_log_sink(app, tab_id).await;
                        let mut stream = reader.unsplit(writer);
                        stream.set_wire_log(transfer_log.clone());
                        let result = transfer::execute(
                            &mut stream,
                            request,
                            transfer::TransferContext {
                                timing: transfer_timing,
                                limits: transfer_limits,
                                log_sink: transfer_log,
                                encoding: &encoding,
                                reporter: &mut reporter,
                                cancellation: transfer_cancellation,
                            },
                        )
                        .await;
                        let failure = result.as_ref().err().cloned();
                        let _ = respond_to.send(result);
                        if let Some(error) = failure {
                            close_serial_stream(&mut stream, control_state, app, tab_id).await;
                            return Err(SerialWorkerError::retryable(format!(
                                "串口文件传输失败，连接已重置：{error}"
                            )));
                        }
                        // Transfer logging is temporary. Restore the current
                        // session sink so raw RX/TX logging continues after a
                        // successful file transfer; clearing it here would
                        // silently disable physical-byte logging for the rest
                        // of the tab's lifetime.
                        stream.set_wire_log(
                            crate::services::session_logs::serial_log_sink(app, tab_id).await,
                        );
                        let (next_reader, next_writer) = tokio::io::split(stream);
                        reader = next_reader;
                        writer = next_writer;
                    }
                    Some(WorkerCmd::ResizeTerminal { .. }) => {
                        // Raw serial links have no terminal-size negotiation.
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        crate::services::logging::session(app, "INFO", "serial", tab_id, "disconnecting");
                        let mut stream = reader.unsplit(writer);
                        close_serial_stream(&mut stream, control_state, app, tab_id).await;
                        set_terminal_state(app, tab_id, "串口已断开".to_string(), WorkspaceTabStatus::Closed).await;
                        return Ok(SerialWorkerExit::Requested);
                    }
                    Some(command) => reject_unsupported(command, "Serial 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                if cancellation.is_cancelled() {
                    let mut stream = reader.unsplit(writer);
                    close_serial_stream(&mut stream, control_state, app, tab_id).await;
                    return Ok(SerialWorkerExit::Requested);
                }
                let count = match read {
                    Ok(count) => count,
                    Err(error) => {
                        let worker_error = SerialWorkerError::retryable(serial_error(
                            &device_path,
                            error,
                        ));
                        close_serial_halves(reader, writer, control_state, app, tab_id).await;
                        return Err(worker_error);
                    }
                };
                if count == 0 {
                    if cancellation.is_cancelled() {
                        let mut stream = reader.unsplit(writer);
                        close_serial_stream(&mut stream, control_state, app, tab_id).await;
                        return Ok(SerialWorkerExit::Requested);
                    }
                    crate::services::logging::session(app, "WARN", "serial", tab_id, "device disconnected");
                    let trailing = text_decoder.finish();
                    if !trailing.is_empty() {
                        emit_terminal_data(app, tab_id, &trailing).await;
                    }
                    close_serial_halves(reader, writer, control_state, app, tab_id).await;
                    set_terminal_state(app, tab_id, "串口设备已断开".to_string(), WorkspaceTabStatus::Closed).await;
                    return Ok(SerialWorkerExit::DeviceDisconnected);
                }
                let output = match serial_stream_display(
                    &mut text_decoder,
                    &buffer[..count],
                    &output_mode,
                ) {
                    Ok(output) => output,
                    Err(error) => {
                        close_serial_halves(reader, writer, control_state, app, tab_id).await;
                        return Err(SerialWorkerError::fatal(error));
                    }
                };
                crate::services::session_logs::append_serial_bytes(
                    app,
                    tab_id,
                    crate::services::session_logs::SerialLogDirection::Rx,
                    &buffer[..count],
                    Some(&output),
                    &encoding,
                )
                .await;
                if !output.is_empty() {
                    emit_terminal_data(app, tab_id, &output).await;
                }
            }
        }
    }
}
