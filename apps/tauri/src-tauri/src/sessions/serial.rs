use std::fmt::Write as _;

use serde_json::Value;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, StopBits};

use super::telnet::reject_unsupported;
use super::terminal::{decode_terminal, emit_terminal_data, encode_terminal, set_terminal_state};
use super::WorkerCmd;
use crate::services::WorkspaceTabStatus;

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
            emit_terminal_data(&app, &tab_id, &format!("\r\n[串口] {error}\r\n")).await;
            set_terminal_state(
                &app,
                &tab_id,
                format!("串口错误：{error}"),
                WorkspaceTabStatus::Error,
            )
            .await;
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
        .ok_or_else(|| "串口设备路径不能为空".to_string())?;
    let baud_rate = profile
        .get("baudRate")
        .and_then(Value::as_u64)
        .unwrap_or(115_200) as u32;
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
    let output_mode = profile
        .get("outputMode")
        .and_then(Value::as_str)
        .unwrap_or("text")
        .to_string();
    validate_serial_modes(&newline_mode, &input_mode, &output_mode)?;
    let local_echo = profile
        .get("localEcho")
        .and_then(Value::as_bool)
        .unwrap_or(false);
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
        format!("串口 {device_path} @ {baud_rate}"),
        WorkspaceTabStatus::Connected,
    )
    .await;
    emit_terminal_data(app, tab_id, "串口已连接\r\n").await;
    let mut buffer = vec![0_u8; 32 * 1024];
    let mut hex_input_buffer = String::new();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCmd::WriteTerminal(data)) => {
                        if input_mode == "hex" {
                            for line in consume_serial_hex_input(&mut hex_input_buffer, &data) {
                                let input = format!("{line}\n");
                                let encoded = match encode_serial_input(
                                    &input,
                                    &encoding,
                                    &input_mode,
                                    &newline_mode,
                                ) {
                                    Ok(encoded) => encoded,
                                    Err(error) => {
                                        emit_terminal_data(app, tab_id, &format!("\r\n[Hex] {error}\r\n")).await;
                                        continue;
                                    }
                                };
                                writer.write_all(&encoded).await.map_err(|error| error.to_string())?;
                                writer.flush().await.map_err(|error| error.to_string())?;
                                if local_echo && !encoded.is_empty() {
                                    let echoed = serial_display(&encoded, &encoding, &output_mode)?;
                                    emit_terminal_data(app, tab_id, &echoed).await;
                                }
                            }
                        } else {
                            let encoded = encode_serial_input(&data, &encoding, &input_mode, &newline_mode)?;
                            writer.write_all(&encoded).await.map_err(|error| error.to_string())?;
                            writer.flush().await.map_err(|error| error.to_string())?;
                            if local_echo && !encoded.is_empty() {
                                let echoed = serial_display(&encoded, &encoding, &output_mode)?;
                                emit_terminal_data(app, tab_id, &echoed).await;
                            }
                        }
                    }
                    Some(WorkerCmd::ResizeTerminal { .. }) => {
                        // Raw serial links have no terminal-size negotiation.
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        crate::services::logging::session(app, "INFO", "serial", tab_id, "disconnecting");
                        let _ = writer.shutdown().await;
                        set_terminal_state(app, tab_id, "串口已断开".to_string(), WorkspaceTabStatus::Closed).await;
                        return Ok(());
                    }
                    Some(command) => reject_unsupported(command, "Serial 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                let count = read.map_err(|error| serial_error(device_path, error))?;
                if count == 0 {
                    crate::services::logging::session(app, "WARN", "serial", tab_id, "device disconnected");
                    set_terminal_state(app, tab_id, "串口设备已断开".to_string(), WorkspaceTabStatus::Closed).await;
                    return Ok(());
                }
                let output = serial_display(&buffer[..count], &encoding, &output_mode)?;
                emit_terminal_data(app, tab_id, &output).await;
            }
        }
    }
}

fn validate_serial_modes(
    newline_mode: &str,
    input_mode: &str,
    output_mode: &str,
) -> Result<(), String> {
    if !matches!(newline_mode, "none" | "lf" | "cr" | "crlf") {
        return Err("串口换行模式无效".to_string());
    }
    if !matches!(input_mode, "text" | "hex") {
        return Err("串口输入模式无效".to_string());
    }
    if !matches!(output_mode, "text" | "hex") {
        return Err("串口输出模式无效".to_string());
    }
    Ok(())
}

fn normalize_serial_newlines(value: &str, mode: &str) -> Result<String, String> {
    let replacement = match mode {
        "none" => return Ok(value.to_string()),
        "lf" => "\n",
        "cr" => "\r",
        "crlf" => "\r\n",
        _ => return Err("串口换行模式无效".to_string()),
    };
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    Ok(normalized.replace('\n', replacement))
}

fn serial_newline_bytes(mode: &str) -> &'static [u8] {
    match mode {
        "lf" => b"\n",
        "cr" => b"\r",
        "crlf" => b"\r\n",
        _ => b"",
    }
}

fn encode_serial_input(
    value: &str,
    encoding: &str,
    input_mode: &str,
    newline_mode: &str,
) -> Result<Vec<u8>, String> {
    let has_line_break = value.contains('\r') || value.contains('\n');
    let transformed = normalize_serial_newlines(value, newline_mode)?;
    match input_mode {
        "text" => Ok(encode_terminal(&transformed, encoding)),
        "hex" => {
            // xterm sends Enter as CR. In hex mode it is a line terminator for
            // the input editor, not another hex digit; append the configured
            // wire terminator explicitly when one is selected.
            let content = transformed.replace(['\r', '\n'], "");
            let mut bytes = parse_hex_input(&content)?;
            if has_line_break {
                bytes.extend_from_slice(serial_newline_bytes(newline_mode));
            }
            Ok(bytes)
        }
        _ => Err("串口输入模式无效".to_string()),
    }
}

fn consume_serial_hex_input(buffer: &mut String, data: &str) -> Vec<String> {
    let mut ready_lines = Vec::new();
    for character in data.chars() {
        match character {
            '\r' | '\n' => ready_lines.push(std::mem::take(buffer)),
            '\u{8}' | '\u{7f}' => {
                buffer.pop();
            }
            _ => buffer.push(character),
        }
    }
    ready_lines
}

fn parse_hex_input(value: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for token in value.split(|character: char| {
        character.is_ascii_whitespace() || matches!(character, ':' | ',' | '_')
    }) {
        if token.is_empty() {
            continue;
        }
        let digits = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        if digits.is_empty() || digits.len() % 2 != 0 {
            return Err(format!("Hex 输入必须按两个字符表示一个字节：{token}"));
        }
        for pair in digits.as_bytes().chunks_exact(2) {
            let pair = std::str::from_utf8(pair).expect("hex input is ASCII-compatible");
            let byte = u8::from_str_radix(pair, 16)
                .map_err(|_| format!("Hex 输入包含无效字节：{pair}"))?;
            bytes.push(byte);
        }
    }
    Ok(bytes)
}

fn serial_display(bytes: &[u8], encoding: &str, output_mode: &str) -> Result<String, String> {
    match output_mode {
        "text" => Ok(decode_terminal(bytes, encoding)),
        "hex" => Ok(format_serial_hex(bytes)),
        _ => Err("串口输出模式无效".to_string()),
    }
}

fn format_serial_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        let _ = write!(output, "{byte:02X}");
    }
    output
}

fn data_bits(value: u64) -> Result<DataBits, String> {
    match value {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err("串口数据位必须是 5、6、7 或 8".to_string()),
    }
}

fn stop_bits(value: u64) -> Result<StopBits, String> {
    match value {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err("串口停止位必须是 1 或 2".to_string()),
    }
}

fn parity(value: &str) -> Result<Parity, String> {
    match value {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        _ => Err("当前平台的串口校验位必须是无、奇或偶校验".to_string()),
    }
}

fn flow_control(value: &str) -> Result<FlowControl, String> {
    match value {
        "none" => Ok(FlowControl::None),
        "software" => Ok(FlowControl::Software),
        "hardware" => Ok(FlowControl::Hardware),
        _ => Err("串口流控必须是无、软件或硬件流控".to_string()),
    }
}

fn serial_error(device_path: &str, error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("Permission denied") || message.contains("EACCES") {
        return format!(
            "无法打开串口 {device_path}：权限不足。Linux 请将当前用户加入 dialout 组。"
        );
    }
    if message.contains("No such file") || message.contains("ENOENT") {
        return format!("串口设备 {device_path} 不存在、不可用或已断开。");
    }
    if message.contains("busy") || message.contains("EBUSY") {
        return format!("串口设备 {device_path} 已被其他程序占用。");
    }
    format!("串口 {device_path}：{message}")
}

#[cfg(test)]
mod tests {
    use super::{
        consume_serial_hex_input, data_bits, encode_serial_input, flow_control, format_serial_hex,
        normalize_serial_newlines, parity, parse_hex_input, serial_display, stop_bits, FlowControl,
    };

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

    #[test]
    fn normalizes_serial_line_endings_without_touching_other_text() {
        assert_eq!(
            normalize_serial_newlines("a\r\nb\rc\nd", "none").unwrap(),
            "a\r\nb\rc\nd"
        );
        assert_eq!(
            normalize_serial_newlines("a\r\nb\rc\nd", "lf").unwrap(),
            "a\nb\nc\nd"
        );
        assert_eq!(
            normalize_serial_newlines("a\r\nb\rc\nd", "cr").unwrap(),
            "a\rb\rc\rd"
        );
        assert_eq!(
            normalize_serial_newlines("a\r\nb\rc\nd", "crlf").unwrap(),
            "a\r\nb\r\nc\r\nd"
        );
    }

    #[test]
    fn encodes_text_and_hex_serial_input_modes() {
        assert_eq!(
            encode_serial_input("ping\r", "UTF-8", "text", "lf").unwrap(),
            b"ping\n"
        );
        assert_eq!(
            encode_serial_input("48 65 0x6C6C6F\r", "UTF-8", "hex", "crlf").unwrap(),
            b"Hello\r\n"
        );
        assert!(encode_serial_input("ABC", "UTF-8", "hex", "none").is_err());
    }

    #[test]
    fn buffers_hex_keystrokes_until_enter_and_supports_backspace() {
        let mut buffer = String::new();
        assert!(consume_serial_hex_input(&mut buffer, "4").is_empty());
        assert!(consume_serial_hex_input(&mut buffer, "8").is_empty());
        assert_eq!(buffer, "48");
        assert_eq!(
            consume_serial_hex_input(&mut buffer, "\u{7f}8\r"),
            vec!["48".to_string()]
        );
        assert!(buffer.is_empty());
        assert!(consume_serial_hex_input(&mut buffer, "48").is_empty());
        assert_eq!(
            consume_serial_hex_input(&mut buffer, " 69\n"),
            vec!["48 69".to_string()]
        );
        assert!(buffer.is_empty());
    }

    #[test]
    fn parses_common_hex_separators_and_formats_received_bytes() {
        assert_eq!(parse_hex_input("0x41:42,43_44").unwrap(), b"ABCD");
        assert_eq!(format_serial_hex(&[0x00, 0x0a, 0xff]), "00 0A FF");
        assert_eq!(
            serial_display(&[0x00, 0x0a, 0xff], "UTF-8", "hex").unwrap(),
            "00 0A FF"
        );
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
