use serde_json::Value;
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
mod pacing;
mod reconnect;
mod transfer;

use self::codec::{
    baud_rate as serial_baud_rate, consume_hex_input as consume_serial_hex_input,
    consume_line_input as consume_serial_line_input, display as serial_display,
    encode_input as encode_serial_input, stream_display as serial_stream_display,
    validate_modes as validate_serial_modes, TextDecoder as SerialTextDecoder,
};
use self::config::{data_bits, flow_control, parity, serial_error, stop_bits};
use self::control::{apply_initial_lines, execute as execute_serial_control, SerialControlState};
use self::pacing::{write_serial_bytes, SerialPacing};
use self::reconnect::ReconnectPolicy;

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
    let device_path = profile
        .get("devicePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| SerialWorkerError::fatal("串口设备路径不能为空"))?;
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
    let mut builder = tokio_serial::new(device_path, baud_rate)
        .data_bits(
            data_bits(profile.get("dataBits").and_then(Value::as_u64).unwrap_or(8))
                .map_err(SerialWorkerError::fatal)?,
        )
        .stop_bits(
            stop_bits(profile.get("stopBits").and_then(Value::as_u64).unwrap_or(1))
                .map_err(SerialWorkerError::fatal)?,
        )
        .parity(
            parity(
                profile
                    .get("parity")
                    .and_then(Value::as_str)
                    .unwrap_or("none"),
            )
            .map_err(SerialWorkerError::fatal)?,
        )
        .flow_control(
            flow_control(
                profile
                    .get("flowControl")
                    .and_then(Value::as_str)
                    .unwrap_or("none"),
            )
            .map_err(SerialWorkerError::fatal)?,
        );
    if let Some(dtr_on_open) = control_state.dtr {
        builder = builder.dtr_on_open(dtr_on_open);
    }
    let mut stream = builder
        .open_native_async()
        .map_err(|error| SerialWorkerError::retryable(serial_error(device_path, error)))?;
    app.state::<crate::services::workspace::WorkspaceState>()
        .serial_reconnect_attempts
        .write()
        .await
        .remove(tab_id);
    let mut control_state = control_state;
    apply_initial_lines(&mut stream, control_state)
        .map_err(|error| SerialWorkerError::retryable(serial_error(device_path, error)))?;
    if cancellation.is_cancelled() {
        return Ok(SerialWorkerExit::Requested);
    }
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
        return Ok(SerialWorkerExit::Requested);
    }
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
                            .into_iter()
                            .map(|line| format!("{line}\n"))
                            .collect::<Vec<_>>()
                        } else if line_mode {
                            consume_serial_line_input(
                                &mut line_input_buffer,
                                &data,
                                &mut line_input_pending_lf,
                            )
                            .into_iter()
                            .map(|line| format!("{line}\n"))
                            .collect::<Vec<_>>()
                        } else {
                            vec![data]
                        };
                        for input in inputs {
                            let encoded = match encode_serial_input(
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
                                Err(error) => return Err(SerialWorkerError::fatal(error)),
                            };
                            let written = write_serial_bytes(
                                &mut writer,
                                &encoded,
                                &cancellation,
                                pacing,
                            )
                                .await
                                .map_err(|error| {
                                    SerialWorkerError::retryable(serial_error(device_path, error))
                                })?;
                            if !written {
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
                                let echoed = serial_display(&encoded, &encoding, &output_mode)
                                    .map_err(SerialWorkerError::fatal)?;
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
                            &mut stream,
                            action,
                            value,
                            duration_ms,
                            &mut control_state,
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
                        let mut stream = reader.unsplit(writer);
                        let result = transfer::execute(&mut stream, request, transfer_cancellation).await;
                        let (next_reader, next_writer) = tokio::io::split(stream);
                        reader = next_reader;
                        writer = next_writer;
                        let _ = respond_to.send(result);
                    }
                    Some(WorkerCmd::ResizeTerminal { .. }) => {
                        // Raw serial links have no terminal-size negotiation.
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        if cancellation.is_cancelled() {
                            return Ok(SerialWorkerExit::Requested);
                        }
                        crate::services::logging::session(app, "INFO", "serial", tab_id, "disconnecting");
                        let _ = writer.shutdown().await;
                        set_terminal_state(app, tab_id, "串口已断开".to_string(), WorkspaceTabStatus::Closed).await;
                        return Ok(SerialWorkerExit::Requested);
                    }
                    Some(command) => reject_unsupported(command, "Serial 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                if cancellation.is_cancelled() {
                    return Ok(SerialWorkerExit::Requested);
                }
                let count = read
                    .map_err(|error| SerialWorkerError::retryable(serial_error(device_path, error)))?;
                if count == 0 {
                    if cancellation.is_cancelled() {
                        return Ok(SerialWorkerExit::Requested);
                    }
                    crate::services::logging::session(app, "WARN", "serial", tab_id, "device disconnected");
                    let trailing = text_decoder.finish();
                    if !trailing.is_empty() {
                        emit_terminal_data(app, tab_id, &trailing).await;
                    }
                    set_terminal_state(app, tab_id, "串口设备已断开".to_string(), WorkspaceTabStatus::Closed).await;
                    return Ok(SerialWorkerExit::DeviceDisconnected);
                }
                let output = serial_stream_display(&mut text_decoder, &buffer[..count], &output_mode)
                    .map_err(SerialWorkerError::fatal)?;
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
    use super::SerialWorkerError;

    #[test]
    fn exposes_retryable_worker_errors_for_reconnect() {
        assert!(!SerialWorkerError::fatal("invalid configuration").retryable);
        assert!(SerialWorkerError::retryable("device unavailable").retryable);
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
