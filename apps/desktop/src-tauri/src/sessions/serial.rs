use serde_json::Value;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, StopBits};

use super::telnet::reject_unsupported;
use super::terminal::{decode_terminal, emit_terminal_data, encode_terminal, set_terminal_state};
use super::WorkerCmd;

pub fn start_serial_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_serial_worker(&tab_id, &profile, command_rx, &app).await {
            crate::services::logging::write(
                &app,
                "ERROR",
                "serial",
                format!("tab={tab_id} {error}"),
            );
            emit_terminal_data(&app, &tab_id, &format!("\r\n[Serial] {error}\r\n")).await;
            set_terminal_state(&app, &tab_id, format!("Serial error: {error}"), false).await;
        }
    });
}

async fn run_serial_worker(
    tab_id: &str,
    profile: &Value,
    mut command_rx: mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
) -> Result<(), String> {
    let device_path = profile
        .get("devicePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Serial device path is required".to_string())?;
    let baud_rate = profile
        .get("baudRate")
        .and_then(Value::as_u64)
        .unwrap_or(115_200) as u32;
    let encoding = profile
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-8")
        .to_string();
    let stream = tokio_serial::new(device_path, baud_rate)
        .data_bits(data_bits(
            profile.get("dataBits").and_then(Value::as_u64).unwrap_or(8),
        )?)
        .stop_bits(stop_bits(
            profile.get("stopBits").and_then(Value::as_u64).unwrap_or(1),
        )?)
        .parity(parity(
            profile
                .get("parity")
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?)
        .flow_control(flow_control(
            profile
                .get("flowControl")
                .and_then(Value::as_str)
                .unwrap_or("none"),
        )?)
        .open_native_async()
        .map_err(|error| serial_error(device_path, error))?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    set_terminal_state(
        app,
        tab_id,
        format!("Serial {device_path} @ {baud_rate}"),
        true,
    )
    .await;
    emit_terminal_data(app, tab_id, "串口已连接\r\n").await;
    let mut buffer = vec![0_u8; 32 * 1024];

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCmd::WriteTerminal(data)) => {
                        writer.write_all(&encode_terminal(&data, &encoding)).await.map_err(|error| error.to_string())?;
                        writer.flush().await.map_err(|error| error.to_string())?;
                    }
                    Some(WorkerCmd::ResizeTerminal { .. }) => {
                        // Raw serial links have no terminal-size negotiation.
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        let _ = writer.shutdown().await;
                        set_terminal_state(app, tab_id, "Serial disconnected".to_string(), false).await;
                        return Ok(());
                    }
                    Some(command) => reject_unsupported(command, "Serial 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                let count = read.map_err(|error| serial_error(device_path, error))?;
                if count == 0 {
                    set_terminal_state(app, tab_id, "Serial device disconnected".to_string(), false).await;
                    return Ok(());
                }
                emit_terminal_data(app, tab_id, &decode_terminal(&buffer[..count], &encoding)).await;
            }
        }
    }
}

fn data_bits(value: u64) -> Result<DataBits, String> {
    match value {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err("Serial data bits must be 5, 6, 7, or 8".to_string()),
    }
}

fn stop_bits(value: u64) -> Result<StopBits, String> {
    match value {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err("Serial stop bits must be 1 or 2".to_string()),
    }
}

fn parity(value: &str) -> Result<Parity, String> {
    match value {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        _ => Err("Serial parity must be none, odd, or even on this platform".to_string()),
    }
}

fn flow_control(value: &str) -> Result<FlowControl, String> {
    match value {
        "none" | "software" => Ok(FlowControl::None),
        "hardware" => Ok(FlowControl::Hardware),
        _ => Err("Serial flow control must be none, software, or hardware".to_string()),
    }
}

fn serial_error(device_path: &str, error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("Permission denied") || message.contains("EACCES") {
        return format!(
            "Cannot open {device_path}: permission denied. On Linux, add this user to dialout."
        );
    }
    if message.contains("No such file") || message.contains("ENOENT") {
        return format!("Serial device {device_path} is unavailable or was disconnected.");
    }
    if message.contains("busy") || message.contains("EBUSY") {
        return format!("Serial device {device_path} is already in use.");
    }
    format!("{device_path}: {message}")
}

#[cfg(test)]
mod tests {
    use super::{data_bits, flow_control, parity, stop_bits};

    #[test]
    fn accepts_core_profile_serial_options() {
        assert!(data_bits(8).is_ok());
        assert!(stop_bits(2).is_ok());
        assert!(parity("even").is_ok());
        assert!(flow_control("hardware").is_ok());
    }
}
