#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PrivilegedCommandKind {
    Sudo,
    Su,
}

#[derive(Debug)]
struct PreparedRemoteExec {
    command: String,
    stdin: Option<String>,
    request_pty: bool,
    kind: Option<PrivilegedCommandKind>,
    used_saved_password: bool,
    save_password: Option<(String, PrivilegedCommandKind, String)>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecResult {
    pub output: String,
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
    /// True when the command was sent through the visible network-device PTY.
    /// Network CLIs do not provide a reliable process exit code on this path.
    pub raw_terminal: bool,
    /// The isolated non-interactive channel detected a supported input prompt
    /// in its bounded output. This is only a routing hint for the Agent; no
    /// input value is ever collected or returned on this path.
    pub input_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_kind: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemoteCommandStartResult {
    pub command_id: String,
    pub tab_id: String,
    pub started_at: u64,
}

#[derive(Clone, Copy, Debug)]
enum RemoteExecMode {
    Synchronous,
    Background,
}

enum RemoteExecOperationResult {
    Completed(RemoteExecResult),
    Started(BackgroundRemoteCommandStartResult),
}

/// Run one explicit command through the normal FileTerm command boundary.
/// Server sessions use the independent exec channel; network-device sessions
/// retain the established raw-PTY fallback used by the renderer and Copilot.
pub async fn execute_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    match execute_remote_command_inner(app, request, true, None, RemoteExecMode::Synchronous).await? {
        RemoteExecOperationResult::Completed(result) => Ok(result),
        RemoteExecOperationResult::Started(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

/// Run one Copilot command with the request-scoped cancellation token wired
/// through approval, secure password input, the network-device wait, and the
/// isolated SSH exec task. External callers keep using the non-cancellable
/// wrapper above so their existing timeout/error semantics stay unchanged.
pub async fn execute_remote_command_cancellable(
    app: &AppHandle,
    request: RemoteExecRequest,
    cancellation: &CancellationToken,
) -> Result<RemoteExecResult, AppError> {
    match execute_remote_command_inner(
        app,
        request,
        true,
        Some(cancellation),
        RemoteExecMode::Synchronous,
    )
    .await?
    {
        RemoteExecOperationResult::Completed(result) => Ok(result),
        RemoteExecOperationResult::Started(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

/// Run one command through the MCP background route. Unlike the normal
/// FileTerm/Copilot route, this function never falls back to the visible PTY
/// for a network-device session; the caller must choose the explicit visible
/// terminal tool instead.
pub async fn execute_background_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<RemoteExecResult, AppError> {
    match execute_remote_command_inner(app, request, false, None, RemoteExecMode::Synchronous).await? {
        RemoteExecOperationResult::Completed(result) => Ok(result),
        RemoteExecOperationResult::Started(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

/// Cancellable counterpart used by the long-lived MCP bridge. The token is
/// passed into the SSH worker so cancellation terminates the existing exec
/// channel instead of merely abandoning the local response future.
pub async fn execute_background_remote_command_cancellable(
    app: &AppHandle,
    request: RemoteExecRequest,
    cancellation: &CancellationToken,
) -> Result<RemoteExecResult, AppError> {
    match execute_remote_command_inner(
        app,
        request,
        false,
        Some(cancellation),
        RemoteExecMode::Synchronous,
    )
    .await?
    {
        RemoteExecOperationResult::Completed(result) => Ok(result),
        RemoteExecOperationResult::Started(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

/// Start a long-running server command and return after the SSH exec channel
/// has accepted it. The caller must use the MCP background-command read tools
/// to observe completion; this function deliberately does not wait for the
/// command or retry it on a replacement channel.
pub async fn start_background_remote_command(
    app: &AppHandle,
    request: RemoteExecRequest,
) -> Result<BackgroundRemoteCommandStartResult, AppError> {
    match execute_remote_command_inner(app, request, false, None, RemoteExecMode::Background).await? {
        RemoteExecOperationResult::Started(result) => Ok(result),
        RemoteExecOperationResult::Completed(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

pub async fn start_background_remote_command_cancellable(
    app: &AppHandle,
    request: RemoteExecRequest,
    cancellation: &CancellationToken,
) -> Result<BackgroundRemoteCommandStartResult, AppError> {
    match execute_remote_command_inner(
        app,
        request,
        false,
        Some(cancellation),
        RemoteExecMode::Background,
    )
    .await?
    {
        RemoteExecOperationResult::Started(result) => Ok(result),
        RemoteExecOperationResult::Completed(_) => Err(AppError::Command(
            "Internal remote command mode mismatch".to_string(),
        )),
    }
}

async fn execute_remote_command_inner(
    app: &AppHandle,
    request: RemoteExecRequest,
    allow_network_device_fallback: bool,
    cancellation: Option<&CancellationToken>,
    mode: RemoteExecMode,
) -> Result<RemoteExecOperationResult, AppError> {
    check_cancellation(cancellation)?;
    let tab_id = validate_remote_exec_tab_id(&request.tab_id)?;
    let command = validate_remote_exec_command(&request.command)?;
    let requested_cwd = validate_remote_exec_cwd(request.cwd)?;

    if matches!(mode, RemoteExecMode::Background)
        && (request.save_sudo_password || request.save_su_password)
    {
        return Err(AppError::Command(
            BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED.to_string(),
        ));
    }
    let (default_timeout_ms, max_timeout_ms) = match mode {
        RemoteExecMode::Synchronous => (DEFAULT_REMOTE_EXEC_TIMEOUT_MS, MAX_REMOTE_EXEC_TIMEOUT_MS),
        RemoteExecMode::Background => (
            DEFAULT_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS,
            MAX_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS,
        ),
    };
    let timeout_ms = request
        .timeout_ms
        .unwrap_or(default_timeout_ms)
        .clamp(MIN_REMOTE_EXEC_TIMEOUT_MS, max_timeout_ms);
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?
    };
    if session_type != "ssh" {
        return Err(AppError::Command(
            "Remote command execution is only supported for SSH sessions".to_string(),
        ));
    }
    // External CLI/MCP callers do not carry the AI context revision, but they
    // still need the same target binding: a disconnect/reconnect while the
    // command is waiting must not make it run against a replacement session
    // that happens to reuse the same tab ID.
    let bound_session_revision = match request.expected_session_revision.clone() {
        Some(expected) => Some(expected),
        None => Some(state.ai_session_revision(&tab_id).await.to_string()),
    };
    ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref()).await?;
    let (cwd, profile_id, host, shell_user, network_device) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.connected {
            return Err(AppError::Command(
                "FileTerm SSH session is not connected".to_string(),
            ));
        }
        let network_device = session.device_mode.as_deref() == Some("network-device");
        if !network_device && !session.capabilities.shell_integration {
            return Err(AppError::Command(
                "Remote command execution is disabled for this SSH session".to_string(),
            ));
        }
        (
            requested_cwd.clone().or_else(|| {
                (!network_device)
                    .then(|| session.shell_cwd.clone())
                    .flatten()
            }),
            session.profile_id.clone(),
            session.access_host.clone(),
            session.shell_user.clone(),
            network_device,
        )
    };

    if network_device {
        if !allow_network_device_fallback {
            return Err(AppError::Command(
                NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED.to_string(),
            ));
        }
        let command = validate_network_device_command(&request.command)?;
        if requested_cwd.is_some() {
            return Err(AppError::Command(
                NETWORK_DEVICE_CWD_UNSUPPORTED.to_string(),
            ));
        }
        if request.sudo_password.is_some()
            || request.su_password.is_some()
            || request.save_sudo_password
            || request.save_su_password
        {
            return Err(AppError::Command(
                NETWORK_DEVICE_PRIVILEGE_UNSUPPORTED.to_string(),
            ));
        }
        ensure_visible_terminal_session_active(app, &tab_id).await?;
        return execute_network_device_command(
            &state,
            &tab_id,
            &command,
            timeout_ms,
            bound_session_revision.as_deref(),
            cancellation,
        )
        .await
        .map(RemoteExecOperationResult::Completed);
    }

    let initial_prepared = prepare_remote_exec(
        app,
        &profile_id,
        &command,
        request.sudo_password.clone(),
        request.su_password.clone(),
        request.save_sudo_password,
        request.save_su_password,
    );
    let prepared = match initial_prepared {
        Ok(prepared) => prepared,
        Err(error)
            if matches!(
                &error,
                AppError::Command(message)
                    if message == SUDO_PASSWORD_NEEDED || message == SU_PASSWORD_NEEDED
            ) =>
        {
            if !request.allow_local_privileged_prompt {
                return Err(error);
            }
            let kind = privileged_command_kind(&command).ok_or(error)?;
            let (password, save) = request_sudo_password_prompt(
                app,
                &state,
                &tab_id,
                bound_session_revision.as_deref(),
                kind,
                &host,
                shell_user.as_deref(),
                cwd.as_deref(),
                &command,
                request.privileged_prompt_notice.clone(),
                cancellation,
            )
            .await?;
            check_cancellation(cancellation)?;
            ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref())
                .await?;
            prepare_remote_exec(
                app,
                &profile_id,
                &command,
                matches!(kind, PrivilegedCommandKind::Sudo).then_some(password.clone()),
                matches!(kind, PrivilegedCommandKind::Su).then_some(password),
                matches!(kind, PrivilegedCommandKind::Sudo) && save,
                matches!(kind, PrivilegedCommandKind::Su) && save,
            )?
        }
        Err(error) => return Err(error),
    };

    ensure_expected_session_revision(&state, &tab_id, bound_session_revision.as_deref()).await?;

    let result = match mode {
        RemoteExecMode::Synchronous => {
            let result = crate::commands::send_worker_cmd_with_response_timeout_cancellable(
                app,
                &tab_id,
                Duration::from_millis(timeout_ms.saturating_add(5_000)),
                cancellation,
                |respond_to| WorkerCmd::ExecuteRemoteCommand {
                    command: prepared.command.clone(),
                    cwd: cwd.clone(),
                    timeout_ms,
                    stdin: prepared.stdin.clone(),
                    request_pty: prepared.request_pty,
                    cancellation: cancellation.cloned(),
                    respond_to,
                },
            )
            .await?;
            check_cancellation(cancellation)?;
            RemoteExecOperationResult::Completed(parse_remote_exec_result(result)?)
        }
        RemoteExecMode::Background => {
            let lifetime_cancellation = state
                .worker_controls
                .read()
                .await
                .get(&tab_id)
                .cloned()
                .ok_or_else(|| AppError::Storage("SSH worker is not running".to_string()))?
                .child_token();
            let result = crate::commands::send_worker_cmd_with_response_timeout_cancellable(
                app,
                &tab_id,
                Duration::from_secs(35),
                cancellation,
                |respond_to| WorkerCmd::StartBackgroundRemoteCommand {
                    command: prepared.command.clone(),
                    cwd: cwd.clone(),
                    timeout_ms,
                    stdin: prepared.stdin.clone(),
                    request_pty: prepared.request_pty,
                    start_cancellation: cancellation.cloned(),
                    lifetime_cancellation,
                    respond_to,
                },
            )
            .await?;
            check_cancellation(cancellation)?;
            RemoteExecOperationResult::Started(parse_background_remote_command_start(result)?)
        }
    };
    let RemoteExecOperationResult::Completed(parsed) = result else {
        return Ok(result);
    };
    if let Some(kind) = prepared.kind {
        if detect_privileged_auth_failure(&parsed.output, kind) {
            if prepared.used_saved_password {
                let clear_result = match kind {
                    PrivilegedCommandKind::Sudo => {
                        crate::services::profile_ops::set_sudo_password(app, &profile_id, None)
                    }
                    PrivilegedCommandKind::Su => {
                        crate::services::profile_ops::set_su_password(app, &profile_id, None)
                    }
                };
                if let Err(error) = clear_result {
                    crate::services::logging::warn(
                        app,
                        "security",
                        format!("failed to clear invalid saved privileged password: {error}"),
                    );
                }
            }
            return Err(AppError::Command(
                match kind {
                    PrivilegedCommandKind::Sudo => SUDO_AUTH_FAILURE,
                    PrivilegedCommandKind::Su => SU_AUTH_FAILURE,
                }
                .to_string(),
            ));
        }
    }
    if let Some((profile_id, kind, password)) = prepared.save_password.as_ref() {
        match kind {
            PrivilegedCommandKind::Sudo => {
                crate::services::profile_ops::set_sudo_password(app, profile_id, Some(password))?
            }
            PrivilegedCommandKind::Su => {
                crate::services::profile_ops::set_su_password(app, profile_id, Some(password))?
            }
        }
    }
    Ok(RemoteExecOperationResult::Completed(parsed))
}

fn parse_background_remote_command_start(
    value: Value,
) -> Result<BackgroundRemoteCommandStartResult, AppError> {
    let command_id = value
        .get("commandId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::Serialization("Background remote command id was invalid".to_string())
        })?
        .to_string();
    let tab_id = value
        .get("tabId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::Serialization("Background remote command tab id was invalid".to_string())
        })?
        .to_string();
    let started_at = value
        .get("startedAt")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            AppError::Serialization("Background remote command start time was invalid".to_string())
        })?;
    Ok(BackgroundRemoteCommandStartResult {
        command_id,
        tab_id,
        started_at,
    })
}

/// Resolve a visible-terminal target and require that it is the session the
/// workspace currently exposes as active. This is shared by the direct MCP
/// visible-command route and legacy visible command-template calls so an
/// external caller cannot write to a background tab by accident.
pub async fn ensure_visible_terminal_session_active(
    app: &AppHandle,
    raw_tab_id: &str,
) -> Result<String, AppError> {
    let (tab_id, _) = visible_terminal_session_context(app, raw_tab_id).await?;
    Ok(tab_id)
}

async fn visible_terminal_session_context(
    app: &AppHandle,
    raw_tab_id: &str,
) -> Result<(String, bool), AppError> {
    let tab_id = validate_remote_exec_tab_id(raw_tab_id)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let (root_tab_id, session_type) = {
        let tabs = state.tabs.read().await;
        let tab = tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        (
            tab.pane_root_tab_id
                .clone()
                .unwrap_or_else(|| tab.id.clone()),
            tab.session_type.clone(),
        )
    };
    let (connected, network_device) = {
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        (
            session.connected,
            session.device_mode.as_deref() == Some("network-device"),
        )
    };
    if session_type != "ssh" {
        return Err(AppError::Command(
            "Visible command execution is only supported for SSH sessions".to_string(),
        ));
    }
    if !connected {
        return Err(AppError::Command(
            "FileTerm SSH session is not connected".to_string(),
        ));
    }

    let active_tab_id = state.active_tab_id.read().await.clone();
    let active_pane_tab_id = state
        .active_pane_tab_id_by_root
        .read()
        .await
        .get(&root_tab_id)
        .cloned();
    let active_pane_matches = active_pane_tab_id
        .as_deref()
        .map_or(root_tab_id == tab_id, |active_id| active_id == tab_id);
    if active_tab_id.as_deref() != Some(root_tab_id.as_str()) || !active_pane_matches {
        return Err(AppError::Command(
            VISIBLE_TERMINAL_SESSION_NOT_ACTIVE.to_string(),
        ));
    }
    Ok((tab_id, network_device))
}

/// Send one explicit command to the already-active visible SSH terminal.
/// Unlike `execute_remote_command`, this route deliberately does not create a
/// separate exec channel or collect a command result for server sessions: the
/// terminal owns the prompt, echo, output and any follow-up interaction.
pub async fn execute_visible_terminal_command(
    app: &AppHandle,
    raw_tab_id: &str,
    raw_command: &str,
    timeout_ms: Option<u64>,
) -> Result<RemoteExecResult, AppError> {
    let command = validate_visible_terminal_command(raw_command)?;
    let timeout_ms = timeout_ms
        .unwrap_or(DEFAULT_REMOTE_EXEC_TIMEOUT_MS)
        .clamp(MIN_REMOTE_EXEC_TIMEOUT_MS, MAX_REMOTE_EXEC_TIMEOUT_MS);
    let (tab_id, network_device) = visible_terminal_session_context(app, raw_tab_id).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    let expected_session_revision = state.ai_session_revision(&tab_id).await.to_string();
    ensure_expected_session_revision(&state, &tab_id, Some(&expected_session_revision)).await?;
    if network_device {
        return execute_network_device_command(
            &state,
            &tab_id,
            &command,
            timeout_ms,
            Some(&expected_session_revision),
            None,
        )
        .await;
    }

    crate::commands::send_exact_active_terminal_input(
        &state,
        &tab_id,
        Some(&expected_session_revision),
        format!("{command}\r"),
    )
    .await?;
    Ok(RemoteExecResult {
        output: String::new(),
        exit_code: None,
        timed_out: false,
        output_truncated: false,
        raw_terminal: true,
        input_required: false,
        input_kind: None,
    })
}

/// Send one single-line native network-device command to the existing shell
/// PTY. There is no portable command delimiter or exit-code protocol for
/// Cisco/Huawei/Comware CLIs, so completion is inferred from a quiet period
/// after the PTY starts producing new transcript data, matching the raw-PTY
/// behavior used by Netcatty for network-device agent commands.
async fn execute_network_device_command(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    command: &str,
    timeout_ms: u64,
    expected_session_revision: Option<&str>,
    cancellation: Option<&CancellationToken>,
) -> Result<RemoteExecResult, AppError> {
    check_cancellation(cancellation)?;
    ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
    let transcript_before = terminal_transcript_snapshot(state, tab_id)
        .await
        .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
    check_cancellation(cancellation)?;
    crate::commands::send_exact_active_terminal_input(
        state,
        tab_id,
        expected_session_revision,
        format!("{command}\r"),
    )
    .await?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut latest_transcript = transcript_before.clone();
    let mut last_change_at = None;
    let mut timed_out = false;

    loop {
        check_cancellation(cancellation)?;
        // A reconnect can replace the terminal sender while this bounded
        // quiet-period wait is in progress. Abort as soon as the identity
        // changes so a stale request cannot wait out the whole timeout or
        // report output from the replacement session.
        ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
        if let Some(transcript) = terminal_transcript_snapshot(state, tab_id).await {
            if transcript != latest_transcript {
                latest_transcript = transcript;
                last_change_at = Some(Instant::now());
            }
            if last_change_at
                .is_some_and(|changed_at| changed_at.elapsed() >= NETWORK_DEVICE_RAW_IDLE_SETTLE)
            {
                break;
            }
        } else {
            return Err(AppError::Command(
                "FileTerm session was not found".to_string(),
            ));
        }

        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        if let Some(cancellation) = cancellation {
            tokio::select! {
                _ = cancellation.cancelled() => return Err(remote_exec_cancelled_error()),
                _ = sleep(NETWORK_DEVICE_RAW_POLL_INTERVAL) => {}
            }
        } else {
            sleep(NETWORK_DEVICE_RAW_POLL_INTERVAL).await;
        }
    }

    let transcript_after = terminal_transcript_snapshot(state, tab_id)
        .await
        .unwrap_or(latest_transcript);
    ensure_expected_session_revision(state, tab_id, expected_session_revision).await?;
    check_cancellation(cancellation)?;
    let output = terminal_transcript_delta(&transcript_before, &transcript_after);
    let (output, output_truncated) = truncate_network_device_output(output);

    Ok(RemoteExecResult {
        output,
        exit_code: None,
        timed_out,
        output_truncated,
        raw_terminal: true,
        input_required: false,
        input_kind: None,
    })
}

async fn terminal_transcript_snapshot(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
) -> Option<String> {
    state
        .sessions
        .read()
        .await
        .get(tab_id)
        .map(|session| session.terminal_transcript.clone())
}

fn terminal_transcript_delta(before: &str, after: &str) -> String {
    after
        .strip_prefix(before)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| after.to_string())
}

fn truncate_network_device_output(output: String) -> (String, bool) {
    if output.len() <= NETWORK_DEVICE_RAW_OUTPUT_BYTES {
        return (output, false);
    }
    let mut start = output.len() - NETWORK_DEVICE_RAW_OUTPUT_BYTES;
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    (output[start..].to_string(), true)
}

async fn ensure_expected_session_revision(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    expected_session_revision: Option<&str>,
) -> Result<(), AppError> {
    let Some(expected_session_revision) = expected_session_revision else {
        return Ok(());
    };
    let expected_session_revision = expected_session_revision.trim();
    if expected_session_revision.is_empty()
        || expected_session_revision.len() > 64
        || expected_session_revision.chars().any(char::is_control)
    {
        return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
    }
    let current_session_revision = state.ai_session_revision(tab_id).await.to_string();
    if current_session_revision != expected_session_revision {
        return Err(AppError::Command("AI_AUTO_MODE_TARGET_CHANGED".to_string()));
    }
    Ok(())
}
