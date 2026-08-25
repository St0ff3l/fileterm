use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::SerialPortBuilderExt;
use tokio_util::sync::CancellationToken;

use super::telnet::reject_unsupported;
use super::terminal::{emit_terminal_data, set_terminal_state};
use super::WorkerCmd;
use crate::services::WorkspaceTabStatus;

mod codec;
mod config;
mod control;
mod kermit;
mod pacing;
mod platform;
mod progress;
mod reconnect;
mod timing;
mod transfer;
mod zmodem;

use self::codec::{
    baud_rate as serial_baud_rate, consume_hex_input as consume_serial_hex_input,
    consume_line_input as consume_serial_line_input, display as serial_display,
    encode_input as encode_serial_input, stream_display as serial_stream_display,
    validate_modes as validate_serial_modes, SerialInputChunk, TextDecoder as SerialTextDecoder,
};
use self::config::{data_bits, flow_control, parity, serial_error, stop_bits};
use self::control::{
    apply_close_lines, apply_initial_lines, execute as execute_serial_control, SerialControlState,
};
use self::pacing::{write_serial_bytes, SerialPacing};
use self::platform::{
    apply_parity as apply_platform_parity, apply_rs485,
    parity_wire_mode as serial_parity_wire_mode, wire_data_bits, SerialIo, SerialParityWireMode,
};
use self::progress::SerialTransferReporter;
use self::reconnect::ReconnectPolicy;
use self::timing::SerialTransferTiming;

enum SerialWorkerExit {
    Requested,
    DeviceDisconnected,
}

#[derive(Debug)]
struct SerialWorkerError {
    message: String,
    retryable: bool,
}

impl SerialWorkerError {
    fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }

    fn retryable(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }
}

impl std::fmt::Display for SerialWorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

async fn resolve_serial_device(
    profile: &Value,
    configured_path: &str,
) -> Result<String, SerialWorkerError> {
    let serial_number = profile
        .get("deviceSerialNumber")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let vendor_id = profile
        .get("deviceVendorId")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let product_id = profile
        .get("deviceProductId")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let port_type = profile
        .get("devicePortType")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty());

    if serial_number.is_none() && vendor_id.is_none() && product_id.is_none() {
        return Ok(configured_path.to_string());
    }

    let scan = tokio::time::timeout(
        Duration::from_secs(3),
        tauri::async_runtime::spawn_blocking(tokio_serial::available_ports),
    )
    .await
    .map_err(|_| SerialWorkerError::retryable("串口设备身份扫描超时".to_string()))?
    .map_err(|error| SerialWorkerError::retryable(format!("串口设备身份扫描失败：{error}")))?
    .map_err(|error| SerialWorkerError::retryable(format!("串口设备身份扫描失败：{error}")))?;

    let configured = scan.iter().find(|port| port.port_name == configured_path);
    if let Some(port) = configured {
        if serial_port_matches_identity(port, serial_number, vendor_id, product_id, port_type) {
            return Ok(configured_path.to_string());
        }
    }

    let matches = scan
        .iter()
        .filter(|port| {
            serial_port_matches_identity(port, serial_number, vendor_id, product_id, port_type)
        })
        .map(|port| port.port_name.clone())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [path] => Ok(path.clone()),
        [] => Err(SerialWorkerError::retryable(format!(
            "串口设备身份未找到匹配端口（当前配置：{configured_path}）"
        ))),
        _ => Err(SerialWorkerError::fatal(format!(
            "串口设备身份匹配到多个端口，请在连接设置中重新选择（{}）",
            matches.join("、")
        ))),
    }
}

async fn close_native_serial_stream(
    stream: &mut tokio_serial::SerialStream,
    state: SerialControlState,
    app: &AppHandle,
    tab_id: &str,
) {
    if let Err(error) = apply_close_lines(stream, state) {
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("close line update failed: {error}"),
        );
    }
    let _ = stream.shutdown().await;
}

async fn close_serial_stream(
    stream: &mut SerialIo,
    state: SerialControlState,
    app: &AppHandle,
    tab_id: &str,
) {
    if let Err(error) = stream.release_rs485() {
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("release RS-485 line failed: {error}"),
        );
    }
    if let Err(error) = apply_close_lines(stream.serial_mut(), state) {
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("close line update failed: {error}"),
        );
    }
    let _ = stream.shutdown().await;
}

async fn close_serial_halves(
    reader: tokio::io::ReadHalf<SerialIo>,
    writer: tokio::io::WriteHalf<SerialIo>,
    state: SerialControlState,
    app: &AppHandle,
    tab_id: &str,
) {
    let mut stream = reader.unsplit(writer);
    close_serial_stream(&mut stream, state, app, tab_id).await;
}

fn serial_port_matches_identity(
    port: &tokio_serial::SerialPortInfo,
    serial_number: Option<&str>,
    vendor_id: Option<u16>,
    product_id: Option<u16>,
    port_type: Option<&str>,
) -> bool {
    if let Some(expected) = port_type {
        let actual = match &port.port_type {
            tokio_serial::SerialPortType::UsbPort(_) => "usb",
            tokio_serial::SerialPortType::PciPort => "pci",
            tokio_serial::SerialPortType::BluetoothPort => "bluetooth",
            tokio_serial::SerialPortType::Unknown => "unknown",
        };
        if actual != expected {
            return false;
        }
    }
    match &port.port_type {
        tokio_serial::SerialPortType::UsbPort(info) => {
            serial_number.is_none_or(|expected| info.serial_number.as_deref() == Some(expected))
                && vendor_id.is_none_or(|expected| info.vid == expected)
                && product_id.is_none_or(|expected| info.pid == expected)
        }
        _ => serial_number.is_none() && vendor_id.is_none() && product_id.is_none(),
    }
}

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
    let stream = SerialIo::new(stream, parity_wire_mode, rs485_mode);
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
                                    emit_terminal_data(
                                        app,
                                        tab_id,
                                        "\r\n[串口] 发送等待 CTS/硬件流控超时，当前数据未继续发送\r\n",
                                    )
                                    .await;
                                    continue;
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
                        let mut reporter = SerialTransferReporter::new(
                            app,
                            tab_id,
                            request.direction,
                            request.mode,
                            &request.local_path,
                            None,
                        );
                        let mut stream = reader.unsplit(writer);
                        let result = transfer::execute(
                            &mut stream,
                            request,
                            transfer_timing,
                            &mut reporter,
                            transfer_cancellation,
                        )
                        .await;
                        let (next_reader, next_writer) = tokio::io::split(stream);
                        reader = next_reader;
                        writer = next_writer;
                        let _ = respond_to.send(result);
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

async fn schedule_auto_reconnect(
    app: &AppHandle,
    tab_id: &str,
    profile: &Value,
    cancellation: CancellationToken,
) {
    let reconnect_mode = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let sessions = state.sessions.read().await;
        sessions
            .get(tab_id)
            .and_then(|session| session.reconnect_mode.clone())
            .or_else(|| crate::services::workspace::reconnect_mode_for_profile(profile))
            .unwrap_or_else(|| "none".to_string())
    };
    if reconnect_mode != "auto" {
        return;
    }

    let policy = ReconnectPolicy::from_profile(profile);
    let attempt = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut attempts = state.serial_reconnect_attempts.write().await;
        let previous = attempts.get(tab_id).copied().unwrap_or_default();
        if let Some(next) = policy.next_attempt(previous) {
            attempts.insert(tab_id.to_string(), next);
            Some(next)
        } else {
            attempts.remove(tab_id);
            None
        }
    };
    let Some(attempt) = attempt else {
        let maximum = policy
            .max_attempts
            .map(|value| value.to_string())
            .unwrap_or_else(|| "∞".to_string());
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("auto-reconnect stopped after reaching max_attempts={maximum}"),
        );
        emit_terminal_data(
            app,
            tab_id,
            &format!("\r\n[串口] 自动重连已达到上限（{maximum} 次）\r\n"),
        )
        .await;
        return;
    };
    let delay = policy.delay_for_attempt(attempt);

    crate::services::logging::session(
        app,
        "INFO",
        "serial",
        tab_id,
        format!(
            "auto-reconnect scheduled attempt={attempt} delay_ms={}",
            delay.as_millis()
        ),
    );
    emit_terminal_data(
        app,
        tab_id,
        &format!(
            "\r\n[串口] 将在 {} 秒后自动重连（第 {attempt} 次）\r\n",
            delay.as_secs()
        ),
    )
    .await;
    tokio::select! {
        _ = tokio::time::sleep(delay) => {}
        _ = cancellation.cancelled() => {
            crate::services::logging::session(
                app,
                "DEBUG",
                "serial",
                tab_id,
                "auto-reconnect canceled by session shutdown",
            );
            return;
        }
    }

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let should_reconnect = {
        let tabs = state.tabs.read().await;
        let sessions = state.sessions.read().await;
        let Some(tab) = tabs.iter().find(|tab| tab.id == tab_id) else {
            return;
        };
        let Some(session) = sessions.get(tab_id) else {
            return;
        };
        let mode = session
            .reconnect_mode
            .as_deref()
            .or_else(|| profile.get("reconnectMode").and_then(Value::as_str))
            .unwrap_or("none");
        !cancellation.is_cancelled()
            && tab.status != WorkspaceTabStatus::Connecting
            && !session.connected
            && mode == "auto"
            && session.summary != "连接已断开"
            && session.summary != "串口已断开"
    };

    if should_reconnect {
        crate::services::logging::session(app, "INFO", "serial", tab_id, "auto-reconnect firing");
        let _ = crate::commands::app_reconnect_tab(app.clone(), tab_id.to_string()).await;
    } else {
        crate::services::logging::session(
            app,
            "DEBUG",
            "serial",
            tab_id,
            "auto-reconnect canceled",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{serial_port_matches_identity, SerialWorkerError};

    #[test]
    fn exposes_retryable_worker_errors_for_reconnect() {
        assert!(!SerialWorkerError::fatal("invalid configuration").retryable);
        assert!(SerialWorkerError::retryable("device unavailable").retryable);
    }

    #[test]
    fn matches_non_usb_identity_by_port_type_without_guessing_a_device() {
        let port = tokio_serial::SerialPortInfo {
            port_name: "/dev/ttyS0".to_string(),
            port_type: tokio_serial::SerialPortType::PciPort,
        };
        assert!(serial_port_matches_identity(
            &port,
            None,
            None,
            None,
            Some("pci")
        ));
        assert!(!serial_port_matches_identity(
            &port,
            None,
            None,
            None,
            Some("usb")
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn virtual_pty_round_trip_exercises_the_real_serial_stack() {
        use std::process::Stdio;

        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;
        use tokio_serial::SerialPortBuilderExt;

        // `openpty` is provided by Python's standard library. Linux's serial
        // backend accepts a PTY as a real serial endpoint, so CI can exercise
        // the complete async read/write lifecycle without a USB device.
        // macOS's backend rejects PTYs with ENOTTY; its acceptance requires an
        // actual /dev/cu.* device (tracked in the release checklist instead of
        // silently pretending a PTY is representative).
        let script = r#"import os, pty, sys
master, slave = pty.openpty()
print(os.ttyname(slave), flush=True)
while True:
    data = os.read(master, 4096)
    if not data:
        break
    os.write(master, b'echo:' + data)
"#;
        let mut child = Command::new("python3")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("python3 must be available for the virtual serial fixture");
        let stdout = child.stdout.take().expect("fixture stdout must be piped");
        let mut lines = BufReader::new(stdout).lines();
        let device_path =
            tokio::time::timeout(std::time::Duration::from_secs(3), lines.next_line())
                .await
                .expect("virtual serial fixture timed out")
                .expect("virtual serial fixture output failed")
                .expect("virtual serial fixture did not provide a device path");

        let stream = tokio_serial::new(&device_path, 115_200)
            .open_native_async()
            .expect("virtual serial device must open");
        let (mut reader, mut writer) = tokio::io::split(stream);
        writer.write_all(b"ping\n").await.unwrap();
        writer.flush().await.unwrap();

        let mut received = Vec::new();
        let mut buffer = [0_u8; 64];
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !received
                .windows(b"echo:ping".len())
                .any(|window| window == b"echo:ping")
            {
                let count = reader.read(&mut buffer).await.unwrap();
                assert!(count > 0, "virtual serial peer closed before echoing data");
                received.extend_from_slice(&buffer[..count]);
            }
        })
        .await
        .expect("virtual serial round trip timed out");

        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
