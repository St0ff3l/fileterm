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
    crate::services::logging::session(&app, "INFO", "serial", &tab_id, "worker starting");
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_serial_worker(&tab_id, &profile, command_rx, &app).await {
            crate::services::logging::session(&app, "ERROR", "serial", &tab_id, &error);
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
                        crate::services::logging::session(app, "INFO", "serial", tab_id, "disconnecting");
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
                    crate::services::logging::session(app, "WARN", "serial", tab_id, "device disconnected");
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
        "none" => Ok(FlowControl::None),
        "software" => Ok(FlowControl::Software),
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
    use super::{data_bits, flow_control, parity, stop_bits, FlowControl};

    #[test]
    fn accepts_core_profile_serial_options() {
        assert!(data_bits(8).is_ok());
        assert!(stop_bits(2).is_ok());
        assert!(parity("even").is_ok());
        assert!(flow_control("hardware").is_ok());
        assert_eq!(flow_control("software").unwrap(), FlowControl::Software);
        assert!(parity("mark").is_err());
        assert!(parity("space").is_err());
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
        let device_path = tokio::time::timeout(std::time::Duration::from_secs(3), lines.next_line())
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
            while !received.windows(b"echo:ping".len()).any(|window| window == b"echo:ping") {
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
