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

    // Some restricted Debian installations allow `uname` but reject a login
    // shell (`sh -lc`) or print an unexpected login banner around it. Keep a
    // bare POSIX fallback so platform detection does not depend on shell
    // startup files. This is especially useful for hosted Debian 12 images.
    if let Ok(output) = exec_command(handle, "uname -s 2>/dev/null").await {
        let output = output.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(platform) = classify_posix_probe_body(&output) {
            return platform.to_string();
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
    if normalized.contains("freebsd") {
        return Some("freebsd");
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
    exec_command_internal(handle, cmd, None, false, None, None).await
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
    exec_command_internal(handle, cmd, None, false, Some(command_timeout), None).await
}

/// Run a command via the exec channel, write `stdin`, and retain the SSH
/// channel's exit status.
pub async fn exec_command_with_stdin_status<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
) -> Result<(String, Option<u32>), String> {
    exec_command_internal(handle, cmd, Some(stdin.as_bytes()), false, None, None)
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
    exec_command_internal(handle, cmd, None, true, None, None)
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
    exec_command_internal(handle, cmd, Some(stdin.as_bytes()), true, None, None)
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
        None,
    )
    .await
}

/// Like [`exec_command_with_stdin_status_timeout_detailed`], but keeps the
/// channel alive long enough to make a best-effort remote termination request
/// when the caller cancels. The signal ladder is deliberately performed on
/// the existing SSH channel; the command is never re-run on a replacement
/// channel.
pub async fn exec_command_with_stdin_status_timeout_detailed_cancellable<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: &str,
    request_pty: bool,
    command_timeout: Duration,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<ExecCommandResult, String> {
    exec_command_internal(
        handle,
        cmd,
        Some(stdin.as_bytes()),
        request_pty,
        Some(command_timeout),
        cancellation,
    )
    .await
}

/// Open and start an exec channel for a command whose output will be consumed
/// asynchronously by the MCP background-command registry. The request timeout
/// only covers channel setup; the command timeout is enforced by the registry
/// after this function returns.
pub(crate) async fn open_background_exec_channel<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: Option<&str>,
    request_pty: bool,
    startup_timeout: Duration,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<BackgroundExecChannel, String> {
    let deadline = Some(tokio::time::Instant::now() + startup_timeout);
    let channel = await_exec_stage(handle.channel_open_session(), deadline, cancellation).await?;
    if request_pty {
        if let Err(error) = await_exec_stage(
            channel.request_pty(
                true,
                "xterm-256color",
                80,
                24,
                0,
                0,
                &[
                    (russh::Pty::ECHO, 0),
                    (russh::Pty::ECHOE, 0),
                    (russh::Pty::ECHOK, 0),
                    (russh::Pty::ECHONL, 0),
                    (russh::Pty::TTY_OP_ISPEED, 115200),
                    (russh::Pty::TTY_OP_OSPEED, 115200),
                ],
            ),
            deadline,
            cancellation,
        )
        .await
        {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
    }
    if let Err(error) = await_exec_stage(channel.exec(true, cmd), deadline, cancellation).await {
        let _ = terminate_remote_exec(&channel).await;
        return Err(error);
    }

    let pending_pty_stdin = if request_pty {
        stdin.map(str::as_bytes).map(ToOwned::to_owned)
    } else if let Some(stdin) = stdin {
        if let Err(error) = await_exec_stage(channel.data(stdin.as_bytes()), deadline, cancellation).await {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
        if let Err(error) = await_exec_stage(channel.eof(), deadline, cancellation).await {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
        None
    } else {
        None
    };

    if cancellation.is_some_and(|token| token.is_cancelled()) {
        let _ = terminate_remote_exec(&channel).await;
        return Err("AI_REQUEST_CANCELLED".to_string());
    }
    let (reader, writer) = channel.split();
    Ok(BackgroundExecChannel {
        reader,
        writer,
        pending_pty_stdin,
    })
}

async fn exec_command_internal<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
    stdin: Option<&[u8]>,
    request_pty: bool,
    command_timeout: Option<Duration>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<ExecCommandResult, String> {
    if cancellation.is_some_and(|token| token.is_cancelled()) {
        return Err("AI_REQUEST_CANCELLED".to_string());
    }
    let deadline = command_timeout.map(|duration| tokio::time::Instant::now() + duration);
    let mut channel = await_exec_stage(handle.channel_open_session(), deadline, cancellation).await?;
    if request_pty {
        let result = await_exec_stage(
            channel.request_pty(
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
            ),
            deadline,
            cancellation,
        )
        .await;
        if let Err(error) = result {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
    }
    if let Err(error) = await_exec_stage(channel.exec(true, cmd), deadline, cancellation).await {
        let _ = terminate_remote_exec(&channel).await;
        return Err(error);
    }
    // `su` reads its password from the controlling PTY only after it has
    // emitted the prompt. Sending the password immediately after `exec`
    // races PAM on several OpenSSH/PAM combinations and leaves `su -c`
    // blocked forever. Keep PTY input pending until the prompt arrives;
    // non-PTY execs (notably `sudo -S`) continue to use pipe semantics.
    let mut pending_pty_stdin = if request_pty { stdin } else { None };
    if let Some(stdin) = stdin.filter(|_| !request_pty) {
        if let Err(error) = await_exec_stage(channel.data(stdin), deadline, cancellation).await {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
        if let Err(error) = await_exec_stage(channel.eof(), deadline, cancellation).await {
            let _ = terminate_remote_exec(&channel).await;
            return Err(error);
        }
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
                        let _ = terminate_remote_exec(&channel).await;
                        break;
                    }
                    _ = wait_for_exec_cancellation(cancellation) => {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err("AI_REQUEST_CANCELLED".to_string());
                    }
                }
            }
            (true, None) => {
                tokio::select! {
                    message = timeout(EXEC_CHANNEL_DRAIN_TIMEOUT, channel.wait()) => match message {
                        Ok(message) => message,
                        Err(_) => break,
                    },
                    _ = wait_for_exec_cancellation(cancellation) => {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err("AI_REQUEST_CANCELLED".to_string());
                    }
                }
            }
            (false, Some(deadline)) => {
                tokio::select! {
                    message = channel.wait() => message,
                    _ = tokio::time::sleep_until(deadline) => {
                        timed_out = true;
                        let _ = terminate_remote_exec(&channel).await;
                        break;
                    }
                    _ = wait_for_exec_cancellation(cancellation) => {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err("AI_REQUEST_CANCELLED".to_string());
                    }
                }
            }
            (false, None) => {
                tokio::select! {
                    message = channel.wait() => message,
                    _ = wait_for_exec_cancellation(cancellation) => {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err("AI_REQUEST_CANCELLED".to_string());
                    }
                }
            }
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
                    if let Err(error) =
                        await_exec_stage(channel.data(stdin), deadline, cancellation).await
                    {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err(error);
                    }
                    // A PTY is a terminal, not a pipe: SSH channel EOF does
                    // not reliably become stdin EOF. Send the terminal's VEOF
                    // byte after the password's newline so `su -c` observes
                    // the same end-of-input as Ctrl+D if it needs it.
                    if let Err(error) = await_exec_stage(
                        channel.data_bytes(vec![0x04]),
                        deadline,
                        cancellation,
                    )
                    .await
                    {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err(error);
                    }
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
                    if let Err(error) =
                        await_exec_stage(channel.data(stdin), deadline, cancellation).await
                    {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err(error);
                    }
                    if let Err(error) = await_exec_stage(
                        channel.data_bytes(vec![0x04]),
                        deadline,
                        cancellation,
                    )
                    .await
                    {
                        let _ = terminate_remote_exec(&channel).await;
                        return Err(error);
                    }
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

/// Keep cancellation responsive even for the non-cancellable legacy helper:
/// the `None` branch is a pending future and therefore never wins the select.
async fn wait_for_exec_cancellation(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) {
    if let Some(cancellation) = cancellation {
        cancellation.cancelled().await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn await_exec_stage<F, T, E>(
    future: F,
    deadline: Option<tokio::time::Instant>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let operation = async move {
        match deadline {
            Some(deadline) => timeout_at(deadline, future)
                .await
                .map_err(|_| "AI_REQUEST_TIMEOUT".to_string())?
                .map_err(|error| error.to_string()),
            None => future.await.map_err(|error| error.to_string()),
        }
    };
    tokio::select! {
        _ = wait_for_exec_cancellation(cancellation) => Err("AI_REQUEST_CANCELLED".to_string()),
        result = operation => result,
    }
}

/// Request termination on the channel that owns the command. SSH servers may
/// reject signal requests or interpret them differently, so this function
/// makes no claim that the remote process has already exited. Closing the
/// channel is the final cleanup step when the signal requests fail.
async fn terminate_remote_exec(
    channel: &russh::Channel<russh::client::Msg>,
) -> (usize, bool) {
    let mut signals_dispatched = 0;
    for signal in [russh::Sig::INT, russh::Sig::TERM, russh::Sig::KILL] {
        if matches!(
            timeout(EXEC_TERMINATION_SIGNAL_TIMEOUT, channel.signal(signal)).await,
            Ok(Ok(()))
        ) {
            signals_dispatched += 1;
        }
    }
    let channel_closed = matches!(
        timeout(EXEC_TERMINATION_SIGNAL_TIMEOUT, channel.close()).await,
        Ok(Ok(()))
    );
    (signals_dispatched, channel_closed)
}

/// The split-channel counterpart used after a background command has handed
/// its reader to the output pump. It deliberately uses the same bounded
/// INT→TERM→KILL→close ladder as the synchronous command path.
pub(crate) async fn terminate_exec_channel_writer(
    writer: &russh::ChannelWriteHalf<russh::client::Msg>,
) -> (usize, bool) {
    let mut signals_dispatched = 0;
    for signal in [russh::Sig::INT, russh::Sig::TERM, russh::Sig::KILL] {
        if matches!(
            timeout(EXEC_TERMINATION_SIGNAL_TIMEOUT, writer.signal(signal)).await,
            Ok(Ok(()))
        ) {
            signals_dispatched += 1;
        }
    }
    let channel_closed = matches!(
        timeout(EXEC_TERMINATION_SIGNAL_TIMEOUT, writer.close()).await,
        Ok(Ok(()))
    );
    (signals_dispatched, channel_closed)
}

const PTY_PROMPT_WINDOW_BYTES: usize = 2 * 1024;

fn append_pty_prompt_window(window: &mut Vec<u8>, chunk: &[u8]) {
    window.extend_from_slice(chunk);
    if window.len() > PTY_PROMPT_WINDOW_BYTES {
        let keep_from = window.len() - PTY_PROMPT_WINDOW_BYTES;
        window.drain(..keep_from);
    }
}

pub(crate) fn pty_password_prompt_detected(window: &[u8]) -> bool {
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
