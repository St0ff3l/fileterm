// System metrics collection — russh async port.
//
// `probe_remote_platform` and `exec_command[_with_stdin]` are async and
// operate on a `russh::client::Handle`. All parsing/formatting helpers
// below are pure functions and unchanged from the ssh2 era.
use std::io::Write;
use std::time::Duration;

use base64::Engine;
use flate2::write::GzEncoder;
use flate2::Compression;
use russh::client::{Handle, Handler};
use russh::ChannelMsg;
use tokio::time::timeout;

// A few SSH transports deliver a terminal channel marker before the final
// stdout packet is drained. Keep a very short grace window after EOF, CLOSE,
// or ExitStatus: it preserves output while still guaranteeing that servers
// which omit the remaining markers cannot hold a caller until its much longer
// command watchdog fires.
const EXEC_CHANNEL_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);

/// `exec_command` / `exec_command_with_stdin` 收集的 output 字节上限。
/// probe 命令的正常输出只有几行（uname / 系统信息），超过 256KB 说明
/// 服务器配置异常或被入侵——这种情况下截断输出比让 Vec 无限增长更安全。
/// 参考 meatshell 的 MON_BUF_CAP 设计，防止恶意服务器内存 DoS。
const EXEC_COMMAND_OUTPUT_CAP: usize = 256 * 1024;

/// Bounded output collected from a dedicated SSH exec channel. The legacy
/// tuple helpers below intentionally keep their stable shape, while callers
/// that surface output to users can expose the truncation bit instead of
/// pretending the remote command produced a complete result.
#[derive(Clone, Debug)]
pub struct ExecCommandResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub output_truncated: bool,
    /// The command deadline elapsed after the exec channel was opened. Any
    /// safely collected bytes are still returned so callers can distinguish a
    /// command that never produced output from one that ran partially.
    pub timed_out: bool,
}

pub async fn probe_remote_platform<H: Handler>(handle: &Handle<H>) -> String {
    // 1. Try POSIX probe
    let posix_cmd = "sh -lc 'printf \"__FILETERM_PROBE_START__\\n\"; uname -s 2>/dev/null; shell_exe=$(readlink /proc/$$/exe 2>/dev/null || readlink /bin/sh 2>/dev/null || true); case \"$shell_exe\" in *busybox*) printf \"busybox\\n\" ;; esac; if [ -f /etc/openwrt_release ]; then printf \"openwrt\\n\"; fi; printf \"__FILETERM_PROBE_END__\\n\"'";

    let posix_result = exec_command(handle, posix_cmd).await;
    eprintln!(
        "[SSH probe] posix exec_command result_ok={} len={}",
        posix_result.is_ok(),
        posix_result.as_ref().map(|s| s.len()).unwrap_or(0)
    );
    if let Ok(output) = &posix_result {
        // CRLF normalization — Windows remotes emit `\r\n` which would
        // pollute platform detection (e.g. `linux\r` fails `contains`).
        let output = output.replace("\r\n", "\n").replace('\r', "\n");
        eprintln!(
            "[SSH probe] posix normalized output (first 300): {:?}",
            output.chars().take(300).collect::<String>()
        );
        if let Some(body) = extract_probe_body(&output) {
            eprintln!("[SSH probe] body='{}'", body);
            if let Some(platform) = classify_posix_probe_body(&body) {
                return platform.to_string();
            }
        }
    }

    // 2. Try Windows probes
    let windows_cmds = [
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "cmd /c ver",
    ];
    for cmd in &windows_cmds {
        if let Ok(output) = exec_command(handle, cmd).await {
            let output = output.replace("\r\n", "\n").replace('\r', "\n");
            eprintln!(
                "[SSH probe] windows cmd='{}' output='{}'",
                cmd,
                output.chars().take(100).collect::<String>()
            );
            if let Some(platform) = classify_windows_probe_output(&output) {
                return platform.to_string();
            }
        }
    }

    eprintln!("[SSH probe] all probes failed — returning 'unknown'");
    "unknown".to_string()
}

/// Classify the body of the POSIX probe (the text between
/// `__FILETERM_PROBE_START__` and `__FILETERM_PROBE_END__`) into a platform
/// label. Returns `None` when no known marker is present so the caller can
/// fall through to the Windows probes.
///
/// Extracted as a pure function so platform detection can be unit-tested
/// without a live SSH handle.
fn classify_posix_probe_body(body: &str) -> Option<&'static str> {
    let normalized = body.to_lowercase();
    if normalized.contains("openwrt") || normalized.contains("busybox") {
        return Some("busybox");
    }
    if normalized.contains("linux") {
        return Some("linux");
    }
    // macOS / Darwin: `uname -s` returns "Darwin". Bash/zsh on macOS support
    // the same PROMPT_COMMAND / precmd hooks as Linux, so we surface a
    // distinct `darwin` label and let the CWD-setup gate reuse the Linux
    // hook. Without this branch macOS remotes fall through to the Windows
    // probes and end up as `unknown`, losing CWD tracking and sudo/root
    // synchronization on the primary development platform.
    if normalized.contains("darwin") {
        return Some("darwin");
    }
    None
}

/// Classify the output of a Windows probe command. `cmd /c ver` and
/// `[Environment]::OSVersion.Platform` both surface the word "windows" or
/// "win32nt" on Windows remotes.
fn classify_windows_probe_output(output: &str) -> Option<&'static str> {
    let normalized = output.to_lowercase();
    if normalized.contains("windows") || normalized.contains("win32nt") {
        Some("windows")
    } else {
        None
    }
}

/// Run a command via the exec channel and collect its combined stdout/stderr.
pub async fn exec_command<H: Handler>(handle: &Handle<H>, cmd: &str) -> Result<String, String> {
    exec_command_with_status(handle, cmd)
        .await
        .map(|(output, _)| output)
}

/// Run a command via the exec channel and retain the SSH-level exit status.
///
/// The regular `exec_command` API intentionally returns output only because
/// most callers are best-effort probes. File operations need the status to
/// distinguish an empty successful result from a failed command, especially
/// when the command output itself reaches the collection cap.
pub async fn exec_command_with_status<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
) -> Result<(String, Option<u32>), String> {
    exec_command_with_status_detailed(handle, cmd)
        .await
        .map(|result| (result.output, result.exit_code))
}

/// Like [`exec_command_with_status`], but preserves whether the bounded
/// collector discarded remote output after its safety cap.
pub async fn exec_command_with_status_detailed<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
) -> Result<ExecCommandResult, String> {
    exec_command_internal(handle, cmd, None, false, None).await
}

/// Like [`exec_command_with_status_detailed`], but bounds the remote command
/// without discarding output already received before the deadline. This is
/// used for externally visible remote-exec results where a partial diagnostic
/// can be more useful than an empty timeout response.
pub async fn exec_command_with_status_timeout_detailed<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    command_timeout: Duration,
) -> Result<ExecCommandResult, String> {
    exec_command_internal(handle, cmd, None, false, Some(command_timeout)).await
}

/// Run a command via the exec channel, write `stdin`, and retain the SSH
/// channel's exit status.
pub async fn exec_command_with_stdin_status<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
) -> Result<(String, Option<u32>), String> {
    exec_command_internal(handle, cmd, Some(stdin.as_bytes()), false, None)
        .await
        .map(|result| (result.output, result.exit_code))
}

/// Run an exec command with a requested PTY and retain its SSH-level exit
/// status.  This is the no-input counterpart to
/// [`exec_command_with_stdin_status_pty`].
pub async fn exec_command_with_status_pty<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
) -> Result<(String, Option<u32>), String> {
    exec_command_internal(handle, cmd, None, true, None)
        .await
        .map(|result| (result.output, result.exit_code))
}

/// Run an exec command with a requested PTY, write `stdin`, and retain the
/// SSH channel's exit status.  `su` authenticates through the controlling
/// terminal on many PAM setups, while a plain exec channel has no terminal at
/// all; callers that reproduce an interactive `su -` exchange use this path.
pub async fn exec_command_with_stdin_status_pty<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
) -> Result<(String, Option<u32>), String> {
    exec_command_internal(handle, cmd, Some(stdin.as_bytes()), true, None)
        .await
        .map(|result| (result.output, result.exit_code))
}

/// Run an exec command with optional stdin/PTY while retaining the bounded
/// output and timeout metadata used by the remote-exec service. The caller is
/// responsible for ensuring that `stdin` never contains data that should be
/// logged or returned to an untrusted surface.
pub async fn exec_command_with_stdin_status_timeout_detailed<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
    request_pty: bool,
    command_timeout: Duration,
) -> Result<ExecCommandResult, String> {
    exec_command_internal(
        handle,
        cmd,
        Some(stdin.as_bytes()),
        request_pty,
        Some(command_timeout),
    )
    .await
}

async fn exec_command_internal<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: Option<&[u8]>,
    request_pty: bool,
    command_timeout: Option<Duration>,
) -> Result<ExecCommandResult, String> {
    let deadline = command_timeout.map(|duration| tokio::time::Instant::now() + duration);
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    if request_pty {
        channel
            .request_pty(
                true,
                "xterm-256color",
                80,
                24,
                0,
                0,
                &[
                    // Do not echo the password (or a base64 payload written
                    // by a future PTY-backed file transfer) back into the
                    // collected command output.
                    (russh::Pty::ECHO, 0),
                    (russh::Pty::ECHOE, 0),
                    (russh::Pty::ECHOK, 0),
                    (russh::Pty::ECHONL, 0),
                    (russh::Pty::TTY_OP_ISPEED, 115200),
                    (russh::Pty::TTY_OP_OSPEED, 115200),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
    }
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;
    // `su` reads its password from the controlling PTY only after it has
    // emitted the prompt. Sending the password immediately after `exec`
    // races PAM on several OpenSSH/PAM combinations and leaves `su -c`
    // blocked forever. Keep PTY input pending until the prompt arrives;
    // non-PTY execs (notably `sudo -S`) continue to use pipe semantics.
    let mut pending_pty_stdin = if request_pty { stdin } else { None };
    if let Some(stdin) = stdin.filter(|_| !request_pty) {
        channel.data(stdin).await.map_err(|e| e.to_string())?;
        channel.eof().await.map_err(|e| e.to_string())?;
    }

    let mut output: Vec<u8> = Vec::new();
    let mut pty_prompt_window: Vec<u8> = Vec::new();
    let mut exit_status = None;
    let mut draining_after_close = false;
    let mut capped = false;
    let mut timed_out = false;
    loop {
        let message = match (draining_after_close, deadline) {
            (true, Some(deadline)) => {
                tokio::select! {
                    message = timeout(EXEC_CHANNEL_DRAIN_TIMEOUT, channel.wait()) => match message {
                        Ok(message) => message,
                        Err(_) => break,
                    },
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        break;
                    }
                }
            }
            (true, None) => match timeout(EXEC_CHANNEL_DRAIN_TIMEOUT, channel.wait()).await {
                Ok(message) => message,
                Err(_) => break,
            },
            (false, Some(deadline)) => {
                tokio::select! {
                    message = channel.wait() => message,
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        break;
                    }
                }
            }
            (false, None) => channel.wait().await,
        };
        match message {
            Some(ChannelMsg::Data { data }) => {
                if !capped {
                    extend_with_cap(&mut output, data.as_ref(), &mut capped);
                }
                append_pty_prompt_window(&mut pty_prompt_window, data.as_ref());
                if pending_pty_stdin.is_some() && pty_password_prompt_detected(&pty_prompt_window) {
                    let stdin = pending_pty_stdin
                        .take()
                        .expect("pending PTY input was checked above");
                    channel.data(stdin).await.map_err(|e| e.to_string())?;
                    // A PTY is a terminal, not a pipe: SSH channel EOF does
                    // not reliably become stdin EOF. Send the terminal's VEOF
                    // byte after the password's newline so `su -c` observes
                    // the same end-of-input as Ctrl+D if it needs it.
                    channel
                        .data_bytes(vec![0x04])
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                if !capped {
                    extend_with_cap(&mut output, data.as_ref(), &mut capped);
                }
                append_pty_prompt_window(&mut pty_prompt_window, data.as_ref());
                if pending_pty_stdin.is_some() && pty_password_prompt_detected(&pty_prompt_window) {
                    let stdin = pending_pty_stdin
                        .take()
                        .expect("pending PTY input was checked above");
                    channel.data(stdin).await.map_err(|e| e.to_string())?;
                    channel
                        .data_bytes(vec![0x04])
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => {
                exit_status = Some(status);
                draining_after_close = true;
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                draining_after_close = true;
            }
            None => break,
            _ => {}
        }
    }
    Ok(ExecCommandResult {
        output: String::from_utf8_lossy(&output).into_owned(),
        exit_code: exit_status,
        output_truncated: capped,
        timed_out,
    })
}

const PTY_PROMPT_WINDOW_BYTES: usize = 2 * 1024;

fn append_pty_prompt_window(window: &mut Vec<u8>, chunk: &[u8]) {
    window.extend_from_slice(chunk);
    if window.len() > PTY_PROMPT_WINDOW_BYTES {
        let keep_from = window.len() - PTY_PROMPT_WINDOW_BYTES;
        window.drain(..keep_from);
    }
}

fn pty_password_prompt_detected(window: &[u8]) -> bool {
    let visible = String::from_utf8_lossy(window);
    let lower = visible.to_ascii_lowercase();
    lower.contains("password") || visible.contains("密码")
}

/// Run a command via the exec channel, write `stdin` to the channel, then
/// collect the combined stdout/stderr.
pub async fn exec_command_with_stdin<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
) -> Result<String, String> {
    exec_command_with_stdin_status(handle, cmd, stdin)
        .await
        .map(|(output, _)| output)
}

fn extract_probe_body(raw: &str) -> Option<String> {
    let start_marker = "__FILETERM_PROBE_START__";
    let end_marker = "__FILETERM_PROBE_END__";
    let start = raw.find(start_marker)?;
    let end = raw.find(end_marker)?;
    if end <= start {
        return None;
    }
    Some(raw[start + start_marker.len()..end].to_string())
}

/// Append `chunk` to `output` but stop growing once `EXEC_COMMAND_OUTPUT_CAP`
/// is reached. Sets `capped` so the caller can skip future appends without
/// re-checking the length each iteration. A malicious or misconfigured server
/// that floods stdout must not be able to grow memory unbounded.
fn extend_with_cap(output: &mut Vec<u8>, chunk: &[u8], capped: &mut bool) {
    if *capped {
        return;
    }
    let remaining = EXEC_COMMAND_OUTPUT_CAP.saturating_sub(output.len());
    if remaining == 0 {
        *capped = true;
        return;
    }
    if chunk.len() <= remaining {
        output.extend_from_slice(chunk);
    } else {
        output.extend_from_slice(&chunk[..remaining]);
        *capped = true;
    }
}

fn megabytes_to_bytes(val: &str) -> f64 {
    val.parse::<f64>().unwrap_or(0.0) * 1024.0 * 1024.0
}

fn format_bytes_as_megabytes(val: f64) -> String {
    let megabytes = val / 1024.0 / 1024.0;
    if megabytes >= 1024.0 {
        format!("{:.1}G", megabytes / 1024.0)
    } else {
        format!("{}M", megabytes.round() as i64)
    }
}

fn format_rate(bytes_per_sec: f64) -> String {
    let bps = bytes_per_sec.max(0.0);
    if bps >= 1024.0 * 1024.0 {
        format!("{}M", (bps / 1024.0 / 1024.0).round() as i64)
    } else if bps >= 1024.0 {
        format!("{}K", (bps / 1024.0).round() as i64)
    } else {
        format!("{}B", bps as i64)
    }
}

fn format_network_bytes(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} TB", bytes / 1024.0 / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 * 1024.0 * 1024.0 {
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 * 1024.0 {
            0
        } else {
            1
        };
        format!("{:.*} GB", decimals, bytes / 1024.0 / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 * 1024.0 {
        let decimals = if bytes >= 10.0 * 1024.0 * 1024.0 {
            0
        } else {
            1
        };
        format!("{:.*} MB", decimals, bytes / 1024.0 / 1024.0)
    } else if bytes >= 1024.0 {
        format!("{} KB", (bytes / 1024.0).round() as i64)
    } else {
        format!("{} B", bytes as i64)
    }
}

fn format_storage_usage(value: &str) -> String {
    if value.is_empty() {
        return "-".to_string();
    }
    if let Some(idx) = value.find('/') {
        format!(
            "{}/{}",
            format_storage_value(&value[..idx]),
            format_storage_value(&value[idx + 1..])
        )
    } else {
        format_storage_value(value)
    }
}

fn format_storage_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.contains(' ') {
        return trimmed.to_string();
    }
    let re = regex::Regex::new(r"(?i)^(\d+(?:\.\d+)?)([KMGT])(?:I?B)?$").unwrap();
    if let Some(caps) = re.captures(trimmed) {
        let val_num: f64 = caps[1].parse().unwrap_or(0.0);
        let unit = caps[2].to_uppercase();
        let power = match unit.as_str() {
            "K" => 1,
            "M" => 2,
            "G" => 3,
            "T" => 4,
            _ => 0,
        };
        let mut bytes = val_num * 1024_f64.powi(power);
        let display_units = ["B", "KB", "MB", "GB", "TB"];
        let mut idx = 0;
        while bytes >= 1024.0 && idx < display_units.len() - 1 {
            bytes /= 1024.0;
            idx += 1;
        }
        let decimals = if idx == 0 { 0 } else { 1 };
        return format!("{:.*} {}", decimals, bytes, display_units[idx]);
    }
    trimmed.to_string()
}

fn parse_gpu_memory_bytes(value: &str) -> Option<f64> {
    let normalized = value.trim().replace(' ', "");
    if normalized.is_empty() || normalized == "-" {
        return None;
    }

    let re = regex::Regex::new(r"(?i)^([0-9]+(?:\.[0-9]+)?)([KMGT]?)(?:I?B)?$").unwrap();
    let caps = re.captures(&normalized)?;
    let amount = caps.get(1)?.as_str().parse::<f64>().ok()?;
    let unit = caps.get(2).map(|m| m.as_str().to_ascii_uppercase());
    let power = match unit.as_deref() {
        Some("K") => 1,
        Some("M") => 2,
        Some("G") => 3,
        Some("T") => 4,
        // nvidia-smi is called with `nounits`, and its memory fields are MiB.
        Some("") | None => 2,
        _ => return None,
    };

    Some(amount * 1024_f64.powi(power))
}

fn format_gpu_memory(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return "-".to_string();
    }

    let normalized = trimmed.replace(' ', "");
    if normalized == "-" {
        return "-".to_string();
    }
    format_storage_value(&normalized)
}

fn parse_gpu_percent(value: &str) -> Option<f64> {
    // PowerShell's Windows emitter formats nvidia-smi values as `49 %`.
    // Trim again after removing the suffix so the whitespace between the
    // number and unit does not turn an otherwise valid sample into `None`.
    let normalized = value.trim().trim_end_matches('%').trim();
    let parsed = normalized.parse::<f64>().ok()?;
    Some(parsed.clamp(0.0, 100.0))
}

fn parse_gpu_temperature(value: &str) -> Option<f64> {
    value
        .trim()
        .trim_end_matches('C')
        .trim_end_matches('c')
        .trim_end_matches('°')
        .trim()
        .parse::<f64>()
        .ok()
}

fn format_gpu_optional(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value == "-" {
        None
    } else {
        Some(value.to_string())
    }
}

fn format_process_megabytes(value: f64) -> String {
    if value >= 1024.0 {
        let decimals = if value >= 10.0 * 1024.0 { 0 } else { 1 };
        format!("{:.*}G", decimals, value / 1024.0)
    } else {
        let decimals = if value >= 100.0 { 0 } else { 1 };
        format!("{:.*}M", decimals, value)
    }
}

pub fn parse_system_metrics(raw: &str, fallback_platform: &str) -> serde_json::Value {
    let normalized_raw = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized_raw.split('\n').collect();

    let read_line = |key: &str| -> String {
        for line in &lines {
            if let Some(stripped) = line.strip_prefix(key) {
                return stripped.trim().to_string();
            }
        }
        "".to_string()
    };

    let read_block = |start: &str, end: &str| -> Vec<String> {
        let start_index = match normalized_raw.find(start) {
            Some(idx) => idx,
            None => return Vec::new(),
        };
        let body_start = start_index + start.len();
        // 起始标记存在但结束标记缺失时，远端脚本可能被截断；
        // 取到字符串结尾作为容错，避免静默丢弃已采集到的数据。
        let body = match normalized_raw[body_start..].find(end) {
            Some(idx) => &normalized_raw[body_start..body_start + idx],
            None => &normalized_raw[body_start..],
        };
        body.trim()
            .split('\n')
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    };

    let platform = read_line("__PLATFORM__");
    let platform = if platform.is_empty() {
        fallback_platform.to_string()
    } else {
        platform
    };
    let load_unit = read_line("__LOAD_UNIT__");
    let load_unit = if load_unit == "busy-logical-processors" {
        Some("busy-logical-processors")
    } else {
        None
    };

    let mem_line = read_line("__MEM__");
    let mem_parts: Vec<&str> = mem_line.split('|').collect();
    let mem_used = mem_parts.first().copied().unwrap_or("0");
    let mem_total = mem_parts.get(1).copied().unwrap_or("0");
    let mem_percent = mem_parts.get(2).copied().unwrap_or("0");
    let mem_app = mem_parts.get(3).copied().unwrap_or("0");
    let mem_cache = mem_parts.get(4).copied().unwrap_or("0");
    let mem_kernel = mem_parts.get(5).copied().unwrap_or("0");

    let mem_bytes_line = read_line("__MEM_BYTES__");
    let mem_bytes_parts: Vec<&str> = mem_bytes_line.split('|').collect();
    let mem_used_bytes = mem_bytes_parts.first().copied().unwrap_or("");
    let mem_total_bytes = mem_bytes_parts.get(1).copied().unwrap_or("");
    let mem_available_bytes = mem_bytes_parts.get(2).copied().unwrap_or("");
    let mem_raw_percent = mem_bytes_parts.get(3).copied().unwrap_or("");
    let mem_app_bytes = mem_bytes_parts.get(4).copied().unwrap_or("");
    let mem_cache_bytes = mem_bytes_parts.get(5).copied().unwrap_or("");
    let mem_kernel_bytes = mem_bytes_parts.get(6).copied().unwrap_or("");

    let swap_line = read_line("__SWAP__");
    let swap_parts: Vec<&str> = swap_line.split('|').collect();
    let swap_used = swap_parts.first().copied().unwrap_or("0");
    let swap_total = swap_parts.get(1).copied().unwrap_or("0");
    let swap_percent = swap_parts.get(2).copied().unwrap_or("0");

    let swap_bytes_line = read_line("__SWAP_BYTES__");
    let swap_bytes_parts: Vec<&str> = swap_bytes_line.split('|').collect();
    let swap_used_bytes = swap_bytes_parts.first().copied().unwrap_or("");
    let swap_total_bytes = swap_bytes_parts.get(1).copied().unwrap_or("");
    let swap_available_bytes = swap_bytes_parts.get(2).copied().unwrap_or("");
    let swap_raw_percent = swap_bytes_parts.get(3).copied().unwrap_or("");

    let cpu_line = read_line("__CPU_USAGE__");
    let cpu_parts: Vec<&str> = cpu_line.split('|').collect();
    let cpu_user = cpu_parts
        .first()
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_system = cpu_parts
        .get(1)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_nice = cpu_parts
        .get(2)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_idle = cpu_parts
        .get(3)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_iowait = cpu_parts
        .get(4)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_irq = cpu_parts
        .get(5)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_softirq = cpu_parts
        .get(6)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);
    let cpu_steal = cpu_parts
        .get(7)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0);

    let rates_line = read_line("__RATES__");
    let rates_parts: Vec<&str> = rates_line.split('|').collect();
    let rx_rate = rates_parts
        .first()
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);
    let tx_rate = rates_parts
        .get(1)
        .copied()
        .unwrap_or("0")
        .parse::<f64>()
        .unwrap_or(0.0)
        .max(0.0);

    let interfaces: Vec<String> = read_line("__IFACES__")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // parse network interface rates
    let mut network_interface_rows = Vec::new();
    let mut network_rates_by_interface = serde_json::Map::new();
    let mut network_samples_by_interface = serde_json::Map::new();
    let mut network_raw_by_interface = serde_json::Map::new();

    let mut aggregate_rx_bytes = 0.0;
    let mut aggregate_tx_bytes = 0.0;

    for line in read_block("__IFACE_RATES_START__", "__IFACE_RATES_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 5 {
            let name = parts[0].to_string();
            let rx_total = parts[1].parse::<f64>().unwrap_or(0.0);
            let tx_total = parts[2].parse::<f64>().unwrap_or(0.0);
            let rx = parts[3].parse::<f64>().unwrap_or(0.0).max(0.0);
            let tx = parts[4].parse::<f64>().unwrap_or(0.0).max(0.0);

            aggregate_rx_bytes += rx_total;
            aggregate_tx_bytes += tx_total;

            network_interface_rows.push(serde_json::json!({
                "name": name,
                "txTotal": format_network_bytes(tx_total),
                "rxTotal": format_network_bytes(rx_total),
                "txRate": format_rate(tx),
                "rxRate": format_rate(rx),
            }));

            network_rates_by_interface.insert(
                name.clone(),
                serde_json::json!({
                    "rx": format_rate(rx),
                    "tx": format_rate(tx),
                }),
            );

            network_samples_by_interface.insert(
                name.clone(),
                serde_json::json!([
                    { "rx": rx, "tx": tx }
                ]),
            );

            network_raw_by_interface.insert(
                name.clone(),
                serde_json::json!({
                    "name": name,
                    "rxBytes": rx_total,
                    "txBytes": tx_total,
                    "rxBytesPerSecond": rx,
                    "txBytesPerSecond": tx,
                }),
            );
        }
    }

    let mut disk_rows = Vec::new();
    for line in read_block("__DISK_START__", "__DISK_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 2 {
            disk_rows.push(serde_json::json!({
                "path": parts[0],
                "usage": format_storage_usage(parts[1]),
            }));
        }
    }

    let mut file_system_rows = Vec::new();
    for line in read_block("__FILESYSTEMS_START__", "__FILESYSTEMS_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 6 {
            file_system_rows.push(serde_json::json!({
                "name": parts[0],
                "size": format_storage_value(parts[1]),
                "used": format_storage_value(parts[2]),
                "usagePercent": parts[3],
                "available": format_storage_value(parts[4]),
                "mountPoint": parts[5],
            }));
        }
    }

    // Newer collectors already provide the richer filesystem rows, while the
    // compact sidebar table still consumes the legacy diskRows shape. Keep the
    // compact table populated when a platform/collector emits only the richer
    // block (which is what caused the sidebar to show an empty body).
    if disk_rows.is_empty() {
        for row in &file_system_rows {
            let path = row
                .get("mountPoint")
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.is_empty())
                .or_else(|| row.get("name").and_then(serde_json::Value::as_str));
            let available = row.get("available").and_then(serde_json::Value::as_str);
            let size = row.get("size").and_then(serde_json::Value::as_str);

            if let (Some(path), Some(available), Some(size)) = (path, available, size) {
                disk_rows.push(serde_json::json!({
                    "path": path,
                    "usage": format!("{available}/{size}"),
                }));
            }
        }
    }

    let mut cpu_info_rows = Vec::new();
    for line in read_block("__CPUINFO_START__", "__CPUINFO_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 5 {
            cpu_info_rows.push(serde_json::json!({
                "model": parts[0],
                "cores": parts[1].parse::<i64>().unwrap_or(0),
                "frequencyMHz": parts[2],
                "cache": parts[3],
                "bogomips": parts[4],
            }));
        }
    }

    let mut gpu_info_rows = Vec::new();
    for line in read_block("__GPUINFO_START__", "__GPUINFO_END__") {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 4 {
            let total_memory_bytes = parse_gpu_memory_bytes(parts[3]);
            let used_memory_bytes = parts.get(5).and_then(|value| parse_gpu_memory_bytes(value));
            let memory_percent = match (used_memory_bytes, total_memory_bytes) {
                (Some(used), Some(total)) if total > 0.0 => {
                    Some((used * 100.0 / total).clamp(0.0, 100.0))
                }
                _ => None,
            };
            gpu_info_rows.push(serde_json::json!({
                "model": parts[0],
                "vendor": if parts[1].is_empty() { "-" } else { parts[1] },
                "driver": if parts[2].is_empty() { "-" } else { parts[2] },
                "memory": format_gpu_memory(parts[3]),
                "usagePercent": parts.get(4).and_then(|value| parse_gpu_percent(value)),
                "memoryUsed": parts
                    .get(5)
                    .map(|value| format_gpu_memory(value))
                    .filter(|value| value != "-"),
                "memoryPercent": memory_percent,
                "temperatureCelsius": parts.get(6).and_then(|value| parse_gpu_temperature(value)),
                "powerUsage": format_gpu_optional(parts.get(7).copied()),
                "powerLimit": format_gpu_optional(parts.get(8).copied()),
            }));
        }
    }

    // Top processes: shell 端按瞬时 CPU 占用降序取 top 40，
    // 这里按到达顺序逐行解析，不做 comm 分组。每行一个 PID，保留 pid/user
    // 字段供排查使用，command 用 args（完整命令行）而非 comm。
    // 格式：pid|user|rss(M)|pcpu|pmem|args（args 内部空格保留）
    let transient_collector_commands: std::collections::HashSet<&str> =
        ["ps", "awk", "bash", "sleep", "sh", "powershell", "pwsh"]
            .iter()
            .cloned()
            .collect();
    let mut top_processes: Vec<serde_json::Value> = Vec::new();
    for line in read_block("__PROCS_START__", "__PROCS_END__") {
        // splitn(6) 保留 args 内部所有字符（含 |），避免误切
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() < 6 {
            continue;
        }
        let pid: u32 = parts[0].parse().unwrap_or(0);
        let user = parts[1].to_string();
        let memory_str = parts[2].to_lowercase();
        let memory_mb: f64 = memory_str.replace('m', "").parse().unwrap_or(0.0);
        let cpu_val = match parts[3].parse::<f64>() {
            Ok(value) if value.is_finite() && (0.0..=100.0).contains(&value) => value,
            // The collector is expected to emit a system-wide 0-100 value.
            // Do not let malformed or unbounded samples reach the renderer.
            _ => continue,
        };
        let _mem_percent: f64 = parts[4].parse().unwrap_or(0.0);
        let command = parts[5].to_string();

        // 过滤采集器自身（ps/awk/sh 等），按 args 首字段匹配
        let comm = command.split_whitespace().next().unwrap_or("");
        let comm_basename = comm.rsplit('/').next().unwrap_or(comm);
        if transient_collector_commands.contains(comm_basename) {
            continue;
        }

        top_processes.push(serde_json::json!({
            "pid": pid,
            "user": user,
            "memory": format_process_megabytes(memory_mb),
            "cpu": format!("{:.1}", cpu_val),
            "command": command,
            "elapsedSeconds": 0_i64,
        }));
    }

    let mem_used_bytes_num = mem_used_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_used));
    let mem_total_bytes_num = mem_total_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_total));
    let mem_available_bytes_num = mem_available_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| (mem_total_bytes_num - mem_used_bytes_num).max(0.0));
    let mem_percent_num = mem_raw_percent
        .parse::<f64>()
        .unwrap_or_else(|_| mem_percent.parse::<f64>().unwrap_or(0.0));

    let swap_used_bytes_num = swap_used_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(swap_used));
    let swap_total_bytes_num = swap_total_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(swap_total));
    let swap_available_bytes_num = swap_available_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| (swap_total_bytes_num - swap_used_bytes_num).max(0.0));
    let swap_percent_num = swap_raw_percent
        .parse::<f64>()
        .unwrap_or_else(|_| swap_percent.parse::<f64>().unwrap_or(0.0));

    let mem_app_bytes_num = mem_app_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_app));
    let mem_cache_bytes_num = mem_cache_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_cache));
    let mem_kernel_bytes_num = mem_kernel_bytes
        .parse::<f64>()
        .unwrap_or_else(|_| megabytes_to_bytes(mem_kernel));

    let aggregate_network_raw = serde_json::json!({
        "name": "all",
        "rxBytes": aggregate_rx_bytes,
        "txBytes": aggregate_tx_bytes,
        "rxBytesPerSecond": rx_rate,
        "txBytesPerSecond": tx_rate,
    });

    let has_mem_app = mem_app.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_app_bytes_num > 0.0;
    let has_mem_cache = mem_cache.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_cache_bytes_num > 0.0;
    let has_mem_kernel =
        mem_kernel.parse::<f64>().unwrap_or(0.0) > 0.0 || mem_kernel_bytes_num > 0.0;

    let mut network_rates_all = serde_json::Map::new();
    network_rates_all.insert(
        "all".to_string(),
        serde_json::json!({
            "rx": format_rate(rx_rate),
            "tx": format_rate(tx_rate),
        }),
    );
    for (k, v) in network_rates_by_interface.iter() {
        network_rates_all.insert(k.clone(), v.clone());
    }

    let mut network_samples_all = serde_json::Map::new();
    network_samples_all.insert(
        "all".to_string(),
        serde_json::json!([
            { "rx": rx_rate, "tx": tx_rate }
        ]),
    );
    for (k, v) in network_samples_by_interface.iter() {
        network_samples_all.insert(k.clone(), v.clone());
    }

    let mut network_raw_all = serde_json::Map::new();
    network_raw_all.insert("all".to_string(), aggregate_network_raw);
    for (k, v) in network_raw_by_interface.iter() {
        network_raw_all.insert(k.clone(), v.clone());
    }

    let mut network_interfaces_val = vec![serde_json::Value::String("all".to_string())];
    for iface in interfaces {
        network_interfaces_val.push(serde_json::Value::String(iface));
    }

    serde_json::json!({
        "platform": platform,
        "ip": read_line("__IP__"),
        "uptime": if read_line("__UPTIME__").is_empty() { "-".to_string() } else { read_line("__UPTIME__") },
        "uptimeSeconds": read_line("__UPTIME_SECONDS__").parse::<i64>().ok(),
        "load": if read_line("__LOAD__").is_empty() { "-".to_string() } else { read_line("__LOAD__") },
        "loadUnit": load_unit,
        "identity": {
            "osName": if read_line("__OS__").is_empty() { "-".to_string() } else { read_line("__OS__") },
            "kernelName": if read_line("__KERNEL_NAME__").is_empty() { "-".to_string() } else { read_line("__KERNEL_NAME__") },
            "kernelVersion": if read_line("__KERNEL_VERSION__").is_empty() { "-".to_string() } else { read_line("__KERNEL_VERSION__") },
            "architecture": if read_line("__ARCH__").is_empty() { "-".to_string() } else { read_line("__ARCH__") },
            "hostname": if read_line("__HOSTNAME__").is_empty() { "-".to_string() } else { read_line("__HOSTNAME__") },
        },
        "cpuPercent": read_line("__CPU__").parse::<f64>().unwrap_or(0.0),
        "cpuUsage": {
            "user": cpu_user,
            "system": cpu_system,
            "nice": cpu_nice,
            "idle": cpu_idle,
            "ioWait": cpu_iowait,
            "irq": cpu_irq,
            "softIrq": cpu_softirq,
            "steal": cpu_steal,
        },
        "cpuInfoRows": cpu_info_rows,
        "gpuInfoRows": gpu_info_rows,
        "memoryPercent": mem_percent_num,
        "memoryUsage": if mem_total_bytes_num > 0.0 {
            format!("{}/{}", format_bytes_as_megabytes(mem_used_bytes_num), format_bytes_as_megabytes(mem_total_bytes_num))
        } else {
            "0/0".to_string()
        },
        "memoryAppUsage": if has_mem_app { Some(format_bytes_as_megabytes(mem_app_bytes_num)) } else { None },
        "memoryCacheUsage": if has_mem_cache { Some(format_bytes_as_megabytes(mem_cache_bytes_num)) } else { None },
        "memoryKernelUsage": if has_mem_kernel { Some(format_bytes_as_megabytes(mem_kernel_bytes_num)) } else { None },
        "memoryBreakdown": {
            "total": format_bytes_as_megabytes(mem_total_bytes_num),
            "used": format_bytes_as_megabytes(mem_used_bytes_num),
            "available": format_bytes_as_megabytes(mem_available_bytes_num),
            "percent": mem_percent_num,
        },
        "memoryRaw": {
            "totalBytes": mem_total_bytes_num,
            "usedBytes": mem_used_bytes_num,
            "availableBytes": mem_available_bytes_num,
            "percent": mem_percent_num,
            "appBytes": mem_app_bytes_num,
            "cacheBytes": mem_cache_bytes_num,
            "kernelBytes": mem_kernel_bytes_num,
        },
        "swapPercent": swap_percent_num,
        "swapUsage": if swap_total_bytes_num > 0.0 {
            format!("{}/{}", format_bytes_as_megabytes(swap_used_bytes_num), format_bytes_as_megabytes(swap_total_bytes_num))
        } else {
            "0/0".to_string()
        },
        "swapBreakdown": {
            "total": format_bytes_as_megabytes(swap_total_bytes_num),
            "used": format_bytes_as_megabytes(swap_used_bytes_num),
            "available": format_bytes_as_megabytes(swap_available_bytes_num),
            "percent": swap_percent_num,
        },
        "swapRaw": {
            "totalBytes": swap_total_bytes_num,
            "usedBytes": swap_used_bytes_num,
            "availableBytes": swap_available_bytes_num,
            "percent": swap_percent_num,
        },
        "diskRows": disk_rows,
        "fileSystemRows": file_system_rows,
        "networkInterfaces": network_interfaces_val,
        "activeNetworkInterface": "all",
        "networkRates": {
            "rx": format_rate(rx_rate),
            "tx": format_rate(tx_rate),
        },
        "networkSamples": [
            { "rx": rx_rate, "tx": tx_rate }
        ],
        "networkInterfaceRows": network_interface_rows,
        "networkRatesByInterface": network_rates_all,
                "networkSamplesByInterface": network_samples_all,
        "networkRawByInterface": network_raw_all,
        "topProcesses": top_processes,
    })
}

pub fn build_posix_metrics_command(platform: &str) -> String {
    let complete_marker = "__FILETERM_METRICS_COMPLETE__";
    format!(
        r#"cd / >/dev/null 2>&1 || true
sleep_interval="0.15"
sleep "$sleep_interval" >/dev/null 2>&1 || sleep_interval="1"
run_bounded() {{
  limit="$1"
  shift
  if command -v timeout >/dev/null 2>&1; then
    if timeout -k 1 1 true >/dev/null 2>&1; then
      timeout -k 1 "$limit" "$@"
    else
      timeout "$limit" "$@"
    fi
    return $?
  fi
  if command -v busybox >/dev/null 2>&1 && busybox timeout 1 true >/dev/null 2>&1; then
    if busybox timeout -k 1 1 true >/dev/null 2>&1; then
      busybox timeout -k 1 "$limit" "$@"
    else
      busybox timeout "$limit" "$@"
    fi
    return $?
  fi
  return 124
}}
has_bounded_runner() {{
  if command -v timeout >/dev/null 2>&1 && timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  if command -v busybox >/dev/null 2>&1 && busybox timeout 1 true >/dev/null 2>&1; then
    return 0
  fi
  return 1
}}
read_cpu_stat() {{
  awk '/^cpu / {{print $2, $3, $4, $5, $6, $7, $8, $9; exit}}' /proc/stat 2>/dev/null
}}
read_process_ticks() {{
  awk '
    {{
      path=FILENAME
      sub(/^\/proc\//, "", path)
      sub(/\/stat$/, "", path)
      line=$0
      sub(/^[0-9]+ \(.+\) /, "", line)
      count=split(line, fields, /[[:space:]]+/)
      if (count >= 13) printf "%s|%s\n", path, fields[12] + fields[13]
    }}
  ' /proc/[0-9]*/stat 2>/dev/null
}}
set -- $(read_cpu_stat)
user=${{1:-0}}
nice=${{2:-0}}
system=${{3:-0}}
idle=${{4:-0}}
iowait=${{5:-0}}
irq=${{6:-0}}
softirq=${{7:-0}}
steal=${{8:-0}}
total1=$((user+nice+system+idle+iowait+irq+softirq+steal))
idle1=$((idle+iowait))
process_ticks_before_file="/tmp/fileterm-procs-before-$$"
process_ticks_after_file="/tmp/fileterm-procs-after-$$"
process_cpu_file="/tmp/fileterm-procs-cpu-$$"
process_cpu_tmp_file="/tmp/fileterm-procs-cpu-tmp-$$"
gpu_info_file="/tmp/fileterm-gpu-info-$$"
trap 'rm -f "$before_file" "$after_file" "$process_ticks_before_file" "$process_ticks_after_file" "$process_cpu_file" "$process_cpu_tmp_file" "$gpu_info_file"' 0 1 2 15
read_process_ticks > "$process_ticks_before_file"
sleep "$sleep_interval"
read_process_ticks > "$process_ticks_after_file"
set -- $(read_cpu_stat)
user2=${{1:-0}}
nice2=${{2:-0}}
system2=${{3:-0}}
idle2=${{4:-0}}
iowait2=${{5:-0}}
irq2=${{6:-0}}
softirq2=${{7:-0}}
steal2=${{8:-0}}
total2=$((user2+nice2+system2+idle2+iowait2+irq2+softirq2+steal2))
idle2sum=$((idle2+iowait2))
diff_total=$((total2-total1))
diff_idle=$((idle2sum-idle1))
if [ "$diff_total" -gt 0 ]; then cpu_pct=$((100*(diff_total-diff_idle)/diff_total)); else cpu_pct=0; fi
cpu_user_pct=$(awk -v diff_total="$diff_total" -v before="$user" -v after="$user2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_system_pct=$(awk -v diff_total="$diff_total" -v before="$system" -v after="$system2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_nice_pct=$(awk -v diff_total="$diff_total" -v before="$nice" -v after="$nice2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_idle_pct=$(awk -v diff_total="$diff_total" -v before="$idle1" -v after="$idle2sum" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_iowait_pct=$(awk -v diff_total="$diff_total" -v before="$iowait" -v after="$iowait2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_irq_pct=$(awk -v diff_total="$diff_total" -v before="$irq" -v after="$irq2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_softirq_pct=$(awk -v diff_total="$diff_total" -v before="$softirq" -v after="$softirq2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
cpu_steal_pct=$(awk -v diff_total="$diff_total" -v before="$steal" -v after="$steal2" 'BEGIN {{ if (diff_total > 0) printf "%.1f", (after-before) * 100 / diff_total; else print "0.0" }}')
os_name=$( ( . /etc/os-release >/dev/null 2>&1 && printf "%s" "$PRETTY_NAME" ) 2>/dev/null )
[ -z "$os_name" ] && os_name=$(sed -n 's/^DISTRIB_DESCRIPTION=['"'"'"]\\{{0,1\\}}\\(.*\\)['"'"'"]\\{{0,1\\}}$/\\1/p' /etc/openwrt_release 2>/dev/null | head -n 1)
[ -z "$os_name" ] && os_name=$(uname -s 2>/dev/null)
kernel_name=$(uname -s 2>/dev/null)
kernel_version=$(uname -r 2>/dev/null)
architecture=$(uname -m 2>/dev/null)
hostname_value=$(hostname 2>/dev/null)
best_ip=""
best_ip_rank=99
rank_ip() {{
  case "$1" in
    10.*|192.168.*|172.1[6-9].*|172.2[0-9].*|172.3[0-1].*)
      echo 1
      ;;
    fc*:*|fd*:*)
      echo 2
      ;;
    100.6[4-9].*|100.[7-9][0-9].*|100.1[0-1][0-9].*|100.12[0-7].*)
      echo 3
      ;;
    *:*)
      echo 5
      ;;
    *)
      echo 4
      ;;
  esac
}}
consider_ip() {{
  candidate="$1"
  [ -z "$candidate" ] && return
  candidate=${{candidate%%/*}}
  case "$candidate" in
    127.*|169.254.*|::1|fe80:*)
      return
      ;;
  esac
  rank=$(rank_ip "$candidate")
  if [ "$rank" -lt "$best_ip_rank" ]; then
    best_ip="$candidate"
    best_ip_rank="$rank"
  fi
}}
is_virtual_iface() {{
  case "$1" in
    tailscale*|zt*|zerotier*|docker*|veth*|virbr*|br-*|cni*|flannel*|tun*|tap*|wg*|vethernet*)
      return 0
      ;;
  esac
  return 1
}}
default_ifaces=$(
  {{
    ip route show default 2>/dev/null | awk '{{for (i=1; i<=NF; i++) if ($i == "dev") print $(i+1)}}'
    awk '$2 == "00000000" {{print $1}}' /proc/net/route 2>/dev/null
  }} | awk 'NF && !seen[$0]++'
)
for iface in $default_ifaces; do
  is_virtual_iface "$iface" && continue
  for candidate in $(ip -o -4 addr show dev "$iface" scope global 2>/dev/null | awk '{{print $4}}'); do
    consider_ip "$candidate"
  done
  for candidate in $(ifconfig "$iface" 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {{print $2}} /inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {{sub("addr:", "", $2); print $2}}'); do
    consider_ip "$candidate"
  done
done
for candidate in $(ip route get 1 2>/dev/null | awk 'NR==1 {{for (i=1; i<=NF; i++) if ($i == "src") {{print $(i+1)}}}}'); do
  consider_ip "$candidate"
done
for candidate in $(hostname -I 2>/dev/null); do
  consider_ip "$candidate"
done
for candidate in $(ip -o addr show up scope global 2>/dev/null | awk '{{print $4}}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet / && $2 !~ /^127\\./ {{print $2}}'); do
  consider_ip "$candidate"
done
for candidate in $(ifconfig 2>/dev/null | awk '/inet addr:/ && $2 !~ /127\\.0\\.0\\.1/ {{sub("addr:", "", $2); print $2}}'); do
  consider_ip "$candidate"
done
ip="$best_ip"
uptime_seconds=$(awk '{{print int($1)}}' /proc/uptime 2>/dev/null)
if [ -z "$uptime_seconds" ]; then
  uptime_seconds=$(uptime 2>/dev/null | awk '
    /day/ {{
      for (i=1; i<=NF; i++) {{
        if ($i ~ /day/) days=$(i-1)
      }}
    }}
    {{
      if (match($0, /[0-9]+:[0-9]+/)) {{
        split(substr($0, RSTART, RLENGTH), time_parts, ":")
        hours=time_parts[1]
        minutes=time_parts[2]
      }}
      printf "%d", (days * 86400) + (hours * 3600) + (minutes * 60)
      exit
    }}
  ')
fi
load=$(awk '{{printf "%s, %s, %s", $1, $2, $3}}' /proc/loadavg 2>/dev/null)
if [ -z "$load" ]; then
  load=$(uptime 2>/dev/null | sed -n 's/.*load averages\\{{0,1\\}}: *//p; s/.*load average: *//p' | awk -F',' 'NF>=3 {{gsub(/^ +| +$/, "", $1); gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); printf "%s, %s, %s", $1, $2, $3; exit}}')
fi
mem_bytes=$(awk 'BEGIN {{ total=available=memfree=buffers=cached=shmem=anonpages=sreclaimable=slab=kernelstack=pagetables=0 }}
  /^MemTotal:/ {{ total=$2 * 1024 }}
  /^MemAvailable:/ {{ available=$2 * 1024 }}
  /^MemFree:/ {{ memfree=$2 * 1024 }}
  /^Buffers:/ {{ buffers=$2 * 1024 }}
  /^Cached:/ {{ cached=$2 * 1024 }}
  /^Shmem:/ {{ shmem=$2 * 1024 }}
  /^AnonPages:/ {{ anonpages=$2 * 1024 }}
  /^SReclaimable:/ {{ sreclaimable=$2 * 1024 }}
  /^Slab:/ {{ slab=$2 * 1024 }}
  /^KernelStack:/ {{ kernelstack=$2 * 1024 }}
  /^PageTables:/ {{ pagetables=$2 * 1024 }}
  END {{
    if (available == 0) available=memfree+buffers+cached+sreclaimable-shmem
    if (available < 0) available=0
    if (total > 0) {{
      used=total-available
      if (used < 0) used=0
      percent=int(used*100/total)
      kernel_total=slab-sreclaimable+kernelstack+pagetables
      if (kernel_total < 0) kernel_total=0
      kernel=kernel_total
      if (kernel > used) kernel=used
      remaining=used-kernel
      app=anonpages+shmem
      if (app > remaining) app=remaining
      if (app < 0) app=0
      cache=remaining-app
      if (cache < 0) cache=0
      printf "%.0f|%.0f|%.0f|%d|%.0f|%.0f|%.0f", used, total, available, percent, app, cache, kernel
    }}
  }}' /proc/meminfo 2>/dev/null)
if [ -z "$mem_bytes" ]; then
  mem_bytes=$(free 2>/dev/null | awk '/^Mem:/ {{
    total=$2 * 1024
    used=$3 * 1024
    available=$7 * 1024
    if (available == 0) available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d|0|0|0", used, total, available, percent
    exit
  }}')
fi
mem=$(printf "%s" "$mem_bytes" | awk -F'|' 'NF >= 4 {{printf "%d|%d|%d|%d|%d|%d", $1/1024/1024, $2/1024/1024, $4, $5/1024/1024, $6/1024/1024, $7/1024/1024}}')
swap_bytes=$(awk 'BEGIN {{ total=free=0 }}
  /^SwapTotal:/ {{ total=$2 * 1024 }}
  /^SwapFree:/ {{ free=$2 * 1024 }}
  END {{
    used=total-free
    if (used < 0) used=0
    available=free
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
  }}' /proc/meminfo 2>/dev/null)
if [ -z "$swap_bytes" ]; then
  swap_bytes=$(free 2>/dev/null | awk '/^Swap:/ {{
    total=$2 * 1024
    used=$3 * 1024
    available=total-used
    percent=(total>0 ? int(used*100/total) : 0)
    printf "%.0f|%.0f|%.0f|%d", used, total, available, percent
    exit
  }}')
fi
swap=$(printf "%s" "$swap_bytes" | awk -F'|' 'NF >= 4 {{printf "%d|%d|%d", $1/1024/1024, $2/1024/1024, $4}}')
logical_cpu_count=$(getconf _NPROCESSORS_ONLN 2>/dev/null)
case "$logical_cpu_count" in
  ''|*[!0-9]*|0) logical_cpu_count=$(nproc 2>/dev/null) ;;
esac
case "$logical_cpu_count" in
  ''|*[!0-9]*|0) logical_cpu_count=$(awk '/^processor[[:space:]]*:/ {{ count++ }} END {{ print count + 0 }}' /proc/cpuinfo 2>/dev/null) ;;
esac
cpu_info=$(awk -F: -v logical_cpu_count="$logical_cpu_count" '
  /^model name[[:space:]]*:/ || /^Hardware[[:space:]]*:/ || /^Processor[[:space:]]*:/ {{
    current=$2
    sub(/^[[:space:]]+/, "", current)
    if (current != "") {{
      model_order[++model_count]=current
      model_occurrences[current]++
      if (!seen[current]++) unique_model_count++
    }}
  }}
  /^cpu cores[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cores[current] == "") cores[current]=value
  }}
  /^cpu MHz[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (mhz[current] == "") mhz[current]=sprintf("%.3f", value + 0)
  }}
  /^cache size[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (cache[current] == "") cache[current]=value
  }}
  /^bogomips[[:space:]]*:/ || /^BogoMIPS[[:space:]]*:/ {{
    value=$2
    sub(/^[[:space:]]+/, "", value)
    if (bogomips[current] == "") bogomips[current]=value
  }}
  END {{
    for (row_index = 1; row_index <= model_count; row_index++) {{
      model=model_order[row_index]
      if (printed[model]) continue
      printed[model]=1
      resolved_cores=model_occurrences[model] + 0
      if (unique_model_count == 1 && logical_cpu_count + 0 > resolved_cores) resolved_cores=logical_cpu_count + 0
      if (resolved_cores == 0 && cores[model] != "") resolved_cores=cores[model] + 0
      printf "%s|%s|%s|%s|%s\n", model, resolved_cores, (mhz[model] == "" ? "-" : mhz[model]), (cache[model] == "" ? "-" : cache[model]), (bogomips[model] == "" ? "-" : bogomips[model])
    }}
  }}
' /proc/cpuinfo 2>/dev/null)
if [ -z "$cpu_info" ]; then
  cpu_info=$(LC_ALL=C lscpu 2>/dev/null | awk -F: '
    function trim(value) {{
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      return value
    }}
    /^Model name:/ {{ model=trim($2) }}
    /^Socket\\(s\\):/ {{ sockets=trim($2) + 0 }}
    /^Core\\(s\\) per socket:/ {{ cores_per_socket=trim($2) + 0 }}
    /^CPU\\(s\\):/ && total_cores == 0 {{ total_cores=trim($2) + 0 }}
    /^CPU max MHz:/ {{ frequency=trim($2) }}
    /^CPU MHz:/ && frequency == "" {{ frequency=trim($2) }}
    /^L3 cache:/ {{ cache=trim($2) }}
    /^L2 cache:/ && cache == "" {{ cache=trim($2) }}
    /^BogoMIPS:/ {{ bogomips=trim($2) }}
    END {{
      if (total_cores == 0 && sockets > 0 && cores_per_socket > 0) total_cores=sockets * cores_per_socket
      if (model != "") printf "%s|%s|%s|%s|%s\n", model, (total_cores > 0 ? total_cores : 0), (frequency == "" ? "-" : sprintf("%.3f", frequency + 0)), (cache == "" ? "-" : cache), (bogomips == "" ? "-" : bogomips)
    }}
  ')
fi
: > "$gpu_info_file"
nvidia_gpu_info=$(run_bounded 1 nvidia-smi --query-gpu=name,driver_version,memory.total,utilization.gpu,memory.used,temperature.gpu,power.draw,power.limit --format=csv,noheader,nounits 2>/dev/null | awk -F',' '
  function trim(value) {{
    sub(/^[[:space:]]+/, "", value)
    sub(/[[:space:]]+$/, "", value)
    return value
  }}
  function with_unit(value, unit) {{
    value=trim(value)
    return (value == "" || value == "-") ? "-" : value " " unit
  }}
  NF >= 3 {{
    model=trim($1)
    driver=trim($2)
    memory_total=trim($3)
    gpu_usage=trim($4)
    memory_used=trim($5)
    temperature=trim($6)
    power_usage=trim($7)
    power_limit=trim($8)
    printf "%s|NVIDIA|%s|%s|%s|%s|%s|%s|%s\n", model, (driver == "" ? "-" : driver), with_unit(memory_total, "MiB"), with_unit(gpu_usage, "%"), with_unit(memory_used, "MiB"), with_unit(temperature, "C"), with_unit(power_usage, "W"), with_unit(power_limit, "W")
  }}
')
if [ -n "$nvidia_gpu_info" ]; then
  printf "%s\n" "$nvidia_gpu_info" >> "$gpu_info_file"
fi

format_gpu_sysfs_bytes() {{
  value="$1"
  case "$value" in
    ''|*[!0-9]*) printf "%s" "-" ;;
    *) awk -v bytes="$value" 'BEGIN {{ if (bytes > 0) printf "%.0f MiB", bytes / 1024 / 1024; else print "-" }}' ;;
  esac
}}

format_gpu_microwatts() {{
  value="$1"
  case "$value" in
    ''|*[!0-9]*) printf "%s" "-" ;;
    *) awk -v microwatts="$value" 'BEGIN {{ if (microwatts > 0) printf "%.1f W", microwatts / 1000000; else print "-" }}' ;;
  esac
}}

read_gpu_memory_value() {{
  card="$1"
  field="$2"
  for candidate in \
    "$card/device/$field" \
    "$card/device/tile0/$field" \
    "$card/device/gt/gt0/$field"; do
    [ -r "$candidate" ] || continue
    raw=$(cat "$candidate" 2>/dev/null)
    case "$raw" in
      ''|*[!0-9]*) continue ;;
    esac
    formatted=$(format_gpu_sysfs_bytes "$raw")
    [ "$formatted" != "-" ] && {{
      printf "%s" "$formatted"
      return
    }}
  done
  printf "%s" "-"
}}

read_gpu_temperature_value() {{
  card="$1"
  for hwmon in "$card"/device/hwmon/hwmon*; do
    [ -r "$hwmon/temp1_input" ] || continue
    raw=$(cat "$hwmon/temp1_input" 2>/dev/null)
    case "$raw" in
      ''|*[!0-9-]*) continue ;;
    esac
    formatted=$(awk -v millidegrees="$raw" 'BEGIN {{ c=millidegrees / 1000; if (c > -50 && c < 150) printf "%.1f C", c }}')
    [ -n "$formatted" ] && {{
      printf "%s" "$formatted"
      return
    }}
  done
  printf "%s" "-"
}}

read_gpu_power_value() {{
  card="$1"
  mode="$2"
  for hwmon in "$card"/device/hwmon/hwmon*; do
    [ -d "$hwmon" ] || continue
    if [ "$mode" = "limit" ]; then
      power_files="power1_cap power1_max"
    else
      power_files="power1_average power1_input"
    fi
    for power_name in $power_files; do
      power_file="$hwmon/$power_name"
      [ -r "$power_file" ] || continue
      raw=$(cat "$power_file" 2>/dev/null)
      formatted=$(format_gpu_microwatts "$raw")
      [ "$formatted" != "-" ] && {{
        printf "%s" "$formatted"
        return
      }}
    done
  done
  printf "%s" "-"
}}

read_intel_gpu_usage() {{
  card="$1"
  card_number=$(printf "%s\n" "$card" | sed 's#.*/card##')
  if ! command -v intel_gpu_top >/dev/null 2>&1; then
    printf "%s" "-"
    return
  fi
  usage=$(run_bounded 2 intel_gpu_top -J -s 1000 -o - -d "drm:/dev/dri/card$card_number" 2>/dev/null | awk '
    {{
      line=$0
      while (match(line, /"busy"[[:space:]]*:[[:space:]]*[0-9]+([.][0-9]+)?/)) {{
        token=substr(line, RSTART, RLENGTH)
        sub(/^.*:[[:space:]]*/, "", token)
        value=token + 0
        if (value > maximum) maximum=value
        seen=1
        line=substr(line, RSTART + RLENGTH)
      }}
    }}
    END {{
      if (seen) {{
        if (maximum < 0) maximum=0
        if (maximum > 100) maximum=100
        printf "%.1f", maximum
      }}
    }}
  ')
  [ -n "$usage" ] && printf "%s%%" "$usage" || printf "%s" "-"
}}

# Linux DRM exposes vendor-independent card directories. AMD's amdgpu and
# Intel's i915/xe drivers expose busy, VRAM, hwmon temperature and power
# values there when the kernel/driver supports them. Keep NVIDIA rows from
# nvidia-smi and only use this path for NVIDIA when that query was unavailable.
for card in /sys/class/drm/card*; do
  [ -d "$card/device" ] || continue
  card_name=$(printf "%s\n" "$card" | sed 's#.*/card##')
  card_number="$card_name"
  case "$card_number" in
    ''|*[!0-9]*) continue ;;
  esac
  [ -r "$card/device/vendor" ] || continue
  vendor_id=$(cat "$card/device/vendor" 2>/dev/null)
  case "$vendor_id" in
    0x1002|0X1002) vendor="AMD" ;;
    0x8086|0X8086) vendor="Intel" ;;
    0x10de|0X10DE) vendor="NVIDIA" ;;
    *) vendor="-" ;;
  esac
  if [ "$vendor" = "NVIDIA" ] && [ -n "$nvidia_gpu_info" ]; then
    continue
  fi
  slot=$(sed -n 's/^PCI_SLOT_NAME=//p' "$card/device/uevent" 2>/dev/null | head -n 1)
  gpu_line=$(lspci -s "$slot" 2>/dev/null | head -n 1)
  model=$(printf "%s\n" "$gpu_line" | awk '
    {{
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+:[[:space:]]*/, "", line)
      sub(/[[:space:]]+\[[[:xdigit:]:]+\]$/, "", line)
      print line
      exit
    }}
  ')
  [ -n "$model" ] || model="$vendor GPU"
  driver=$(readlink "$card/device/driver" 2>/dev/null | sed 's#.*/##')
  [ -n "$driver" ] || driver="-"

  gpu_usage="-"
  if [ -r "$card/device/gpu_busy_percent" ]; then
    raw_usage=$(cat "$card/device/gpu_busy_percent" 2>/dev/null)
    case "$raw_usage" in
      ''|*[!0-9.]*) ;;
      *) gpu_usage=$(awk -v value="$raw_usage" 'BEGIN {{ if (value < 0) value=0; if (value > 100) value=100; printf "%.1f%%", value }}') ;;
    esac
  elif [ "$vendor" = "Intel" ]; then
    gpu_usage=$(read_intel_gpu_usage "$card")
  fi
  gpu_memory=$(read_gpu_memory_value "$card" "mem_info_vram_total")
  gpu_memory_used=$(read_gpu_memory_value "$card" "mem_info_vram_used")
  gpu_temperature=$(read_gpu_temperature_value "$card")
  gpu_power=$(read_gpu_power_value "$card" "current")
  gpu_power_limit=$(read_gpu_power_value "$card" "limit")
  printf "%s|%s|%s|%s|%s|%s|%s|%s|%s\n" \
    "$model" "$vendor" "$driver" "$gpu_memory" "$gpu_usage" "$gpu_memory_used" \
    "$gpu_temperature" "$gpu_power" "$gpu_power_limit" >> "$gpu_info_file"
done

if [ ! -s "$gpu_info_file" ]; then
  # Last-resort hardware discovery for systems without DRM sysfs.
  run_bounded 1 lspci 2>/dev/null | awk '
    BEGIN {{ IGNORECASE=1 }}
    /VGA compatible controller|3D controller|Display controller/ {{
      line=$0
      sub(/^[[:xdigit:]:.]+[[:space:]]+[^:]+: /, "", line)
      vendor=line
      sub(/[[:space:]].*$/, "", vendor)
      printf "%s|%s|-|-|-|-|-|-|-\n", line, (vendor == "" ? "-" : vendor)
    }}
  ' >> "$gpu_info_file"
fi
gpu_info=$(cat "$gpu_info_file" 2>/dev/null)
ifaces=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); if (name != "lo") {{ if (out != "") out=out ","; out=out name }}}} END {{print out}}' /proc/net/dev 2>/dev/null)
active_iface=$(awk '$2 == 00000000 {{print $1; exit}}' /proc/net/route 2>/dev/null)
[ -z "$active_iface" ] && active_iface=$(echo "$ifaces" | awk -F, '{{print $1}}')
rx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
tx1=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
before_file="/tmp/fileterm-if-before-$$"
after_file="/tmp/fileterm-if-after-$$"
awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\n", name, values[2], values[10]}}' /proc/net/dev 2>/dev/null > "$before_file"
sleep "$sleep_interval"
rx2=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[2]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
tx2=$(awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") sum += values[10]}} END {{printf "%.0f", sum+0}}' /proc/net/dev 2>/dev/null)
awk -F: 'NR>2 {{name=$1; gsub(/[[:space:]]/,"",name); split($2, values, /[[:space:]]+/); if (name != "lo") printf "%s|%.0f|%.0f\n", name, values[2], values[10]}}' /proc/net/dev 2>/dev/null > "$after_file"
sample_ms=$(awk -v interval="$sleep_interval" 'BEGIN {{ printf "%d", interval * 1000 }}')
[ -z "$sample_ms" ] && sample_ms=1000
rx_rate=$(awk -v before="$rx1" -v after="$rx2" -v ms="$sample_ms" 'BEGIN {{ if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }}')
tx_rate=$(awk -v before="$tx1" -v after="$tx2" -v ms="$sample_ms" 'BEGIN {{ if (ms > 0) printf "%d", (after-before) * 1000 / ms; else print 0 }}')
df_flags="-kP"
df -kPl / >/dev/null 2>&1 && df_flags="-kPl"
if has_bounded_runner; then
  df_output=$(run_bounded 2 df "$df_flags" 2>/dev/null)
else
  local_mounts=$(awk '
    $3 ~ /^(overlay|squashfs|tmpfs|ramfs|ext[234]|xfs|btrfs|f2fs|vfat|ubifs|jffs2|zfs)$/ && !seen[$2]++ {{ print $2 }}
  ' /proc/mounts 2>/dev/null | head -n 20)
  [ -z "$local_mounts" ] && local_mounts="/"
  df_output=$(df "$df_flags" $local_mounts 2>/dev/null)
fi
disk=$(printf "%s\n" "$df_output" | awk 'NR>1 {{printf "%s|%sK/%sK\n", $6, $4, $2}}' | head -n 12)
filesystems=$(printf "%s\n" "$df_output" | awk 'NR>1 {{printf "%s|%sK|%sK|%s|%sK|%s\n", $1, $2, $3, $5, $4, $6}}' | head -n 20)
# 进程采集：按两次 /proc/<pid>/stat 采样计算窗口内的瞬时 CPU 占用。
# ps 的 %CPU 是进程生命周期平均值，长时间运行的进程在突然升高时会严重漏报；
# 同时它按单核百分比返回，多核机器还会出现与总 CPU 仪表不一致的问题。
# 这里用进程 tick 增量 / 全局 CPU tick 增量，直接得到 0-100 的整机占比。
if [ -s "$process_ticks_before_file" ] && [ -s "$process_ticks_after_file" ] && [ "$diff_total" -gt 0 ]; then
  # A process delta larger than the global delta means the two /proc snapshots
  # are not comparable (PID reuse, a broken proc reader, or a too-short sample).
  # Reject the whole tick sample and use ps below rather than emitting values
  # such as thousands of percent.
  if awk -F'|' -v diff_total="$diff_total" '
    NR==FNR {{ before[$1]=$2; next }}
    {{
      if (!($1 in before)) next
      delta=$2-before[$1]
      if (delta < 0 || delta > diff_total) {{ invalid=1; next }}
      # Keep processes that consumed no CPU during this short sample too.
      # Filtering them out makes the top-process list randomly shrink to two
      # or three rows whenever fewer processes receive a tick in the window.
      printf "%s|%.4f\n", $1, delta * 100 / diff_total
      matched++
    }}
    END {{ if (matched == 0 || invalid) exit 1 }}
  ' "$process_ticks_before_file" "$process_ticks_after_file" > "$process_cpu_tmp_file"; then
    mv "$process_cpu_tmp_file" "$process_cpu_file"
  else
    rm -f "$process_cpu_tmp_file" "$process_cpu_file"
  fi
fi

if [ -s "$process_cpu_file" ]; then
  procs=$(ps -eo pid=,user=,rss=,pmem=,args= 2>/dev/null | awk -v cpu_file="$process_cpu_file" '
    BEGIN {{
      while ((getline line < cpu_file) > 0) {{
        split(line, values, "|")
        cpu[values[1]]=values[2] + 0
      }}
      close(cpu_file)
    }}
    NF >= 5 {{
      pid=$1
      if (!(pid in cpu)) next
      args=$5
      for (i=6; i<=NF; i++) args=args" "$i
      command_name=args
      sub(/^[[:space:]]*/, "", command_name)
      split(command_name, command_parts, /[[:space:]]+/)
      comm=command_parts[1]
      sub(/^.*\//, "", comm)
      if (comm == "ps" || comm == "awk" || comm == "bash" || comm == "sleep" || comm == "sh" || comm == "powershell" || comm == "pwsh") next
      if (cpu[pid] < 0 || cpu[pid] > 100) next
      row_count++
      scores[row_count]=cpu[pid]
      rows[row_count]=sprintf("%s|%s|%.1fM|%.1f|%s|%s", pid, $2, $3/1024, cpu[pid], $4, substr(args, 1, 200))
    }}
    END {{
      for (rank=1; rank<=40 && rank<=row_count; rank++) {{
        best=0
        best_score=-1
        for (i=1; i<=row_count; i++) {{
          if (!used[i] && scores[i] > best_score) {{
            best=i
            best_score=scores[i]
          }}
        }}
        if (best == 0) break
        print rows[best]
        used[best]=1
      }}
    }}
  ')
else
  # fallback：无法读取 /proc 进程 tick 或快照校验失败时，使用 ps 的
  # 生命周期平均值。ps 的 %CPU 按单核百分比返回，这里归一化到整机 0-100。
  if has_bounded_runner; then
    procs=$(run_bounded 1 ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 40 | awk -v logical_cpu_count="$logical_cpu_count" 'NF >= 6 {{rss=$3/1024; args=$6; for(i=7;i<=NF;i++) args=args" "$i; cpu=$4+0; if (logical_cpu_count + 0 > 0) cpu=cpu/logical_cpu_count; if (cpu < 0) cpu=0; if (cpu > 100) cpu=100; printf "%s|%s|%.1fM|%.1f|%s|%s\n", $1, $2, rss, cpu, $5, substr(args,1,200)}}')
  else
    procs=$(ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu 2>/dev/null | head -n 40 | awk -v logical_cpu_count="$logical_cpu_count" 'NF >= 6 {{rss=$3/1024; args=$6; for(i=7;i<=NF;i++) args=args" "$i; cpu=$4+0; if (logical_cpu_count + 0 > 0) cpu=cpu/logical_cpu_count; if (cpu < 0) cpu=0; if (cpu > 100) cpu=100; printf "%s|%s|%.1fM|%.1f|%s|%s\n", $1, $2, rss, cpu, $5, substr(args,1,200)}}')
  fi
fi
if [ -z "$procs" ]; then
  # fallback：极简 ps（如某些 BusyBox 不支持 --sort 或 -o args=）
  if has_bounded_runner; then
    procs=$(run_bounded 1 ps 2>/dev/null | head -n 40 | awk 'NR>1 && NF >= 5 {{printf "0|-|%.1fM|0|0|%s\n", $3/1024, $5}}')
  else
    procs=$(ps 2>/dev/null | head -n 40 | awk 'NR>1 && NF >= 5 {{printf "0|-|%.1fM|0|0|%s\n", $3/1024, $5}}')
  fi
fi
echo "__PLATFORM__{}"
echo "__OS__$os_name"
echo "__KERNEL_NAME__$kernel_name"
echo "__KERNEL_VERSION__$kernel_version"
echo "__ARCH__$architecture"
echo "__HOSTNAME__$hostname_value"
echo "__IP__$ip"
echo "__UPTIME__"
echo "__UPTIME_SECONDS__$uptime_seconds"
echo "__LOAD__$load"
echo "__CPU__$cpu_pct"
echo "__CPU_USAGE__$cpu_user_pct|$cpu_system_pct|$cpu_nice_pct|$cpu_idle_pct|$cpu_iowait_pct|$cpu_irq_pct|$cpu_softirq_pct|$cpu_steal_pct"
echo "__MEM__$mem"
echo "__MEM_BYTES__$mem_bytes"
echo "__SWAP__$swap"
echo "__SWAP_BYTES__$swap_bytes"
echo "__CPUINFO_START__"
echo "$cpu_info"
echo "__CPUINFO_END__"
echo "__GPUINFO_START__"
echo "$gpu_info"
echo "__GPUINFO_END__"
echo "__IFACES__$ifaces"
echo "__ACTIVE_IFACE__$active_iface"
echo "__RATES__$rx_rate|$tx_rate"
echo "__IFACE_RATES_START__"
awk -F'|' -v sample_ms="$sample_ms" '
  NR==FNR {{rx[$1]=$2; tx[$1]=$3; next}}
  NF >= 3 {{
    prev_rx=rx[$1]
    prev_tx=tx[$1]
    curr_rx=$2
    curr_tx=$3
    rx_rate=(curr_rx-prev_rx) * 1000 / sample_ms
    tx_rate=(curr_tx-prev_tx) * 1000 / sample_ms
    printf "%s|%.0f|%.0f|%d|%d\n", $1, curr_rx, curr_tx, rx_rate, tx_rate
  }}
' "$before_file" "$after_file"
rm -f "$before_file" "$after_file" "$process_ticks_before_file" "$process_ticks_after_file" "$process_cpu_file" "$process_cpu_tmp_file" "$gpu_info_file"
echo "__IFACE_RATES_END__"
echo "__DISK_START__"
echo "$disk"
echo "__DISK_END__"
echo "__FILESYSTEMS_START__"
echo "$filesystems"
echo "__FILESYSTEMS_END__"
echo "__PROCS_START__"
echo "$procs"
echo "__PROCS_END__"
echo "{}"
"#,
        platform, complete_marker
    )
}

/// PowerShell-based metrics script for Windows remotes.
/// Emits the same `__KEY__VALUE` markers as `build_posix_metrics_command`.
pub fn build_windows_metrics_command() -> String {
    r#"
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference = 'SilentlyContinue'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}

function Write-Metric([string]$Name, [object]$Value) {
    if ($null -eq $Value) { $Value = '' }
    Write-Output ('__' + $Name + '__' + [string]$Value)
}

function Format-FileTermBytes([double]$Bytes) {
    $culture = [Globalization.CultureInfo]::InvariantCulture
    if ($Bytes -ge 1TB) { return [string]::Format($culture, '{0:0.0} TB', $Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return [string]::Format($culture, '{0:0.0} GB', $Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return [string]::Format($culture, '{0:0.0} MB', $Bytes / 1MB) }
    if ($Bytes -ge 1KB) { return [string]::Format($culture, '{0:0.0} KB', $Bytes / 1KB) }
    return [string]::Format($culture, '{0:0} B', $Bytes)
}

function Get-CpuUsagePercent {
    # Get-Counter rejects sub-second SampleInterval values on Windows. Use the
    # .NET performance counter directly so the initial snapshot is useful and
    # does not silently collapse to 0% on localized Windows installations.
    try {
        $counter = New-Object Diagnostics.PerformanceCounter('Processor', '% Processor Time', '_Total')
        $null = $counter.NextValue()
        Start-Sleep -Milliseconds 500
        $value = [double]$counter.NextValue()
        if ($value -ge 0 -and $value -le 100) {
            return [Math]::Round($value)
        }
    } catch {}

    # Keep a CIM fallback for Server Core/minimal images where the performance
    # counter category is unavailable.
    try {
        $loads = @(Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue |
            Where-Object { $null -ne $_.LoadPercentage } |
            ForEach-Object { [double]$_.LoadPercentage })
        if ($loads.Count -gt 0) {
            return [Math]::Round((($loads | Measure-Object -Average).Average))
        }
    } catch {}

    return 0
}

$os = Get-CimInstance Win32_OperatingSystem
$cs = Get-CimInstance Win32_ComputerSystem
$cpu = Get-CimInstance Win32_Processor
$memTotal = [double]$os.TotalVisibleMemorySize * 1KB
$memFree  = [double]$os.FreePhysicalMemory  * 1KB
$memUsed  = $memTotal - $memFree
$memPct   = if ($memTotal -gt 0) { [Math]::Round($memUsed * 100 / $memTotal) } else { 0 }

$swapTotal = [double]$os.TotalVirtualMemorySize * 1KB
$swapFree  = [double]$os.FreeVirtualMemory      * 1KB
$swapUsed  = $swapTotal - $swapFree
$swapPct   = if ($swapTotal -gt 0) { [Math]::Round($swapUsed * 100 / $swapTotal) } else { 0 }

# CPU usage sampled over 0.5s
$cpuPct = Get-CpuUsagePercent
$logicalProcessorCount = [Math]::Max(1, [Environment]::ProcessorCount)
$systemLoad = [string]::Format(
    [Globalization.CultureInfo]::InvariantCulture,
    '{0:0.00}',
    ($cpuPct * $logicalProcessorCount) / 100
)

$hostname = $env:COMPUTERNAME
$ip = ''
$sshConnectionParts = @(([string]$env:SSH_CONNECTION).Trim() -split '\s+')
if ($sshConnectionParts.Count -ge 4) { $ip = [string]$sshConnectionParts[2] }
if (-not $ip) {
    $net = Get-NetIPConfiguration -ErrorAction SilentlyContinue | Where-Object { $_.IPv4DefaultGateway -ne $null } | Select-Object -First 1
    if ($net) { $ip = $net.IPv4Address.IPAddress }
}

$uptimeSec = 0
if ($os.LastBootUpTime) {
    $uptimeSec = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
}

$cpuCores = ($cpu | Measure-Object NumberOfLogicalProcessors -Sum).Sum
if (-not $cpuCores) { $cpuCores = 0 }
$cpuRows = @()
foreach ($processor in @($cpu)) {
    $cpuModel = ([string]$processor.Name).Trim()
    $cpuFrequency = if ([double]$processor.MaxClockSpeed -gt 0) { [string][int]$processor.MaxClockSpeed } else { '-' }
    $cacheParts = @()
    if ([double]$processor.L2CacheSize -gt 0) { $cacheParts += ('L2 ' + (Format-FileTermBytes ([double]$processor.L2CacheSize * 1KB))) }
    if ([double]$processor.L3CacheSize -gt 0) { $cacheParts += ('L3 ' + (Format-FileTermBytes ([double]$processor.L3CacheSize * 1KB))) }
    $cpuCache = if ($cacheParts.Count -gt 0) { $cacheParts -join ' / ' } else { '-' }
    $cpuRows += ('{0}|{1}|{2}|{3}|-' -f $cpuModel, $cpuCores, $cpuFrequency, $cpuCache)
}
if ($cpuRows.Count -eq 0) { $cpuRows += ('-|{0}|-|-|-' -f $cpuCores) }

function Convert-GpuMetricText([object]$Value, [string]$Unit) {
    $text = ([string]$Value).Trim()
    if (-not $text -or $text -eq '-' -or $text -match '^(?:\[?N/?A\]?|NA)$') { return '-' }
    return $text + ' ' + $Unit
}

function Get-GpuRuntimeMap {
    $runtime = @{}
    try {
        $runtimeLines = @(nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit --format=csv,noheader,nounits 2>$null)
        foreach ($line in $runtimeLines) {
            $parts = @(([string]$line) -split ',')
            if ($parts.Count -lt 7) { continue }
            $name = ([string]$parts[0]).Trim()
            if (-not $name) { continue }
            $runtime[$name.ToLowerInvariant()] = @{
                usage = Convert-GpuMetricText $parts[1] '%'
                memoryUsed = Convert-GpuMetricText $parts[2] 'MiB'
                memoryTotal = Convert-GpuMetricText $parts[3] 'MiB'
                temperature = Convert-GpuMetricText $parts[4] 'C'
                powerUsage = Convert-GpuMetricText $parts[5] 'W'
                powerLimit = Convert-GpuMetricText $parts[6] 'W'
            }
        }
    } catch {}
    return $runtime
}

function Get-GpuRows([object[]]$Adapters) {
    $runtime = Get-GpuRuntimeMap
    $rows = @()
    foreach ($adapter in @($Adapters)) {
        $gpuName = ([string]$adapter.Name).Trim()
        if (-not $gpuName) { continue }
        $gpuVendor = ([string]$adapter.AdapterCompatibility).Trim()
        if (-not $gpuVendor) { $gpuVendor = '-' }
        $gpuDriver = ([string]$adapter.DriverVersion).Trim()
        if (-not $gpuDriver) { $gpuDriver = '-' }
        $gpuMemory = if ([double]$adapter.AdapterRAM -gt 0) { Format-FileTermBytes ([double]$adapter.AdapterRAM) } else { '-' }
        $runtimeEntry = $null
        $gpuNameKey = $gpuName.ToLowerInvariant()
        if ($runtime.ContainsKey($gpuNameKey)) {
            $runtimeEntry = $runtime[$gpuNameKey]
        } else {
            foreach ($runtimeName in @($runtime.Keys)) {
                if ($gpuNameKey.Contains([string]$runtimeName) -or ([string]$runtimeName).Contains($gpuNameKey)) {
                    $runtimeEntry = $runtime[$runtimeName]
                    break
                }
            }
        }
        if ($runtimeEntry) {
            # Win32_VideoController.AdapterRAM is truncated to 4 GB on some
            # WDDM laptop drivers. nvidia-smi reports the physical VRAM, so
            # prefer its runtime total whenever it is available.
            if ($runtimeEntry.memoryTotal -ne '-') { $gpuMemory = $runtimeEntry.memoryTotal }
            $rows += ('{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}|{8}' -f $gpuName, $gpuVendor, $gpuDriver, $gpuMemory, $runtimeEntry.usage, $runtimeEntry.memoryUsed, $runtimeEntry.temperature, $runtimeEntry.powerUsage, $runtimeEntry.powerLimit)
        } else {
            $rows += ('{0}|{1}|{2}|{3}|-|-|-|-|-' -f $gpuName, $gpuVendor, $gpuDriver, $gpuMemory)
        }
    }
    return $rows
}

$gpuAdapters = @(Get-CimInstance Win32_VideoController)
$gpuRows = @(Get-GpuRows -Adapters $gpuAdapters)

$disks = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3'
$diskLines = @()
$fsLines = @()
foreach ($d in $disks) {
    $size = [double]$d.Size
    $free = [double]$d.FreeSpace
    $used = $size - $free
    $pct  = if ($size -gt 0) { [Math]::Round($used * 100 / $size) } else { 0 }
    $sizeStr = Format-FileTermBytes $size
    $usedStr = Format-FileTermBytes $used
    $freeStr = Format-FileTermBytes $free
    $diskLines += ('{0}|{1}/{2}' -f $d.DeviceID, $usedStr, $sizeStr)
    $fsLines   += ('{0}|{1}|{2}|{3}%|{4}|{5}' -f $d.DeviceID, $sizeStr, $usedStr, $pct, $freeStr, $d.DeviceID)
}

$procs = Get-Process | Sort-Object -Property WS -Descending | Select-Object -First 20
$procLines = @()
foreach ($p in $procs) {
    $memMB = [Math]::Round($p.WorkingSet64 / 1MB, 1)
    $procLines += ('{0}||{1}M|0|0|{2}' -f $p.Id, $memMB, $p.ProcessName)
}

$ifaces = (Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' } | Select-Object -ExpandProperty Name) -join ','
$rx1 = 0; $tx1 = 0
$ifStats = @{}
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $ifStats[$i.Name] = @{ rx = $i.ReceivedBytes; tx = $i.SentBytes }
    $rx1 += $i.ReceivedBytes
    $tx1 += $i.SentBytes
}
Start-Sleep -Milliseconds 500
$rx2 = 0; $tx2 = 0
$ifRates = @()
foreach ($i in (Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $rx2 += $i.ReceivedBytes
    $tx2 += $i.SentBytes
    $prev = $ifStats[$i.Name]
    if ($prev) {
        $rxRate = ($i.ReceivedBytes - $prev.rx) * 2
        $txRate = ($i.SentBytes   - $prev.tx) * 2
        $ifRates += ('{0}|{1}|{2}|{3}|{4}' -f $i.Name, $i.ReceivedBytes, $i.SentBytes, $rxRate, $txRate)
    }
}
$rxRate = ($rx2 - $rx1) * 2
$txRate = ($tx2 - $tx1) * 2

Write-Output ('__PLATFORM__windows')
Write-Output ('__OS__' + $os.Caption)
Write-Output ('__KERNEL_NAME__Windows')
Write-Output ('__KERNEL_VERSION__' + $os.Version)
Write-Output ('__ARCH__' + $env:PROCESSOR_ARCHITECTURE)
Write-Output ('__HOSTNAME__' + $hostname)
Write-Output ('__IP__' + $ip)
Write-Output '__UPTIME__'
Write-Output ('__UPTIME_SECONDS__' + $uptimeSec)
Write-Output ('__LOAD__' + $systemLoad)
Write-Output '__LOAD_UNIT__busy-logical-processors'
Write-Output ('__CPU__' + $cpuPct)
Write-Output ('__CPU_USAGE__{0}|{1}|0|{2}|0|0|0|0' -f $cpuPct, $cpuPct, [Math]::Max(0, 100 - $cpuPct))
Write-Output ('__MEM__{0}|{1}|{2}|0|0|0' -f [Math]::Round($memUsed / 1MB), [Math]::Round($memTotal / 1MB), $memPct)
Write-Output ('__MEM_BYTES__{0}|{1}|{2}|{3}|0|0|0' -f $memUsed, $memTotal, $memFree, $memPct)
Write-Output ('__SWAP__{0}|{1}|{2}' -f [Math]::Round($swapUsed / 1MB), [Math]::Round($swapTotal / 1MB), $swapPct)
Write-Output ('__SWAP_BYTES__{0}|{1}|{2}|{3}' -f $swapUsed, $swapTotal, $swapFree, $swapPct)
Write-Output '__CPUINFO_START__'
$cpuRows | ForEach-Object { Write-Output $_ }
Write-Output '__CPUINFO_END__'
Write-Output '__GPUINFO_START__'
$gpuRows | ForEach-Object { Write-Output $_ }
Write-Output '__GPUINFO_END__'
Write-Output ('__IFACES__' + $ifaces)
Write-Output '__ACTIVE_IFACE__all'
Write-Output ('__RATES__{0}|{1}' -f $rxRate, $txRate)
Write-Output '__IFACE_RATES_START__'
$ifRates | ForEach-Object { Write-Output $_ }
Write-Output '__IFACE_RATES_END__'
Write-Output '__DISK_START__'
$diskLines | ForEach-Object { Write-Output $_ }
Write-Output '__DISK_END__'
Write-Output '__FILESYSTEMS_START__'
$fsLines | ForEach-Object { Write-Output $_ }
Write-Output '__FILESYSTEMS_END__'
Write-Output '__PROCS_START__'
$procLines | ForEach-Object { Write-Output $_ }
Write-Output '__PROCS_END__'
Write-Output '__FILETERM_METRICS_COMPLETE__'
"#.to_string()
}

/// Builds a long-lived Windows collector. The first block is the full system
/// snapshot; later blocks reuse cached static data and warm performance
/// counters so CPU/memory/network samples are emitted on a fixed clock without
/// paying PowerShell/CIM startup cost on every refresh.
pub fn build_windows_streaming_metrics_command(interval_seconds: u64) -> String {
    let mut script = build_windows_metrics_command();
    script.push_str(
        r#"
$cpuCounter = $null
$memoryAvailableCounter = $null
try {
    $cpuCounter = New-Object Diagnostics.PerformanceCounter('Processor', '% Processor Time', '_Total')
    $memoryAvailableCounter = New-Object Diagnostics.PerformanceCounter('Memory', 'Available Bytes')
    $null = $cpuCounter.NextValue()
    $null = $memoryAvailableCounter.NextValue()
} catch {}

$previousNetworkStats = @{}
foreach ($item in @(Get-NetAdapterStatistics -ErrorAction SilentlyContinue)) {
    $previousNetworkStats[[string]$item.Name] = @{
        rx = [double]$item.ReceivedBytes
        tx = [double]$item.SentBytes
    }
}
$previousProcCpuTimes = @{}
$sampleClock = [Diagnostics.Stopwatch]::StartNew()
$previousNetworkSampleMs = [double]$sampleClock.ElapsedMilliseconds
$previousProcSampleMs = [double]$sampleClock.ElapsedMilliseconds
$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000
Write-Output '__FILETERM_METRICS_BLOCK__'

while ($true) {
    if ($cpuCounter) {
        try { $cpuPct = [Math]::Round($cpuCounter.NextValue()) } catch {}
    }
    if ($memoryAvailableCounter) {
        try { $memFree = [double]$memoryAvailableCounter.NextValue() } catch {}
    }
    $cpuPct = [Math]::Max(0, [Math]::Min(100, [double]$cpuPct))
    $memUsed = [Math]::Max(0, $memTotal - $memFree)
    $memPct = if ($memTotal -gt 0) { [Math]::Round($memUsed * 100 / $memTotal) } else { 0 }
    $systemLoad = [string]::Format(
        [Globalization.CultureInfo]::InvariantCulture,
        '{0:0.00}',
        ($cpuPct * $logicalProcessorCount) / 100
    )
    if ($os.LastBootUpTime) {
        $uptimeSec = [int]((Get-Date) - $os.LastBootUpTime).TotalSeconds
    }

    $networkNowMs = [double]$sampleClock.ElapsedMilliseconds
    $networkElapsedSeconds = [Math]::Max(0.001, ($networkNowMs - $previousNetworkSampleMs) / 1000)
    $previousNetworkSampleMs = $networkNowMs
    $rxRate = 0
    $txRate = 0
    $ifRates = @()
    $currentNetworkStats = @(Get-NetAdapterStatistics -ErrorAction SilentlyContinue)
    foreach ($item in $currentNetworkStats) {
        $name = [string]$item.Name
        $rxTotal = [double]$item.ReceivedBytes
        $txTotal = [double]$item.SentBytes
        $previous = $previousNetworkStats[$name]
        $itemRxRate = 0
        $itemTxRate = 0
        if ($previous) {
            $itemRxRate = [Math]::Max(0, ($rxTotal - [double]$previous.rx) / $networkElapsedSeconds)
            $itemTxRate = [Math]::Max(0, ($txTotal - [double]$previous.tx) / $networkElapsedSeconds)
        }
        $previousNetworkStats[$name] = @{ rx = $rxTotal; tx = $txTotal }
        $rxRate += $itemRxRate
        $txRate += $itemTxRate
        $ifRates += ('{0}|{1}|{2}|{3}|{4}' -f $name, $rxTotal, $txTotal, [Math]::Round($itemRxRate), [Math]::Round($itemTxRate))
    }

    $procSampleMs = [double]$sampleClock.ElapsedMilliseconds
    $procElapsedSeconds = [Math]::Max(0.001, ($procSampleMs - $previousProcSampleMs) / 1000)
    $previousProcSampleMs = $procSampleMs

    $procLines = @()
    $currentProcCpuTimes = @{}
    Get-Process -ErrorAction SilentlyContinue |
        Sort-Object -Property WorkingSet64 -Descending |
        Select-Object -First 20 |
        ForEach-Object {
            $memMB = [Math]::Round($_.WorkingSet64 / 1MB, 1)
            $currentCpu = if ($_.CPU) { [double]$_.CPU } else { 0 }
            $procId = [string]$_.Id
            $currentProcCpuTimes[$procId] = $currentCpu
            $prevCpu = $previousProcCpuTimes[$procId]
            $processCpuPct = if ($null -ne $prevCpu) { [Math]::Max(0, [Math]::Round((([double]$currentCpu - [double]$prevCpu) / $procElapsedSeconds) * 100 / $logicalProcessorCount, 1)) } else { 0 }
            $procLines += ('{0}||{1}M|{2}|0|{3}' -f $_.Id, $memMB, $processCpuPct, $_.ProcessName)
        }
    $previousProcCpuTimes = $currentProcCpuTimes
    $gpuRows = @(Get-GpuRows -Adapters $gpuAdapters)

    $waitMs = [Math]::Round($nextEmitMs - [double]$sampleClock.ElapsedMilliseconds)
    if ($waitMs -gt 0) { Start-Sleep -Milliseconds $waitMs }
    $nextEmitMs += 1000
    if ($nextEmitMs -le [double]$sampleClock.ElapsedMilliseconds) {
        $nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000
    }

    Write-Output '__PLATFORM__windows'
    Write-Output ('__OS__' + $os.Caption)
    Write-Output '__KERNEL_NAME__Windows'
    Write-Output ('__KERNEL_VERSION__' + $os.Version)
    Write-Output ('__ARCH__' + $env:PROCESSOR_ARCHITECTURE)
    Write-Output ('__HOSTNAME__' + $hostname)
    Write-Output ('__IP__' + $ip)
    Write-Output '__UPTIME__'
    Write-Output ('__UPTIME_SECONDS__' + $uptimeSec)
    Write-Output ('__LOAD__' + $systemLoad)
    Write-Output '__LOAD_UNIT__busy-logical-processors'
    Write-Output ('__CPU__' + $cpuPct)
    Write-Output ('__CPU_USAGE__0|{0}|0|{1}|0|0|0|0' -f $cpuPct, [Math]::Max(0, 100 - $cpuPct))
    Write-Output ('__MEM__{0}|{1}|{2}|0|0|0' -f [Math]::Round($memUsed / 1MB), [Math]::Round($memTotal / 1MB), $memPct)
    Write-Output ('__MEM_BYTES__{0}|{1}|{2}|{3}|0|0|0' -f $memUsed, $memTotal, $memFree, $memPct)
    Write-Output ('__SWAP__{0}|{1}|{2}' -f [Math]::Round($swapUsed / 1MB), [Math]::Round($swapTotal / 1MB), $swapPct)
    Write-Output ('__SWAP_BYTES__{0}|{1}|{2}|{3}' -f $swapUsed, $swapTotal, $swapFree, $swapPct)
    Write-Output '__CPUINFO_START__'
    $cpuRows | ForEach-Object { Write-Output $_ }
    Write-Output '__CPUINFO_END__'
    Write-Output '__GPUINFO_START__'
    $gpuRows | ForEach-Object { Write-Output $_ }
    Write-Output '__GPUINFO_END__'
    Write-Output ('__IFACES__' + $ifaces)
    Write-Output '__ACTIVE_IFACE__all'
    Write-Output ('__RATES__{0}|{1}' -f [Math]::Round($rxRate), [Math]::Round($txRate))
    Write-Output '__IFACE_RATES_START__'
    $ifRates | ForEach-Object { Write-Output $_ }
    Write-Output '__IFACE_RATES_END__'
    Write-Output '__DISK_START__'
    $diskLines | ForEach-Object { Write-Output $_ }
    Write-Output '__DISK_END__'
    Write-Output '__FILESYSTEMS_START__'
    $fsLines | ForEach-Object { Write-Output $_ }
    Write-Output '__FILESYSTEMS_END__'
    Write-Output '__PROCS_START__'
    $procLines | ForEach-Object { Write-Output $_ }
    Write-Output '__PROCS_END__'
    Write-Output '__FILETERM_METRICS_BLOCK__'
}
"#,
    );
    let interval_ms = interval_seconds.saturating_mul(1_000);
    script
        .replace(
            "$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + 1000",
            &format!("$nextEmitMs = [double]$sampleClock.ElapsedMilliseconds + {interval_ms}"),
        )
        .replace(
            "$nextEmitMs += 1000",
            &format!("$nextEmitMs += {interval_ms}"),
        )
}

pub fn build_windows_streaming_metrics_exec_command(
    interval_seconds: u64,
) -> Result<String, String> {
    let script = build_windows_streaming_metrics_command(interval_seconds);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(script.as_bytes())
        .map_err(|error| error.to_string())?;
    let compressed = encoder.finish().map_err(|error| error.to_string())?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(compressed);
    let loader = format!(
        "$b=[Convert]::FromBase64String('{encoded}');$m=New-Object IO.MemoryStream(,$b);$g=New-Object IO.Compression.GzipStream($m,[IO.Compression.CompressionMode]::Decompress);$r=New-Object IO.StreamReader($g,[Text.Encoding]::UTF8);& ([scriptblock]::Create($r.ReadToEnd()))"
    );
    let command = format!(
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"{loader}\""
    );
    if command.len() >= 8000 {
        return Err(format!(
            "Windows streaming metrics command exceeds cmd.exe safe length: {}",
            command.len()
        ));
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::{
        append_pty_prompt_window, build_posix_metrics_command, build_windows_metrics_command,
        build_windows_streaming_metrics_command, build_windows_streaming_metrics_exec_command,
        classify_posix_probe_body, classify_windows_probe_output, extend_with_cap,
        parse_system_metrics, pty_password_prompt_detected, EXEC_COMMAND_OUTPUT_CAP,
    };

    #[test]
    fn bounded_exec_output_marks_when_remote_data_is_discarded() {
        let mut output = vec![b'x'; EXEC_COMMAND_OUTPUT_CAP - 2];
        let mut capped = false;

        extend_with_cap(&mut output, b"abcd", &mut capped);
        extend_with_cap(&mut output, b"ignored", &mut capped);

        assert_eq!(output.len(), EXEC_COMMAND_OUTPUT_CAP);
        assert!(capped);
        assert_eq!(&output[EXEC_COMMAND_OUTPUT_CAP - 2..], b"ab");
    }

    #[test]
    fn pty_password_prompt_detection_handles_fragmented_prompts() {
        let mut window = Vec::new();
        append_pty_prompt_window(&mut window, b"Pass");
        assert!(!pty_password_prompt_detected(&window));
        append_pty_prompt_window(&mut window, b"word: ");
        assert!(pty_password_prompt_detected(&window));
        assert!(pty_password_prompt_detected("请输入密码：".as_bytes()));
        assert!(!pty_password_prompt_detected(b"uid=0(root) gid=0(root)"));
    }

    #[test]
    fn posix_metrics_command_emits_real_awk_line_breaks() {
        let command = build_posix_metrics_command("linux");

        assert!(command.contains(r#"printf "%s|%sK/%sK\n"#));
        // 进程输出格式：pid|user|rss(M)|pcpu(已归一化)|pmem|args
        assert!(command.contains(r#"printf "%s|%s|%.1fM|%.1f|%s|%s\n"#));
        assert!(command.contains("getconf _NPROCESSORS_ONLN"));
        assert!(command.contains("for (row_index = 1; row_index <= model_count; row_index++)"));
        assert!(!command.contains("for (index = 1; index <= model_count; index++)"));
        assert!(!command.contains(r#"printf "%s|%sK/%sK\\n"#));
        assert!(!command.contains(r#"printf "%.1fM|%s|%s|%s\\n"#));
    }

    #[test]
    fn posix_metrics_command_collects_amd_and_intel_drm_runtime_metrics() {
        let command = build_posix_metrics_command("linux");

        assert!(command.contains("nvidia_gpu_info"));
        assert!(command.contains("gpu_busy_percent"));
        assert!(command.contains("mem_info_vram_total"));
        assert!(command.contains("mem_info_vram_used"));
        assert!(command.contains("power1_average"));
        assert!(command.contains("power1_cap"));
        assert!(command.contains("0x1002|0X1002"));
        assert!(command.contains("0x8086|0X8086"));
        assert!(command.contains("i915/xe"));
        assert!(command.contains("intel_gpu_top -J -s 1000 -o - -d"));
    }

    #[cfg(unix)]
    #[test]
    fn posix_metrics_command_is_valid_sh_syntax() {
        let status = std::process::Command::new("sh")
            .args(["-n", "-c", &build_posix_metrics_command("linux")])
            .status()
            .expect("shell syntax checker should start");

        assert!(
            status.success(),
            "generated POSIX metrics script is invalid"
        );
    }

    #[test]
    fn posix_metrics_command_samples_instantaneous_process_cpu() {
        // 进程 CPU 必须使用 /proc tick 增量，不能依赖 ps 的生命周期平均值。
        let command = build_posix_metrics_command("linux");

        assert!(command.contains("read_process_ticks()"));
        assert!(command.contains("process_ticks_before_file"));
        assert!(command.contains("process_ticks_after_file"));
        assert!(command.contains("process_cpu_tmp_file"));
        assert!(command.contains("delta * 100 / diff_total"));
        assert!(command.contains("delta > diff_total"));
        assert!(
            command.contains("($1 in before) && delta >= 0")
                || command.contains("if (!($1 in before)) next")
        );
        assert!(command.contains("ps -eo pid=,user=,rss=,pmem=,args="));
        assert!(command.contains("rank<=40 && rank<=row_count"));
        assert!(command.contains("if (comm == \"ps\" || comm == \"awk\""));
        assert!(
            !command.contains("cpu_pct=(logical_cpu_count + 0 > 0) ? $4 / logical_cpu_count : $4")
        );
        assert!(command.contains("cpu=cpu/logical_cpu_count"));
        assert!(command.contains(r#"printf "%s|%s|%.1fM|%.1f|%s|%s\n""#));
    }

    #[test]
    fn parser_keeps_disk_and_process_rows_separate() {
        // 新格式：pid|user|rss(M)|pcpu|pmem|args
        // 解析器按输入顺序保留行；构造采集命令时由 shell 端按瞬时 CPU 排序。
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__DISK_START__\n/|10K/20K\n/dev|30K/40K\n__DISK_END__\n__PROCS_START__\n1|root|1.0M|0.1|0.5|/usr/lib/systemd/systemd\n2|root|2.0M|0.2|1.0|/usr/sbin/sshd -D\n__PROCS_END__\n",
            "linux",
        );

        assert_eq!(metrics["diskRows"].as_array().map(Vec::len), Some(2));
        assert_eq!(metrics["topProcesses"].as_array().map(Vec::len), Some(2));
        // 按到达顺序，第一行是 systemd
        assert_eq!(
            metrics["topProcesses"][0]["command"],
            "/usr/lib/systemd/systemd"
        );
        assert_eq!(metrics["topProcesses"][0]["pid"], 1);
        assert_eq!(metrics["topProcesses"][0]["user"], "root");
        assert_eq!(metrics["topProcesses"][0]["cpu"], "0.1");
    }

    #[test]
    fn parser_backfills_legacy_disk_rows_from_filesystem_rows() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__FILESYSTEMS_START__\n/dev/sda1|100 GB|40 GB|40%|60 GB|/\n__FILESYSTEMS_END__\n",
            "linux",
        );

        assert_eq!(metrics["diskRows"][0]["path"], "/");
        assert_eq!(metrics["diskRows"][0]["usage"], "60 GB/100 GB");
    }

    #[test]
    fn parser_filters_transient_collector_processes() {
        // ps/awk/bash 等采集器自身进程应被过滤，不显示给用户
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n100|root|1.0M|0.1|0.5|/usr/bin/sleep 1\n101|root|2.0M|0.2|1.0|/usr/sbin/nginx -g 'daemon off;'\n102|root|1.5M|0.3|0.8|ps -eo pid=,user=,rss=,pcpu=,pmem=,args= --sort=-pcpu\n__PROCS_END__\n",
            "linux",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0]["command"], "/usr/sbin/nginx -g 'daemon off;'");
    }

    #[test]
    fn parser_rejects_invalid_or_unbounded_process_cpu_samples() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__PROCS_START__\n100|root|1.0M|40666.7|0.5|/usr/bin/bad-sample\n101|root|2.0M|12.3|1.0|/usr/bin/valid\n102|root|3.0M|NaN|1.0|/usr/bin/nan-sample\n__PROCS_END__\n",
            "linux",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(procs.len(), 1);
        assert_eq!(procs[0]["pid"], 101);
        assert_eq!(procs[0]["cpu"], "12.3");
    }

    #[test]
    fn windows_metrics_command_emits_electron_compatible_load() {
        let command = build_windows_metrics_command();

        assert!(command.contains("($cpuPct * $logicalProcessorCount) / 100"));
        assert!(command.contains("Write-Output ('__LOAD__' + $systemLoad)"));
        assert!(command.contains("Write-Output '__LOAD_UNIT__busy-logical-processors'"));
        assert!(command.contains("Get-CpuUsagePercent"));
        assert!(!command.contains("-SampleInterval 0.3"));
        assert!(command.contains("Get-CimInstance Win32_VideoController"));
        assert!(command.contains("utilization.gpu"));
        assert!(command.contains("memory.used"));
        assert!(command.contains("temperature.gpu"));
        assert!(command.contains("$rows += ('{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}|{8}'"));
        assert!(command.contains("return $rows"));
        assert!(command.contains("$processor.L3CacheSize"));
        assert!(command.contains("$fsLines   +="));
        assert!(command.contains("prefer its runtime total"));
        assert!(command.contains("N/?A"));

        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__LOAD__1.25\n__LOAD_UNIT__busy-logical-processors\n",
            "windows",
        );
        assert_eq!(metrics["load"], "1.25");
        assert_eq!(metrics["loadUnit"], "busy-logical-processors");
    }

    #[test]
    fn parser_keeps_windows_static_hardware_and_filesystem_rows() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPUINFO_START__\n12th Gen Intel(R) Core(TM) i7-12700H|20|2300|L2 11.5 MB / L3 24.0 MB|-\n__CPUINFO_END__\n__GPUINFO_START__\nNVIDIA GeForce RTX 3070 Laptop GPU|NVIDIA|32.0.16.1047|4.0 GB\n__GPUINFO_END__\n__FILESYSTEMS_START__\nC:|400.1 GB|345.6 GB|86%|54.5 GB|C:\n__FILESYSTEMS_END__\n",
            "windows",
        );

        assert_eq!(metrics["cpuInfoRows"][0]["frequencyMHz"], "2300");
        assert_eq!(
            metrics["cpuInfoRows"][0]["cache"],
            "L2 11.5 MB / L3 24.0 MB"
        );
        assert_eq!(metrics["gpuInfoRows"][0]["vendor"], "NVIDIA");
        assert_eq!(metrics["gpuInfoRows"][0]["memory"], "4.0 GB");
        assert_eq!(metrics["fileSystemRows"][0]["mountPoint"], "C:");
        assert_eq!(metrics["fileSystemRows"][0]["usagePercent"], "86%");
    }

    #[test]
    fn parser_keeps_optional_gpu_runtime_metrics() {
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__GPUINFO_START__\nRTX 4090|NVIDIA|550.54|8.0 GB|75|4096 MiB|64 C|120.0 W|200.0 W\n__GPUINFO_END__\n",
            "linux",
        );

        let gpu = &metrics["gpuInfoRows"][0];
        assert_eq!(gpu["model"], "RTX 4090");
        assert_eq!(gpu["usagePercent"], 75.0);
        assert_eq!(gpu["memory"], "8.0 GB");
        assert_eq!(gpu["memoryUsed"], "4.0 GB");
        assert_eq!(gpu["memoryPercent"], 50.0);
        assert_eq!(gpu["temperatureCelsius"], 64.0);
        assert_eq!(gpu["powerUsage"], "120.0 W");
        assert_eq!(gpu["powerLimit"], "200.0 W");
    }

    #[test]
    fn parser_accepts_windows_gpu_units_and_prefers_runtime_vram_total() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__GPUINFO_START__\nNVIDIA GeForce RTX 3070 Laptop GPU|NVIDIA|32.0.16.1047|8192 MiB|49 %|920 MiB|49 C|19.37 W|-\n__GPUINFO_END__\n",
            "windows",
        );

        let gpu = &metrics["gpuInfoRows"][0];
        assert_eq!(gpu["usagePercent"], 49.0);
        assert_eq!(gpu["memory"], "8.0 GB");
        assert_eq!(gpu["memoryUsed"], "920.0 MB");
        assert_eq!(gpu["temperatureCelsius"], 49.0);
        assert_eq!(gpu["powerUsage"], "19.37 W");
        assert!(gpu["powerLimit"].is_null());
    }

    #[test]
    fn windows_streaming_metrics_reuses_warm_counters_on_a_fixed_clock() {
        let command = build_windows_streaming_metrics_command(1);

        assert!(command.contains("Diagnostics.PerformanceCounter('Processor'"));
        assert!(command.contains("$processCpuPct = if"));
        assert!(command
            .contains("'{0}||{1}M|{2}|0|{3}' -f $_.Id, $memMB, $processCpuPct, $_.ProcessName"));
        assert!(command.contains("Write-Output ('__CPU__' + $cpuPct)"));
        assert!(command.contains("$nextEmitMs += 1000"));
        assert!(command.contains("while ($true)"));
        assert!(command.matches("__FILETERM_METRICS_BLOCK__").count() >= 2);
        assert!(!command.contains("while ($true) {\n\n$ErrorActionPreference"));

        let low_frequency_command = build_windows_streaming_metrics_command(30);
        assert!(low_frequency_command.contains("$nextEmitMs += 30000"));

        let exec_command = build_windows_streaming_metrics_exec_command(1).unwrap();
        assert!(exec_command.len() < 8000);
        assert!(exec_command.contains("IO.Compression.GzipStream"));
    }

    #[test]
    fn parser_parses_windows_process_lines() {
        // Windows 发射端格式：pid||rss(M)|pcpu|pmem|ProcessName
        // 6 字段，user 为空，pmem 为 0
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n1234||256.5M|12.3|0|chrome\n5678||128.0M|5.0|0|code\n__PROCS_END__\n",
            "windows",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert!(
            !procs.is_empty(),
            "Windows top processes should not be empty"
        );
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0]["pid"], 1234);
        assert_eq!(procs[0]["command"], "chrome");
        assert_eq!(procs[0]["cpu"], "12.3");
        assert_eq!(procs[1]["pid"], 5678);
        assert_eq!(procs[1]["command"], "code");
    }

    #[test]
    fn parser_rejects_malformed_windows_process_lines() {
        // Regression for S1: the original Windows emitter produced 4-field
        // rows (memMB|cpuT|0|ProcessName) while the parser required ≥6
        // fields (pid|user|rss|pcpu|pmem|args). Malformed rows must be
        // dropped silently rather than crash the parser, and well-formed
        // rows in the same block must still come through.
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n256.5|12.3|0|chrome\n1234||256.5M|12.3|0|code\n__PROCS_END__\n",
            "windows",
        );

        let procs = metrics["topProcesses"].as_array().unwrap();
        assert_eq!(
            procs.len(),
            1,
            "malformed 4-field row must be dropped, well-formed 6-field row must survive"
        );
        assert_eq!(procs[0]["pid"], 1234);
        assert_eq!(procs[0]["command"], "code");
    }

    #[test]
    fn parser_handles_empty_windows_process_block() {
        let metrics = parse_system_metrics(
            "__PLATFORM__windows\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__PROCS_START__\n__PROCS_END__\n",
            "windows",
        );
        assert_eq!(
            metrics["topProcesses"].as_array().map(Vec::len),
            Some(0),
            "empty process block must parse to an empty list, not null"
        );
    }

    #[test]
    fn posix_probe_classifies_linux_and_busybox_variants() {
        assert_eq!(classify_posix_probe_body("Linux\n"), Some("linux"));
        // CRLF pollution is normalized by the caller, but the classifier is
        // tolerant of stray case differences.
        assert_eq!(classify_posix_probe_body("LINUX\n"), Some("linux"));
        assert_eq!(classify_posix_probe_body("busybox\n"), Some("busybox"));
        assert_eq!(classify_posix_probe_body("OpenWrt\n"), Some("busybox"));
    }

    #[test]
    fn posix_probe_classifies_darwin_so_macos_keeps_cwd_tracking() {
        // Regression for M1: without a darwin branch macOS remotes fell through
        // to the Windows probes and ended up as `unknown`, skipping the
        // POSIX CWD hook on the primary development platform.
        assert_eq!(classify_posix_probe_body("Darwin\n"), Some("darwin"));
        assert_eq!(classify_posix_probe_body("darwin\n"), Some("darwin"));
    }

    #[test]
    fn posix_probe_returns_none_for_unrecognized_bodies() {
        assert_eq!(classify_posix_probe_body(""), None);
        assert_eq!(classify_posix_probe_body("freebsd\n"), None);
        assert_eq!(classify_posix_probe_body("sunos\n"), None);
    }

    #[test]
    fn windows_probe_recognizes_ver_and_powershell_outputs() {
        assert_eq!(
            classify_windows_probe_output("Microsoft Windows [Version 10.0.19045.4291]"),
            Some("windows")
        );
        assert_eq!(classify_windows_probe_output("Win32NT"), Some("windows"));
        assert_eq!(classify_windows_probe_output("win32nt"), Some("windows"));
        assert_eq!(classify_windows_probe_output("linux\n"), None);
    }

    #[test]
    fn parser_tolerates_missing_block_end_marker() {
        // 远端脚本被截断、网络中断或 PTY 缓冲区超限时，结束标记可能丢失。
        // read_block 应取从起始标记到字符串结尾的内容作为容错数据，
        // 而不是静默返回空，导致整段采集结果丢失。
        let metrics = parse_system_metrics(
            "__PLATFORM__linux\n__CPU__10\n__MEM__1|2|50|0|0|0\n__MEM_BYTES__1048576|2097152|1048576|50|0|0|0\n__SWAP__0|0|0\n__SWAP_BYTES__0|0|0|0\n__CPU_USAGE__1|2|0|97|0|0|0|0\n__DISK_START__\n/|10K/20K\n/dev|30K/40K\n__DISK_END__\n__PROCS_START__\n1|root|1.0M|0.1|0.5|/usr/lib/systemd/systemd\n2|root|2.0M|0.2|1.0|/usr/sbin/sshd -D\n",
            "linux",
        );

        // __PROCS_END__ 缺失，但 topProcesses 仍应保留两条已采集记录
        assert_eq!(metrics["topProcesses"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            metrics["topProcesses"][0]["command"],
            "/usr/lib/systemd/systemd"
        );
        assert_eq!(metrics["topProcesses"][1]["command"], "/usr/sbin/sshd -D");
    }
}
