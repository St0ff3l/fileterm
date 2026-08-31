async fn update_tab_status_and_emit(app: &AppHandle, tab_id: &str, status: WorkspaceTabStatus) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let connected = status.is_connected();
    let mut summary = "连接已断开".to_string();
    let mut transcript = String::new();
    let mut target_changed = false;
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = status;
        }
    }
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            target_changed = session.connected != connected;
            session.connected = connected;
            summary = session.summary.clone();
            transcript = session.terminal_transcript.clone();
        }
    }
    if target_changed {
        state.touch_ai_session_revision(tab_id).await;
    }
    let operation_state = if matches!(
        status,
        WorkspaceTabStatus::Error | WorkspaceTabStatus::Closed
    ) {
        crate::services::connection_operations::ConnectionOperationState::Failed {
            code: crate::services::connection_operations::FILETERM_CONNECTION_FAILED.to_string(),
        }
    } else {
        crate::services::connection_operations::ConnectionOperationState::Connecting
    };
    if !connected {
        state
            .connection_operations
            .publish_for_tab(tab_id, operation_state)
            .await;
    }
    let payload = serde_json::json!({
        "tabId": tab_id.to_string(),
        "summary": summary,
        "transcript": transcript,
        "connected": connected,
        "status": status,
    });
    let _ = app.emit("terminal:state", payload);

    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

/// Emit a terminal data chunk to the renderer and append it to the session
/// snapshot's `terminal_transcript` so later `terminal:state` / snapshot
/// refreshes surface the full history (handles the case where the renderer
/// missed the live terminal stream, e.g. during a fast-fail connect).
async fn emit_terminal_data(app: &AppHandle, tab_id: &str, chunk: &str) {
    // Keep SSH on the same publication path as Telnet and local sessions so
    // opt-in automatic session logging receives the actual PTY output too.
    crate::sessions::terminal::emit_terminal_data(app, tab_id, chunk).await;
}

/// Mirrors Electron's `followShellCwd`: only a confirmed shell CWD update may
/// move the file panel, and only while the user has Follow terminal enabled.
///
/// The first SFTP listing is intentionally detached from the terminal loop so
/// a slow server cannot block terminal input. Once the shell reports its CWD,
/// that detached request is stale even if it started with the same path that is
/// still visible in the snapshot. Do not let its result put the old directory
/// rows back under the new CWD path. The rows are still a safe fallback when
/// the shell and SFTP expose different path namespaces (for example, a
/// chrooted SFTP user whose shell reports `/volume1/homes/user`).
fn initial_remote_listing_matches_current_session(
    initial_remote_path: &str,
    current_remote_path: &str,
    shell_cwd: Option<&str>,
    follow_shell_cwd: bool,
) -> bool {
    current_remote_path == initial_remote_path
        && (!follow_shell_cwd
            || shell_cwd
                .map(|cwd| cwd == initial_remote_path)
                .unwrap_or(true))
}

fn initial_remote_listing_can_be_fallback(
    initial_listing_is_current: bool,
    initial_remote_path: &str,
    current_remote_path: &str,
    current_remote_files_empty: bool,
) -> bool {
    !initial_listing_is_current
        && current_remote_path == initial_remote_path
        && current_remote_files_empty
}

/// Return the SFTP paths that can represent a shell CWD.
///
/// SSH and SFTP do not necessarily see the same filesystem root. Synology is
/// a common example: the shell reports a physical path such as
/// `/volume2/homes/alice`, while SFTP may expose `/`, `/homes/alice`, or
/// `/alice` depending on the configured service root/chroot. Keep the
/// physical path first so ordinary Linux hosts retain their exact behaviour;
/// only a `NoSuchFile` result may advance to one of the namespace candidates.
///
/// The volume number is detected from the path instead of assuming
/// `/volume1`. Synology documents often use volume1 as an example, but the
/// actual volume is configurable when multiple volumes exist.
pub(crate) fn shell_cwd_sftp_path_candidates(shell_cwd: &str) -> Vec<String> {
    let normalized = if shell_cwd.is_empty() {
        "/".to_string()
    } else {
        let trimmed = shell_cwd.trim_end_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            trimmed.to_string()
        }
    };
    let mut candidates = vec![normalized.clone()];

    if let Some(prefix) = synology_volume_prefix(&normalized) {
        push_path_suffix_candidate(&mut candidates, &normalized, prefix);
        push_synology_home_candidates(&mut candidates, &normalized, prefix);
    }
    push_path_suffix_candidate(&mut candidates, &normalized, "/var/services");
    push_synology_home_candidates(&mut candidates, &normalized, "/var/services");

    candidates
}

/// Detect `/volumeN` without treating names such as `/volume10foo` as a
/// volume root. The actual volume number is intentionally not fixed.
fn synology_volume_prefix(path: &str) -> Option<&str> {
    const PREFIX: &str = "/volume";
    let suffix = path.strip_prefix(PREFIX)?;
    let digit_count = suffix
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    if suffix
        .as_bytes()
        .get(digit_count)
        .is_some_and(|byte| *byte != b'/')
    {
        return None;
    }
    Some(&path[..PREFIX.len() + digit_count])
}

fn push_path_suffix_candidate(candidates: &mut Vec<String>, path: &str, prefix: &str) {
    let Some(suffix) = path.strip_prefix(prefix) else {
        return;
    };
    if !suffix.is_empty() && !suffix.starts_with('/') {
        return;
    }
    let candidate = if suffix.is_empty() { "/" } else { suffix };
    push_candidate(candidates, candidate);
}

fn push_candidate(candidates: &mut Vec<String>, candidate: &str) {
    let candidate = if candidate.is_empty() { "/" } else { candidate };
    if !candidates.iter().any(|item| item == candidate) {
        candidates.push(candidate.to_string());
    }
}

/// Add the two deeper Synology user-home namespace shapes. This covers both
/// the shared-folder root (`/homes/user`) and a chroot rooted at that user's
/// home (`/`).
fn push_synology_home_candidates(candidates: &mut Vec<String>, path: &str, storage_prefix: &str) {
    let Some(storage_relative) = path.strip_prefix(storage_prefix) else {
        return;
    };
    if storage_relative != "/homes" && !storage_relative.starts_with("/homes/") {
        return;
    }

    let after_homes = &storage_relative["/homes".len()..];
    push_candidate(candidates, after_homes);

    let Some(user_and_rest) = after_homes.strip_prefix('/') else {
        return;
    };
    if user_and_rest.is_empty() {
        return;
    }
    let after_user = user_and_rest
        .find('/')
        .map(|user_end| &user_and_rest[user_end..])
        .unwrap_or("");
    push_candidate(candidates, after_user);
}

/// A path-listing error can safely trigger a namespace fallback. Permission,
/// timeout and protocol errors must remain visible instead of being hidden by
/// trying unrelated paths.
pub(crate) fn is_sftp_path_not_found_message(error: &str) -> bool {
    error.to_ascii_lowercase().contains("no such file")
}

async fn list_shell_cwd_dir(
    sftp: &SftpSession,
    shell_cwd: &str,
) -> Result<(Vec<Value>, String), String> {
    let candidates = shell_cwd_sftp_path_candidates(shell_cwd);
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        match list_dir(sftp, candidate).await {
            Ok(files) => return Ok((files, candidate.clone())),
            Err(error) => {
                let can_try_next =
                    index + 1 < candidates.len() && is_sftp_path_not_found_message(&error);
                last_error = Some(error);
                if !can_try_next {
                    break;
                }
            }
        }
    }
    Err(last_error.unwrap_or_else(|| format!("无法列出 Shell 当前目录 {shell_cwd}")))
}

#[allow(clippy::too_many_arguments)] // Protocol/session context is intentionally explicit at this async boundary.
async fn follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    cwd: String,
    sftp: Arc<RwLock<SftpSession>>,
    handle: Arc<Handle<ClientHandler>>,
    operation_timeout: Duration,
    file_access_mode: String,
    root_file_access_method: RootFileAccessMethod,
    sudo_user: Option<String>,
    sudo_password: Option<String>,
) {
    crate::services::logging::ssh_debug(
        &app,
        &tab_id,
        format!(
            "CWD follow scheduled cwd={cwd} mode={file_access_mode} method={root_file_access_method:?} target_user={} password_cached={}",
            sudo_user.as_deref().unwrap_or("root"),
            sudo_password.is_some(),
        ),
    );
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(&tab_id) else {
            return;
        };
        if session.shell_cwd.as_deref() != Some(cwd.as_str()) || !session.follow_shell_cwd {
            return;
        }
        session.remote_files_loading = true;
    }
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }

    // The SFTP session belongs to the login user. Once `sudo -i` has started
    // a root shell, following CWD through that channel silently remains in
    // the old user's view. Electron switches to its sudo shell path here.
    let listing = match timeout(operation_timeout, async {
        if file_access_mode == "root" {
            exec_list_dir_via_shell(
                &handle,
                &cwd,
                root_file_access_method,
                &sudo_user,
                &sudo_password,
            )
            .await
            .map(|files| (files, cwd.clone()))
        } else {
            // russh-sftp's client is one request stream. Serialise access to it:
            // concurrent read locks let multiple list/delete/upload requests
            // interleave and eventually time out after app focus is restored.
            // The timeout covers both waiting for the lock and SFTP read_dir.
            let sftp = sftp.write().await;
            list_shell_cwd_dir(&sftp, &cwd).await
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err(format!("跟随终端目录 {cwd} 超时")),
    };

    let follow_error = listing.as_ref().err().cloned();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    let Some(session) = sessions.get_mut(&tab_id) else {
        return;
    };
    session.remote_files_loading = false;
    if session.shell_cwd.as_deref() == Some(cwd.as_str()) && session.follow_shell_cwd {
        if let Ok((files, resolved_path)) = &listing {
            session.remote_path = resolved_path.clone();
            session.remote_files = files.clone();
        }
    }
    drop(sessions);

    if let Some(error) = follow_error {
        crate::services::logging::ssh_debug(
            &app,
            &tab_id,
            format!("CWD follow failed for {cwd}: {error}"),
        );
    } else if let Ok((_, resolved_path)) = listing.as_ref() {
        if resolved_path == &cwd {
            crate::services::logging::ssh_debug(
                &app,
                &tab_id,
                format!("CWD follow applied: {cwd}"),
            );
        } else {
            crate::services::logging::ssh_debug(
                &app,
                &tab_id,
                format!("CWD follow mapped shell={cwd} sftp={resolved_path}"),
            );
        }
    }

    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

/// Flush the batch buffer to the terminal output pump channel.
///
/// 非阻塞：用 `try_send` 把 chunk 推到 bounded channel，由独立的 pump
/// task 异步消费并推送到 renderer。通道满时丢弃 chunk 并限频记录——终端
/// 输出是尽力而为的，丢几帧不影响功能，但 worker 主循环的 select! 必须
/// 立即返回以保证 Ctrl+C 路径畅通。
fn flush_batch(
    batch: &mut Vec<u8>,
    output_tx: &tokio::sync::mpsc::Sender<String>,
    app: &AppHandle,
    tab_id: &str,
) {
    if batch.is_empty() {
        return;
    }
    let chunk = String::from_utf8_lossy(batch).into_owned();
    batch.clear();
    if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = output_tx.try_send(chunk) {
        // 通道满说明 pump task 跟不上（renderer IPC 或 RwLock 竞争）。
        // 丢弃 chunk 避免阻塞主循环。限频日志避免在极端高吞吐下刷屏。
        crate::services::logging::session(
            app,
            "WARN",
            "ssh",
            tab_id,
            "terminal output pump saturated, dropping chunk",
        );
    }
}

fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                result.push(hex);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn track_cwd_and_user(chunk: &str, buffer: &mut String) -> (Option<String>, Option<String>) {
    static CWD_OSC_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\x1b\]7;file://([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap()
    });
    static USER_OSC_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\x1b\]1337;RemoteUser=([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap()
    });

    buffer.push_str(chunk);
    if buffer.len() > 8192 {
        // 滚动窗口裁剪必须 char 边界安全：buffer 里有原始终端输出
        // （含中文），切片切到多字节字符内部会 panic 杀死 worker。
        trim_string_front(buffer, 4096);
    }

    let mut cwd = None;
    let mut user = None;

    for cap in CWD_OSC_PATTERN.captures_iter(buffer) {
        let raw_path = &cap[1];
        if let Some(slash_idx) = raw_path.find('/') {
            let path_part = &raw_path[slash_idx..];
            cwd = Some(percent_decode(path_part));
        }
    }
    for cap in USER_OSC_PATTERN.captures_iter(buffer) {
        user = Some(cap[1].to_string());
    }
    // The rolling buffer only exists to join an OSC sequence split across SSH
    // packets. Once complete markers have been consumed, discard them so a
    // later packet does not look like a fresh CWD/user event. Keeping only a
    // trailing, unterminated OSC sequence also lets the worker correlate a
    // CWD marker with the following RemoteUser marker when the two are split
    // across packets.
    retain_incomplete_osc_suffix(buffer);
    (cwd, user)
}

fn retain_incomplete_osc_suffix(buffer: &mut String) {
    let Some(start) = buffer.rfind("\x1b]") else {
        buffer.clear();
        return;
    };
    let suffix = &buffer[start + 2..];
    let terminated = suffix.contains('\u{7}') || suffix.contains("\x1b\\");
    if terminated {
        buffer.clear();
    } else {
        buffer.drain(..start);
    }
}
