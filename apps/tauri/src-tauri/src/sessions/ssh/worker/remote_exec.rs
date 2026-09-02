
/// Execute one explicit remote command on an independent SSH exec channel.
/// The interactive PTY remains owned by the terminal, so an external CLI/MCP
/// call cannot steal terminal input or mix its output into the user's shell.
#[allow(clippy::too_many_arguments)]
fn spawn_remote_command(
    handle: &Arc<Handle<ClientHandler>>,
    command: String,
    cwd: Option<String>,
    timeout_ms: u64,
    stdin: Option<String>,
    request_pty: bool,
    cancellation: Option<tokio_util::sync::CancellationToken>,
    respond_to: oneshot::Sender<Result<Value, String>>,
) {
    let handle = Arc::clone(handle);
    let command = cwd
        .filter(|path| !path.trim().is_empty())
        .map(|path| format!("cd -- {} && {command}", shell_quote(path.trim())))
        .unwrap_or(command);
    let timeout_duration = Duration::from_millis(timeout_ms);
    tokio::spawn(async move {
        let result = crate::sessions::system_metrics::exec_command_with_stdin_status_timeout_detailed_cancellable(
            &handle,
            &command,
            stdin.as_deref().unwrap_or(""),
            request_pty,
            timeout_duration,
            cancellation.as_ref(),
        )
        .await;
        let result = match result {
            Ok(result) => {
                let input_kind =
                    detect_remote_exec_input_kind(&result.output).map(ToOwned::to_owned);
                let input_required = stdin.is_none() && input_kind.is_some();
                // This is only a redacted routing hint. A privileged exec
                // has already received its one-shot stdin and must not route
                // the prompt to a second input surface.
                Ok(serde_json::json!({
                    "output": result.output,
                    "exitCode": result.exit_code,
                    "timedOut": result.timed_out,
                    "outputTruncated": result.output_truncated,
                    "rawTerminal": false,
                    "inputRequired": input_required,
                    "inputKind": input_kind,
                }))
            }
            Err(error) => Err(error),
        };
        let _ = respond_to.send(result);
    });
}

/// Start a long-running command without tying its lifetime to the MCP bridge
/// response. The SSH channel is opened once and handed to the registry; later
/// reads and termination requests address that same command id, so a polling
/// timeout can never cause the deployment to be submitted a second time.
#[allow(clippy::too_many_arguments)]
fn spawn_background_remote_command(
    handle: &Arc<Handle<ClientHandler>>,
    tab_id: &str,
    command: String,
    cwd: Option<String>,
    timeout_ms: u64,
    stdin: Option<String>,
    request_pty: bool,
    start_cancellation: Option<tokio_util::sync::CancellationToken>,
    lifetime_cancellation: tokio_util::sync::CancellationToken,
    registry: Arc<crate::services::mcp::BackgroundRemoteCommandRegistry>,
    respond_to: oneshot::Sender<Result<Value, String>>,
) {
    let handle = Arc::clone(handle);
    let tab_id = tab_id.to_string();
    let command = cwd
        .filter(|path| !path.trim().is_empty())
        .map(|path| format!("cd -- {} && {command}", shell_quote(path.trim())))
        .unwrap_or(command);
    tokio::spawn(async move {
        let result = async {
            let channel = crate::sessions::system_metrics::open_background_exec_channel(
                &handle,
                &command,
                stdin.as_deref(),
                request_pty,
                Duration::from_secs(30),
                start_cancellation.as_ref(),
            )
            .await?;
            let started = registry
                .register(
                    tab_id,
                    now_millis(),
                    Duration::from_millis(timeout_ms),
                    channel.reader,
                    channel.writer,
                    channel.pending_pty_stdin,
                    lifetime_cancellation,
                )
                .await?;
            serde_json::to_value(started).map_err(|error| error.to_string())
        }
        .await;
        let _ = respond_to.send(result);
    });
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn remote_exec_input_kind(prompt: &str) -> &'static str {
    let lower = prompt.to_ascii_lowercase();
    if lower.contains("password")
        || prompt.contains("密码")
        || lower.contains("passphrase")
        || lower.contains("verification code")
        || lower.contains("one-time")
        || lower.contains("otp")
    {
        "secret"
    } else {
        "text"
    }
}

fn detect_remote_exec_input_kind(output: &str) -> Option<&'static str> {
    let visible = visible_shell_text(output).replace('\r', "\n");
    let candidate = visible
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())?
        .trim();
    let lower = candidate.to_ascii_lowercase();
    let needs_input = lower.contains("password")
        || candidate.contains("密码")
        || lower.contains("passphrase")
        || lower.contains("verification code")
        || lower.contains("one-time")
        || lower.contains("otp")
        || lower.contains("[y/n]")
        || lower.contains("[yes/no]")
        || lower.contains("(y/n)")
        || lower.contains("confirm")
        || candidate.contains("确认");
    needs_input.then(|| remote_exec_input_kind(candidate))
}
