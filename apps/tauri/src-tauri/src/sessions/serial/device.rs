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

/// Open and configure a serial device once, then close it without creating a
/// terminal worker. This validates the selected device and all line settings
/// while keeping the test side-effect free for the workspace.
pub async fn test_connection(app: &AppHandle, profile: &Value, tab_id: &str) -> Result<(), String> {
    let configured_device_path = profile
        .get("devicePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "串口设备路径不能为空".to_string())?;
    let device_path = resolve_serial_device(profile, configured_device_path)
        .await
        .map_err(|error| error.to_string())?;
    let baud_rate = serial_baud_rate(
        profile
            .get("baudRate")
            .and_then(Value::as_u64)
            .unwrap_or(115_200),
    )?;
    let serial_parity = parity(
        profile
            .get("parity")
            .and_then(Value::as_str)
            .unwrap_or("none"),
    )?;
    let data_bits_value = profile.get("dataBits").and_then(Value::as_u64).unwrap_or(8);
    let stop_bits_value = profile.get("stopBits").and_then(Value::as_u64).unwrap_or(1);
    let data_bits_value_u8 =
        u8::try_from(data_bits_value).map_err(|_| "串口数据位超出支持范围".to_string())?;
    let parity_wire_mode = serial_parity_wire_mode(serial_parity, data_bits_value_u8)?;
    let configured_flow_control = profile
        .get("flowControl")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let configured_rs485_mode = profile
        .get("rs485Mode")
        .and_then(Value::as_str)
        .unwrap_or("none");
    if configured_flow_control == "hardware" && configured_rs485_mode == "half-duplex" {
        return Err("串口 RS-485 半双工不能与硬件流控同时启用".to_string());
    }

    let control_state = SerialControlState::from_profile(profile);
    let wire_data_bits_value = wire_data_bits(data_bits_value, parity_wire_mode);
    let mut builder = tokio_serial::new(&device_path, baud_rate)
        .data_bits(data_bits(wire_data_bits_value)?)
        .stop_bits(stop_bits(stop_bits_value)?)
        .parity(serial_parity.tokio_value())
        .flow_control(flow_control(configured_flow_control)?);
    if let Some(dtr_on_open) = control_state.dtr {
        builder = builder.dtr_on_open(dtr_on_open);
    }
    let mut stream = builder
        .open_native_async()
        .map_err(|error| serial_error(&device_path, error))?;
    if matches!(parity_wire_mode, SerialParityWireMode::Native) {
        if let Err(error) = apply_platform_parity(&stream, serial_parity) {
            close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
            return Err(error);
        }
    }

    let delay_before_send = profile
        .get("rs485DelayRtsBeforeSendMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let delay_after_send = profile
        .get("rs485DelayRtsAfterSendMs")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if delay_before_send > 60_000 || delay_after_send > 60_000 {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Err("串口 RS-485 RTS 延迟必须在 0 到 60000 毫秒之间".to_string());
    }
    if let Err(error) = apply_rs485(
        &mut stream,
        configured_rs485_mode,
        profile
            .get("rs485RtsOnSend")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        delay_before_send as u32,
        delay_after_send as u32,
    ) {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Err(error);
    }
    if let Err(error) = apply_initial_lines(&mut stream, control_state) {
        close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
        return Err(error);
    }
    close_native_serial_stream(&mut stream, control_state, app, tab_id).await;
    Ok(())
}

async fn close_native_serial_stream(
    stream: &mut tokio_serial::SerialStream,
    state: SerialControlState,
    app: &AppHandle,
    tab_id: &str,
) {
    let shutdown_result = tokio::time::timeout(SERIAL_CLOSE_TIMEOUT, stream.shutdown()).await;
    match shutdown_result {
        Err(error) => crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("serial flush before close timed out: {error}"),
        ),
        Ok(Err(error)) => crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("serial flush before close failed: {error}"),
        ),
        Ok(Ok(())) => {}
    }
    if let Err(error) = apply_close_lines(stream, state) {
        crate::services::logging::session(
            app,
            "WARN",
            "serial",
            tab_id,
            format!("close line update failed: {error}"),
        );
    }
}

async fn close_serial_stream(
    stream: &mut SerialIo,
    state: SerialControlState,
    app: &AppHandle,
    tab_id: &str,
) {
    // SerialIo::poll_shutdown drains the driver queue and releases software
    // RTS only after that drain. Releasing RTS before shutdown can truncate
    // the final byte on macOS software RS-485 adapters. If the bounded drain
    // fails, release is still attempted as a safe fallback before closing.
    let shutdown_result = tokio::time::timeout(SERIAL_CLOSE_TIMEOUT, stream.shutdown()).await;
    match shutdown_result {
        Err(error) => {
            crate::services::logging::session(
                app,
                "WARN",
                "serial",
                tab_id,
                format!("serial flush before close timed out: {error}"),
            );
            if let Err(release_error) = stream.release_rs485() {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "serial",
                    tab_id,
                    format!("release RS-485 line failed: {release_error}"),
                );
            }
        }
        Ok(Err(error)) => {
            crate::services::logging::session(
                app,
                "WARN",
                "serial",
                tab_id,
                format!("serial flush before close failed: {error}"),
            );
            if let Err(release_error) = stream.release_rs485() {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "serial",
                    tab_id,
                    format!("release RS-485 line failed: {release_error}"),
                );
            }
        }
        Ok(Ok(())) => {}
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
