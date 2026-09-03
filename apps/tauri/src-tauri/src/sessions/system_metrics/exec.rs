pub async fn probe_remote_platform<H: Handler>(handle: &Handle<H>) -> String {
    probe_remote_platform_for_session(handle, None).await
}

/// Probe the remote platform while attaching the tab id to diagnostic lines.
/// The context-free wrapper above keeps the pure protocol tests and other
/// callers stable; worker sessions use this variant so concurrent tabs can be
/// separated in the exported app log.
pub(crate) async fn probe_remote_platform_for_session<H: Handler>(
    handle: &Handle<H>,
    tab_id: Option<&str>,
) -> String {
    probe_remote_platform_for_session_with_transport(handle, tab_id)
        .await
        .platform
}

/// Build a single remote command for PTY-only SSH gateways.
///
/// KoKo-style gateways inspect the raw `exec` command before forwarding it to
/// the asset.  A command beginning with `bash --login` is the compatible
/// path, while writing a multi-kilobyte script to a PTY's stdin can be
/// truncated by the terminal line discipline or echoed into the metrics
/// stream.  Encode the script locally and let a login shell decode it on the
/// remote side.  Base64's standard alphabet does not contain a single quote,
/// so the payload is safe inside the outer `-c` shell string.
pub(crate) fn build_pty_login_shell_command(script: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    format!(
        "bash --login -c 'printf \"%s\" \"{encoded}\" | (base64 -d 2>/dev/null || base64 -D 2>/dev/null) | bash'"
    )
}

fn probe_command_supports_login_shell_wrapper(command: &str) -> bool {
    let normalized = command.trim_start().to_ascii_lowercase();
    !normalized.starts_with("powershell ")
        && !normalized.starts_with("pwsh ")
        && !normalized.starts_with("cmd /c")
}

/// Probe the remote platform and report whether the remote requires a PTY for
/// exec commands. The first attempt remains a regular non-PTY exec so normal
/// servers keep their separate stderr channel; a response such as
/// `No PTY requested.` triggers one bounded PTY retry and all following probes
/// reuse that transport.
pub(crate) async fn probe_remote_platform_for_session_with_transport<H: Handler>(
    handle: &Handle<H>,
    tab_id: Option<&str>,
) -> PlatformProbeResult {
    let mut request_pty = false;

    let (posix_result, used_pty) = run_probe_command(
        handle,
        "posix",
        "sh -lc 'printf \"__FILETERM_PROBE_START__\\n\"; uname -s 2>/dev/null; shell_exe=$(readlink /proc/$$/exe 2>/dev/null || readlink /bin/sh 2>/dev/null || true); case \"$shell_exe\" in *busybox*) printf \"busybox\\n\" ;; esac; if [ -f /etc/openwrt_release ]; then printf \"openwrt\\n\"; fi; if [ -r /etc/centos-release ] || [ -r /etc/redhat-release ] || [ -r /etc/fedora-release ]; then printf \"linux\\n\"; fi; printf \"__FILETERM_PROBE_END__\\n\"'",
        tab_id,
        request_pty,
    )
    .await;
    request_pty |= used_pty;
    if let Ok(result) = &posix_result {
        return_if_interactive_gateway!(&posix_result, tab_id, "posix", request_pty);
        // CRLF normalization — Windows remotes emit `\r\n` which would
        // pollute platform detection (e.g. `linux\r` fails `contains`).
        let output = result.output.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(body) = extract_probe_body(&output) {
            if let Some(platform) = classify_posix_probe_body(&body) {
                log_probe_message(
                    tab_id,
                    format!(
                        "probe=posix classified platform={platform} transport={}",
                        probe_transport_label(request_pty)
                    ),
                );
                return PlatformProbeResult::new(platform, request_pty);
            }
        }
    }

    // Some restricted Debian installations allow `uname` but reject a login
    // shell (`sh -lc`) or print an unexpected login banner around it. Keep a
    // bare POSIX fallback so platform detection does not depend on shell
    // startup files. This is especially useful for hosted Debian 12 images.
    let fallback_probe = "uname -s 2>/dev/null; if [ -r /etc/centos-release ] || [ -r /etc/redhat-release ] || [ -r /etc/fedora-release ]; then cat /etc/centos-release /etc/redhat-release /etc/fedora-release 2>/dev/null; fi";
    let (fallback_result, used_pty) = run_probe_command(
        handle,
        "posix-fallback",
        fallback_probe,
        tab_id,
        request_pty,
    )
    .await;
    request_pty |= used_pty;
    if let Ok(result) = &fallback_result {
        return_if_interactive_gateway!(&fallback_result, tab_id, "posix-fallback", request_pty);
        let output = result.output.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(platform) = classify_posix_probe_body(&output) {
            log_probe_message(
                tab_id,
                format!(
                    "probe=posix-fallback classified platform={platform} transport={}",
                    probe_transport_label(request_pty)
                ),
            );
            return PlatformProbeResult::new(platform, request_pty);
        }
    }

    // RHEL/CentOS 7 documents `/etc/redhat-release` as the release identity
    // source. Keep a direct `cat` probe after the generic POSIX probes so a
    // restricted login shell (or an image without a usable `sh -lc`) can
    // still be recognized without depending on `/etc/os-release`.
    let (release_result, used_pty) = run_probe_command(
        handle,
        "release-files",
        "cat /etc/redhat-release /etc/centos-release /etc/fedora-release /etc/os-release 2>/dev/null",
        tab_id,
        request_pty,
    )
    .await;
    request_pty |= used_pty;
    if let Ok(result) = &release_result {
        return_if_interactive_gateway!(&release_result, tab_id, "release-files", request_pty);
        let output = result.output.replace("\r\n", "\n").replace('\r', "\n");
        if let Some(platform) = classify_posix_probe_body(&output) {
            log_probe_message(
                tab_id,
                format!(
                    "probe=release-files classified platform={platform} transport={}",
                    probe_transport_label(request_pty)
                ),
            );
            return PlatformProbeResult::new(platform, request_pty);
        }
    }

    // Try Windows probes only after the POSIX family. Once a PTY-required
    // response has been observed, use PTY for these too; issuing more rejected
    // non-PTY requests only adds latency and can exhaust a jump host's
    // MaxSessions allowance.
    let windows_cmds = [
        "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "pwsh -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command \"[Environment]::OSVersion.Platform\"",
        "cmd /c ver",
    ];
    for cmd in &windows_cmds {
        let (result, used_pty) = run_probe_command(handle, cmd, cmd, tab_id, request_pty).await;
        request_pty |= used_pty;
        return_if_interactive_gateway!(&result, tab_id, cmd, request_pty);
        if let Ok(result) = &result {
            let output = result.output.replace("\r\n", "\n").replace('\r', "\n");
            if let Some(platform) = classify_windows_probe_output(&output) {
                log_probe_message(
                    tab_id,
                    format!(
                        "probe={cmd} classified platform={platform} transport={}",
                        probe_transport_label(request_pty)
                    ),
                );
                return PlatformProbeResult::new(platform, request_pty);
            }
        }
    }

    log_probe_message(
        tab_id,
        format!(
            "all probes failed; returning platform=unknown transport={}",
            probe_transport_label(request_pty)
        ),
    );
    PlatformProbeResult::new("unknown", request_pty)
}

/// Execute one platform probe with a transport fallback. The returned bool
/// records whether the result was obtained through a PTY (or whether a PTY
/// was required but the retry still failed), so the caller can carry that
/// compatibility decision into the persistent metrics channel.
async fn run_probe_command<H: Handler>(
    handle: &Handle<H>,
    label: &str,
    command: &str,
    tab_id: Option<&str>,
    prefer_pty: bool,
) -> (Result<ExecCommandResult, String>, bool) {
    if prefer_pty {
        let wrapped_command = probe_command_supports_login_shell_wrapper(command)
            .then(|| build_pty_login_shell_command(command));
        if let Some(wrapped_command) = wrapped_command.as_deref() {
            log_probe_message(
                tab_id,
                format!(
                    "probe={label} flow_role=target command prepared command_mode=login-shell-inline-b64 script_bytes={} command_bytes={}",
                    command.len(),
                    wrapped_command.len(),
                ),
            );
        }
        let result = exec_command_with_status_pty_detailed(
            handle,
            wrapped_command.as_deref().unwrap_or(command),
        )
        .await;
        log_probe_result(label, &result, tab_id, "exec-pty");
        return (result, true);
    }

    let result = exec_command_with_status_detailed(handle, command).await;
    log_probe_result(label, &result, tab_id, "exec");
    let requires_pty = match &result {
        Ok(result) => output_indicates_pty_required(&result.output),
        Err(error) => output_indicates_pty_required(error),
    };
    if !requires_pty {
        return (result, false);
    }

    log_probe_message(
        tab_id,
        format!(
            "probe={label} flow_role=target retrying transport=exec-pty reason=pty-required-response command_mode={}",
            if probe_command_supports_login_shell_wrapper(command) {
                "login-shell-inline-b64"
            } else {
                "direct"
            }
        ),
    );
    let wrapped_command = probe_command_supports_login_shell_wrapper(command)
        .then(|| build_pty_login_shell_command(command));
    if let Some(wrapped_command) = wrapped_command.as_deref() {
        log_probe_message(
            tab_id,
            format!(
                "probe={label} flow_role=target command prepared command_mode=login-shell-inline-b64 script_bytes={} command_bytes={}",
                command.len(),
                wrapped_command.len(),
            ),
        );
    }
    let pty_result = exec_command_with_status_pty_detailed(
        handle,
        wrapped_command.as_deref().unwrap_or(command),
    )
    .await;
    log_probe_result(label, &pty_result, tab_id, "exec-pty");
    (pty_result, true)
}

fn probe_transport_label(request_pty: bool) -> &'static str {
    if request_pty {
        "exec-pty"
    } else {
        "exec"
    }
}

/// Detect the benign response emitted by SSH servers that only execute
/// commands attached to a terminal. Keep this deliberately narrow: a generic
/// command error mentioning `pty` must not make every subsequent probe request
/// a PTY and change stderr semantics on otherwise compatible servers.
fn output_indicates_pty_required(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    [
        "no pty requested",
        "no pty was requested",
        "pty required",
        "requires a pty",
        "requires pty",
        "must request a pty",
        "must request pty",
        "request a pty",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

fn log_probe_scope(tab_id: Option<&str>) -> String {
    tab_id
        .map(|tab_id| format!("ssh-probe:{tab_id}"))
        .unwrap_or_else(|| "ssh-probe".to_string())
}

fn log_probe_message(tab_id: Option<&str>, message: impl AsRef<str>) {
    let scope = log_probe_scope(tab_id);
    crate::services::logging::debug_global(&scope, message);
}

fn log_probe_result(
    label: &str,
    result: &Result<ExecCommandResult, String>,
    tab_id: Option<&str>,
    transport: &str,
) {
    let scope = log_probe_scope(tab_id);
    match result {
        Ok(result) => crate::services::logging::debug_global(
            &scope,
            format!(
                "probe={label} flow_role=target transport={transport} result=ok exit_code={} timed_out={} output_truncated={} output={:?}",
                result
                    .exit_code
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                result.timed_out,
                result.output_truncated,
                probe_output_preview(&result.output),
            ),
        ),
        Err(error) => crate::services::logging::debug_global(
            &scope,
                format!(
                    "probe={label} flow_role=target transport={transport} result=error error={error}"
                ),
        ),
    }
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
    if normalized.contains("linux")
        || normalized.contains("centos")
        || normalized.contains("red hat")
        || normalized.contains("rhel")
        || normalized.contains("fedora")
        || normalized.contains("rocky linux")
        || normalized.contains("almalinux")
    {
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
    exec_command_with_status_pty_detailed(handle, cmd)
        .await
        .map(|result| (result.output, result.exit_code))
}

/// PTY variant of [`exec_command_with_status_detailed`] used by platform
/// detection. Keeping the truncation/timeout metadata lets the probe logger
/// describe the retry with the same fidelity as the regular transport.
pub(crate) async fn exec_command_with_status_pty_detailed<H: Handler>(
    handle: &Handle<H>,
    cmd: &str,
) -> Result<ExecCommandResult, String> {
    exec_command_internal(handle, cmd, None, true, None, None).await
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
    let channel = open_exec_channel_with_retry(handle, deadline, cancellation).await?;
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
    let mut channel = open_exec_channel_with_retry(handle, deadline, cancellation).await?;
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

/// Open one SSH exec channel with the bounded retry used by ssh-mcp's channel
/// retry layer. Only `channel_open_session` is retried: no PTY request, stdin
/// write, or command execution has happened yet, so this cannot duplicate a
/// command that the server may already have accepted.
async fn open_exec_channel_with_retry<H: Handler>(
    handle: &Handle<H>,
    deadline: Option<tokio::time::Instant>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<russh::Channel<russh::client::Msg>, String> {
    let mut last_error = None;
    for attempt in 0..EXEC_CHANNEL_OPEN_RETRY_ATTEMPTS {
        match await_exec_stage(handle.channel_open_session(), deadline, cancellation).await {
            Ok(channel) => return Ok(channel),
            Err(error) => {
                if !is_retryable_exec_channel_open_error(&error)
                    || attempt + 1 >= EXEC_CHANNEL_OPEN_RETRY_ATTEMPTS
                {
                    return Err(error);
                }
                last_error = Some(error);
                let delay = EXEC_CHANNEL_OPEN_RETRY_DELAY
                    .checked_mul((attempt + 1) as u32)
                    .unwrap_or(EXEC_CHANNEL_OPEN_RETRY_DELAY);
                wait_for_exec_channel_retry(delay, deadline, cancellation).await?;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "SSH exec channel open failed".to_string()))
}

fn is_retryable_exec_channel_open_error(error: &str) -> bool {
    !matches!(error, "AI_REQUEST_TIMEOUT" | "AI_REQUEST_CANCELLED")
}

async fn wait_for_exec_channel_retry(
    delay: Duration,
    deadline: Option<tokio::time::Instant>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<(), String> {
    if let Some(deadline) = deadline {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => Err("AI_REQUEST_TIMEOUT".to_string()),
            _ = tokio::time::sleep(delay) => Ok(()),
            _ = wait_for_exec_cancellation(cancellation) => Err("AI_REQUEST_CANCELLED".to_string()),
        }
    } else {
        tokio::select! {
            _ = tokio::time::sleep(delay) => Ok(()),
            _ = wait_for_exec_cancellation(cancellation) => Err("AI_REQUEST_CANCELLED".to_string()),
        }
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
