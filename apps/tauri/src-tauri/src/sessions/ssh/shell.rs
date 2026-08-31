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

/// Map the identity reported by the interactive shell to the file pane access
/// model. Cached sudo credentials are deliberately not part of this decision:
/// they make a future root switch reusable, but they do not mean the current
/// shell is still privileged after `exit` returns to the login user.
fn resolve_shell_file_access(login_user: &str, shell_user: &str) -> (&'static str, Option<String>) {
    let login_user = login_user.trim();
    let shell_user = shell_user.trim();
    if login_user.is_empty() || shell_user.is_empty() || login_user == shell_user {
        ("user", None)
    } else {
        ("root", Some(shell_user.to_string()))
    }
}

fn root_access_method_for_shell_user(
    shell_user: &str,
    last_authenticated_access: Option<&PendingRootAccessAuth>,
    pending_access_command: Option<&PendingRootAccessAuth>,
) -> RootFileAccessMethod {
    // The command that just produced the new shell identity is authoritative.
    // This matters for passwordless sudo and for switching from `sudo -i` to
    // `su -` (or vice versa), where no new password prompt may be available to
    // overwrite the previous cached method.
    pending_access_command
        .filter(|auth| auth.interactive_shell && auth.target_user == shell_user)
        .or_else(|| {
            last_authenticated_access
                .filter(|auth| auth.interactive_shell && auth.target_user == shell_user)
        })
        .map(|auth| auth.method)
        .unwrap_or(RootFileAccessMethod::Sudo)
}

fn root_password_for_method(
    method: RootFileAccessMethod,
    sudo_password: &Option<String>,
    su_password: &Option<String>,
) -> Option<String> {
    match method {
        RootFileAccessMethod::Sudo => sudo_password.clone(),
        RootFileAccessMethod::Su => su_password.clone(),
    }
}

fn cache_root_password_for_auth(
    auth: Option<&PendingRootAccessAuth>,
    root_password: &Option<String>,
    sudo_password: &mut Option<String>,
    su_password: &mut Option<String>,
) {
    let Some(auth) = auth else {
        return;
    };
    let Some(password) = root_password.clone() else {
        return;
    };
    match auth.method {
        RootFileAccessMethod::Sudo => *sudo_password = Some(password),
        RootFileAccessMethod::Su => *su_password = Some(password),
    }
}

/// Fill an interactive sudo/su prompt from the separately saved profile
/// secret. The write happens only after the PTY has emitted a matching
/// password prompt; no password is sent pre-emptively or written to the
/// terminal transcript.
async fn autofill_root_access_password(
    shell_writer: &SshShellWriteHalf,
    awaiting_auth: &mut Option<PendingRootAccessAuth>,
    pending_password: &mut String,
    root_password: &mut Option<String>,
    sudo_password: &Option<String>,
    su_password: &Option<String>,
) -> Result<bool, String> {
    let Some(auth) = awaiting_auth.clone() else {
        return Ok(false);
    };
    if !auth.interactive_shell {
        return Ok(false);
    }
    let Some(password) = root_password_for_method(auth.method, sudo_password, su_password) else {
        return Ok(false);
    };
    write_shell_data(shell_writer, format!("{password}\r").into_bytes()).await?;
    *root_password = Some(password);
    pending_password.clear();
    *awaiting_auth = None;
    Ok(true)
}

/// Remove CSI/OSC control sequences before inspecting a prompt. This mirrors
/// Electron's root-prompt heuristic without feeding visual escape codes into
/// the comparison.
///
/// The regexes are pre-compiled: `visible_shell_text` is on the shell data
/// hot path (called per chunk for sudo prompt tracking and root prompt
/// detection), and re-compiling them per chunk burned enough CPU to
/// noticeably stretch `terminal_input_rx` polling latency under
/// high-throughput output (e.g. `pacman-key --populate`).
static VISIBLE_SHELL_CSI_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("constant CSI regex"));
static VISIBLE_SHELL_OSC_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").expect("constant OSC regex")
});

fn visible_shell_text(value: &str) -> String {
    let stripped = VISIBLE_SHELL_CSI_RE.replace_all(value, "");
    VISIBLE_SHELL_OSC_RE.replace_all(&stripped, "").into_owned()
}

fn looks_like_root_prompt(value: &str) -> bool {
    visible_shell_text(value).trim_end().ends_with('#')
}

/// A root-style prompt is only a reason to re-install the hook after the
/// terminal has explicitly sent an interactive `sudo`/`su` command. A normal
/// user's literal `#` can be echoed as a one-character chunk, so prompt shape
/// alone must never trigger another command write into the PTY.
fn should_reinject_root_shell_setup(
    shell_setup_available: bool,
    setup_echo_pending: bool,
    waiting_for_initial_prompt: bool,
    interactive_root_transition_pending: bool,
    shell_is_root: bool,
    visible: &str,
) -> bool {
    shell_setup_available
        && !setup_echo_pending
        && !waiting_for_initial_prompt
        && interactive_root_transition_pending
        && !shell_is_root
        && looks_like_root_prompt(visible)
}

fn looks_like_shell_prompt(value: &str) -> bool {
    let visible = visible_shell_text(value);
    let prompt = visible.trim_end();
    prompt.ends_with('$') || prompt.ends_with('#') || prompt.ends_with('%') || prompt.ends_with('>')
}

/// 在等待 shell 第一个 prompt 期间，把 chunk 里"prompt 尾部"从 forward 文本
/// 里剥离出来——只 forward banner 部分（保留原始 escape 序列和颜色），prompt
/// 部分由调用方暂存到 `shell_prompt_buffer` 用于触发 setup 注入。
///
/// 这样 shell 启动期间输出的 prompt 不会立即显示给用户；setup 注入成功后
/// suppress 接管，新 prompt 由 suppress 释放时统一 forward，用户只看到一个
/// prompt。群晖 DSM 的 /etc/profile 等启动脚本可能在第一个 prompt 之后还
/// 异步执行命令并输出新 prompt，这些都会被暂存而非 forward。
///
/// 切分在原始 chunk 上进行：从末尾往前找第一个 prompt 结尾符（$ / # / % / >），
/// 再从该位置往前找行首（跳过 escape 序列），行首之前是 banner（forward），
/// 之后是 prompt 尾部（暂存）。找不到则整个 chunk 作为 banner forward。
fn split_prompt_tail_for_setup_wait(chunk: &str) -> (String, String) {
    let bytes = chunk.as_bytes();
    let mut prompt_end_idx: Option<usize> = None;
    // 从末尾往前找第一个 prompt 结尾符，遇到换行则停（说明最后一行不是 prompt）
    for i in (0..bytes.len()).rev() {
        let c = bytes[i] as char;
        if c == '$' || c == '#' || c == '%' || c == '>' {
            prompt_end_idx = Some(i);
            break;
        }
        if c == '\n' {
            break;
        }
    }
    let Some(end_idx) = prompt_end_idx else {
        return (chunk.to_string(), String::new());
    };
    // 从 prompt 结尾符往前找行首：跳过同行所有字符直到遇到换行或 chunk 开头。
    // escape 序列（CSI/OSC）如果出现在 prompt 行内（比如彩色 prompt），会被
    // 一起划入 prompt 尾部暂存，不会丢失——暂存的 prompt 尾部不 forward，
    // setup 注入后由 shell 输出的新 prompt（含颜色）替代。
    let mut line_start = end_idx;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let banner = chunk[..line_start].to_string();
    let prompt_tail = chunk[line_start..].to_string();
    (banner, prompt_tail)
}

/// Separate file operations run in a fresh SSH exec channel, so they need to
/// reproduce the privilege transition performed in the interactive shell.
/// Keep the transition method worker-local: neither it nor its password is
/// serialized into a workspace snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootFileAccessMethod {
    Sudo,
    Su,
}

fn parse_root_file_access_method(value: Option<&str>) -> Result<RootFileAccessMethod, String> {
    match value.unwrap_or("sudo") {
        "sudo" => Ok(RootFileAccessMethod::Sudo),
        "su" => Ok(RootFileAccessMethod::Su),
        other => Err(format!("不支持的 root 文件访问方式: {other}")),
    }
}

fn root_file_access_method_label(method: RootFileAccessMethod) -> &'static str {
    match method {
        RootFileAccessMethod::Sudo => "sudo",
        RootFileAccessMethod::Su => "su",
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRootAccessAuth {
    method: RootFileAccessMethod,
    target_user: String,
    interactive_shell: bool,
}

fn privilege_command_from_terminal_input(input: &str) -> Option<PendingRootAccessAuth> {
    let command = input
        .trim_end_matches(['\r', '\n'])
        .rsplit(['\r', '\n'])
        .next()?
        .trim();
    let mut parts = command.split_whitespace();
    let executable = parts.next()?;
    let args = parts.collect::<Vec<_>>();

    let method = match executable {
        "sudo" => RootFileAccessMethod::Sudo,
        "su" => RootFileAccessMethod::Su,
        _ => return None,
    };

    let mut target_user = None;
    let mut interactive_shell = method == RootFileAccessMethod::Su;
    let mut skip_next = false;
    let mut next_is_user = false;
    for arg in args {
        if skip_next {
            if next_is_user {
                target_user = Some(arg);
            }
            skip_next = false;
            next_is_user = false;
            continue;
        }
        if arg == "-u" || arg == "--user" {
            skip_next = true;
            next_is_user = true;
            continue;
        }
        if arg == "-c" || arg == "--command" {
            interactive_shell = false;
            skip_next = true;
            next_is_user = false;
            continue;
        }
        if arg == "-s" || arg == "--shell" {
            interactive_shell = true;
            skip_next = true;
            next_is_user = false;
            continue;
        }
        if arg == "-i" || arg == "--login" || (method == RootFileAccessMethod::Su && arg == "-l") {
            interactive_shell = true;
            continue;
        }
        if !arg.starts_with('-') {
            target_user = Some(arg);
        }
    }

    Some(PendingRootAccessAuth {
        method,
        target_user: target_user.unwrap_or("root").to_string(),
        interactive_shell,
    })
}

/// Track an interactive sudo or su exchange on the terminal channel. The
/// password stays worker-local and is never copied into a snapshot or emitted
/// event.
fn capture_root_access_password_input(
    input: &str,
    awaiting_auth: &mut Option<PendingRootAccessAuth>,
    pending_password: &mut String,
    recent_input: &mut String,
    sudo_password: &mut Option<String>,
    last_authenticated_access: &mut Option<PendingRootAccessAuth>,
    pending_command: &mut Option<PendingRootAccessAuth>,
) -> bool {
    let mut changed = false;
    for ch in input.chars() {
        if awaiting_auth.is_none() && matches!(ch, '\r' | '\n') {
            let current_line = recent_input
                .rsplit(['\r', '\n'])
                .next()
                .unwrap_or("")
                .trim();
            // Do not erase the last interactive privilege command when the
            // next line is an ordinary terminal input (most commonly the
            // password itself).  The shell can deliver the `su` password
            // prompt after the input channel has already received the user's
            // password, so replacing `Some(su)` with `None` here makes the
            // subsequent RemoteUser=root marker fall back to sudo.
            if let Some(command) = privilege_command_from_terminal_input(recent_input) {
                *pending_command = Some(command);
                // Reuse this worker-local buffer as a pre-prompt password
                // candidate. It is only promoted after a matching password
                // prompt is observed, so a passwordless `su` cannot turn an
                // arbitrary later shell command into credentials.
                pending_password.clear();
            } else if !current_line.is_empty()
                && pending_command
                    .as_ref()
                    .is_some_and(|auth| auth.interactive_shell)
            {
                pending_password.clear();
                pending_password.push_str(current_line);
            }
        }
        recent_input.push(ch);
        if recent_input.len() > 512 {
            // 用户输入可含 CJK，滚动窗口必须 char 边界安全，否则此分支
            // panic 会无声杀死 worker（输入通道随之失效，Ctrl+C 无响应）。
            trim_string_front(recent_input, 256);
        }
        let Some(auth) = awaiting_auth.clone() else {
            // Keep a worker-local pre-prompt line while the network is
            // delivering the password prompt. If the prompt arrives midway
            // through typing, the already received prefix must not be lost.
            if pending_command
                .as_ref()
                .is_some_and(|command| command.interactive_shell)
            {
                match ch {
                    '\u{8}' | '\u{7f}' => {
                        pending_password.pop();
                    }
                    '\u{3}' => pending_password.clear(),
                    _ if !ch.is_control() => pending_password.push(ch),
                    _ => {}
                }
            }
            continue;
        };
        match ch {
            '\r' | '\n' => {
                if !pending_password.is_empty() {
                    changed = sudo_password.as_deref() != Some(pending_password.as_str());
                    *sudo_password = Some(std::mem::take(pending_password));
                    *last_authenticated_access = Some(auth);
                }
                *awaiting_auth = None;
            }
            '\u{3}' => {
                changed = sudo_password.take().is_some();
                pending_password.clear();
                *awaiting_auth = None;
                *last_authenticated_access = None;
            }
            '\u{8}' | '\u{7f}' => {
                pending_password.pop();
            }
            _ if !ch.is_control() => pending_password.push(ch),
            _ => {}
        }
    }
    changed
}

fn coalesce_terminal_input(
    mut first: String,
    receiver: &mut mpsc::UnboundedReceiver<String>,
) -> String {
    while let Ok(next) = receiver.try_recv() {
        first.push_str(&next);
    }
    first
}

fn track_root_access_prompt_from_terminal(
    output: &str,
    prompt_buffer: &mut String,
    awaiting_auth: &mut Option<PendingRootAccessAuth>,
    pending_password: &mut String,
    sudo_password: &mut Option<String>,
    last_authenticated_access: &mut Option<PendingRootAccessAuth>,
    pending_command: &mut Option<PendingRootAccessAuth>,
) -> bool {
    let mut changed = false;
    prompt_buffer.push_str(&visible_shell_text(output));
    if prompt_buffer.len() > 2048 {
        // shell 输出含中文时直接字节切片会 panic 杀死 worker，
        // 滚动窗口必须 char 边界安全。
        trim_string_front(prompt_buffer, 1024);
    }
    let lower = prompt_buffer.to_ascii_lowercase();
    let auth_failed = root_access_auth_failed(&lower);
    if auth_failed {
        *awaiting_auth = None;
        pending_password.clear();
        prompt_buffer.clear();
        *last_authenticated_access = None;
        *pending_command = None;
        return sudo_password.take().is_some();
    }
    if lower.contains("password") || prompt_buffer.contains("密码") {
        if let Some(auth) = pending_command.clone() {
            if !pending_password.is_empty() {
                // The user may have entered the password before this output
                // packet reached the worker. Promote the deferred line now
                // that the prompt proves it was an authentication exchange.
                changed = sudo_password.as_deref() != Some(pending_password.as_str());
                *sudo_password = Some(std::mem::take(pending_password));
                *last_authenticated_access = Some(auth);
                *awaiting_auth = None;
            } else {
                *awaiting_auth = Some(auth);
            }
            // Consume this prompt; otherwise the historical word "password"
            // would mark every later terminal keystroke as a root password.
            prompt_buffer.clear();
        }
    }
    changed
}

fn root_access_auth_failed(output: &str) -> bool {
    output.contains("sorry, try again")
        || output.contains("incorrect password")
        || output.contains("authentication failure")
        || output.contains("authentication failed")
        || output.contains("密码错误")
        || output.contains("密码不正确")
        || output.contains("身份验证失败")
        || output.contains("认证失败")
}

/// Buffered output produced while injecting the internal CWD hook.
///
/// A POSIX PTY is allowed to split the command echo, the generated OSC marker
/// and the replacement prompt across packets. Do not release the buffer as
/// soon as the marker is observed: doing so leaks the tail of a long setup
/// command after `sudo -i` on Debian/bash.
struct ShellSetupEchoSuppression {
    buffer: String,
    started_at: Instant,
    visible_prefix_length: Option<usize>,
    marker_seen_at: Option<Instant>,
    preserve_visible_prefix: bool,
    fallback_visible: Option<String>,
}

impl ShellSetupEchoSuppression {
    fn new(preserve_visible_prefix: bool) -> Self {
        Self {
            buffer: String::new(),
            started_at: Instant::now(),
            visible_prefix_length: None,
            marker_seen_at: None,
            preserve_visible_prefix,
            fallback_visible: None,
        }
    }

    fn with_fallback(fallback_visible: String) -> Self {
        let mut state = Self::new(false);
        state.fallback_visible = Some(fallback_visible);
        state
    }
}

const SHELL_SETUP_SETTLE_DELAY: Duration = Duration::from_millis(200);
// The setup command is sent through the PTY and its echo/OSC response can be
// delayed by a slow embedded SSH server. Keep the fail-open window long enough
// not to release user input back into an unfinished line-editor command.
const SHELL_SETUP_TIMEOUT: Duration = Duration::from_secs(5);
const SHELL_SETUP_PROMPT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_SHELL_SETUP_BUFFER_BYTES: usize = 16 * 1024;

fn shell_setup_release_deadline(pending: &Option<ShellSetupEchoSuppression>) -> Option<Instant> {
    pending.as_ref().map(|state| {
        state
            .marker_seen_at
            .map(|seen_at| seen_at + SHELL_SETUP_SETTLE_DELAY)
            .unwrap_or(state.started_at + SHELL_SETUP_TIMEOUT)
    })
}

fn finish_shell_setup_suppression(pending: &mut Option<ShellSetupEchoSuppression>) -> String {
    let Some(state) = pending.take() else {
        return String::new();
    };
    if !state.preserve_visible_prefix {
        // setup 成功执行（检测到唯一的 ready OSC marker）后，shell 会输出新 prompt。
        // 第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存（不 forward），
        // 所以这里释放新 prompt——让用户看到一个完整 prompt，而不是空白。
        if state.marker_seen_at.is_some() {
            // buffer 里同时含 setup echo、ready marker 和新 prompt。找到 marker
            // 的结束位置，释放它之后的部分（新 prompt），
            // 吞掉 setup echo 和 marker。marker 后可能直接接 prompt（无换行），
            // 所以不能用 rfind('\n') 切分。
            if let Some(marker_end) = last_shell_setup_marker_end(&state.buffer) {
                let after_marker = &state.buffer[marker_end..];
                if looks_like_shell_prompt(after_marker) {
                    return after_marker.to_string();
                }
            }
            // 新 prompt 还没到（慢设备，settle/timeout 到期仍未见）：补换行
            // 让晚到的新 prompt 从新行开始。
            return "\r\n".to_string();
        }
        // Root-shell injection keeps the prompt that was withheld before the
        // write as a fail-open fallback. The initial login setup has no such
        // fallback and therefore releases nothing when the marker is absent.
        return state.fallback_visible.unwrap_or_default();
    }
    state
        .visible_prefix_length
        .map(|length| state.buffer[..length].to_string())
        .unwrap_or_default()
}

// Pre-compiled private ready marker used by `suppress_shell_setup_echo` while
// it inspects buffered shell-setup output. Compiled once instead of per chunk.
static SHELL_SETUP_READY_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\x1b\]7777;FileTermReady(?:\x07|\x1b\\)")
        .expect("constant shell setup ready regex")
});

fn last_shell_setup_marker_end(value: &str) -> Option<usize> {
    SHELL_SETUP_READY_RE
        .find_iter(value)
        .last()
        .map(|mat| mat.end())
}

/// Suppresses the echo and replacement prompt from an internal CWD-hook
/// injection. The bounded timeout fails closed: a malformed shell must not
/// expose the hidden command in the user's terminal transcript.
fn suppress_shell_setup_echo(
    pending: &mut Option<ShellSetupEchoSuppression>,
    chunk: &str,
) -> String {
    if pending.is_none() {
        return chunk.to_string();
    }

    let now = Instant::now();
    if shell_setup_release_deadline(pending).is_some_and(|deadline| now >= deadline) {
        return format!("{}{chunk}", finish_shell_setup_suppression(pending));
    }

    let state = pending
        .as_mut()
        .expect("pending CWD hook suppression exists");

    state.buffer.push_str(chunk);
    const HOOK_MARKER: &str = "__tdcwd";

    if let Some(marker_end) = last_shell_setup_marker_end(&state.buffer) {
        state.marker_seen_at.get_or_insert(now);
        if state.visible_prefix_length.is_none() {
            state.visible_prefix_length = Some(
                state
                    .buffer
                    .find("test -z \"${FISH_VERSION-}\"")
                    .or_else(|| state.buffer.find("__tdcwd(){"))
                    .or_else(|| state.buffer.find(HOOK_MARKER))
                    .unwrap_or(0),
            );
        }
        // marker 已看到后，setup 命令执行完 shell 会输出新 prompt。一旦新 prompt
        // 到达（ready marker 之后的部分匹配 prompt 结尾），立即结束 suppress 并
        // 释放新 prompt。第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存
        // （不 forward），所以这里释放新 prompt 让用户看到一个完整 prompt。
        // 慢设备（群晖）新 prompt 可能晚于 settle delay 到达，固定窗口兜不住；
        // 改为检测到 prompt 就提前结束，无论快慢设备都只显示一个 prompt。
        // 仅 preserve_visible_prefix == false（首次注入）路径生效；sudo 重注入
        // 路径需要保留 visible prefix，仍走 settle delay 释放。
        if !state.preserve_visible_prefix {
            if let Some(after_marker) = state.buffer.get(marker_end..) {
                if looks_like_shell_prompt(after_marker) {
                    return finish_shell_setup_suppression(pending);
                }
            }
        }
    }

    if state.buffer.len() > MAX_SHELL_SETUP_BUFFER_BYTES {
        return finish_shell_setup_suppression(pending);
    }

    String::new()
}

/// Returns the POSIX shell CWD setup script for the given platform.
///
/// Mirrors Electron's `shellCwdSetupForPlatform`:
/// - `busybox` → compact ash-compatible one-liner (≤256 bytes to avoid
///   BusyBox line-editor truncation)
/// - `linux` / `darwin` → bash/zsh/posix-aware hook via PROMPT_COMMAND /
///   precmd / PS1 (macOS bash/zsh support the same hooks as Linux)
/// - `windows` / unknown → `None` (fail-closed, no injection)
///
/// The injected hook defines `__tdcwd` which emits OSC7 (`file://<path>`) and
/// 1337 (`RemoteUser=<user>`) on every prompt, enabling CWD + sudo user
/// tracking without polling.
fn shell_cwd_setup_for_platform(platform: &str) -> Option<&'static str> {
    match platform {
        "busybox" => Some(BUSYBOX_SHELL_CWD_SETUP),
        "linux" | "darwin" => Some(SHELL_CWD_SETUP),
        _ => None,
    }
}

/// Linux shell CWD hook (bash / zsh / posix). Mirrors Electron's
/// `SHELL_CWD_SETUP` constant. Uses `test -z "${FISH_VERSION-}"` as a fish
/// guard so the hook is a no-op on fish (which has its own CWD reporting).
const SHELL_CWD_SETUP: &str = concat!(
    "test -z \"${FISH_VERSION-}\" && eval '",
    "__tdcwd() { printf \"\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007\" \"$(pwd -P 2>/dev/null)\" \"$(id -un 2>/dev/null)\"; }; ",
    "if [ -n \"${ZSH_VERSION-}\" ]; then autoload -Uz add-zsh-hook 2>/dev/null; add-zsh-hook -D precmd __tdcwd 2>/dev/null; add-zsh-hook precmd __tdcwd 2>/dev/null; ",
    "elif [ -n \"${BASH_VERSION-}\" ]; then case \"${PROMPT_COMMAND-}\" in *\"__tdcwd\"*) ;; *) PROMPT_COMMAND=\"__tdcwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}\" ;; esac; ",
    "else case \"${PS1-}\" in *\"__tdcwd\"*) ;; *) PS1=\"\\$(__tdcwd)${PS1-}\" ;; esac; fi; ",
    "__tdcwd; ",
    // A leading space is only a best-effort history guard. Bash users may
    // have HISTCONTROL disabled, so remove this exact internal line by its
    // marker after it has executed.
    "if [ -n \"${BASH_VERSION-}\" ]; then ",
    "__ft_hist_marker=\"__FILETERM_INTERNAL_SETUP_1\"; ",
    "__ft_hist_line=$(HISTTIMEFORMAT= builtin history 1 2>/dev/null); ",
    "case \"$__ft_hist_line\" in *\"__FILETERM_INTERNAL_SETUP_1\"*) ",
    "__ft_hist_number=$(printf \"%s\\n\" \"$__ft_hist_line\" | sed -n \"s/^ *\\([0-9][0-9]*\\).*/\\1/p\"); ",
    "case \"$__ft_hist_number\" in \"\"|*[!0-9]*) ;; *) builtin history -d \"$__ft_hist_number\" 2>/dev/null ;; esac; ",
    ";; esac; ",
    "unset __ft_hist_marker __ft_hist_line __ft_hist_number; ",
    "fi; ",
    "printf \"\\033]7777;FileTermReady\\007\"' || printf \"\\033]7777;FileTermReady\\007\"",
);

/// BusyBox ash CWD hook. Kept under 256 bytes to avoid truncation in the
/// small interactive line-editing buffer. Mirrors Electron's
/// `BUSYBOX_SHELL_CWD_SETUP` constant.
const BUSYBOX_SHELL_CWD_SETUP: &str = "__tdcwd(){ printf '\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007' \"$(pwd -P 2>/dev/null)\" \"$(id -un 2>/dev/null)\";};PS1='$(__tdcwd)'\"${PS1-}\";__tdcwd;printf '\\033]7777;FileTermReady\\007'";

/// Normalize an encoding label to a canonical name understood by
/// `encoding_rs`. Mirrors Electron's `normalizeEncoding` alias table.
fn normalize_encoding(encoding: &str) -> &'static str {
    let normalized = encoding.trim().to_lowercase();
    match normalized.as_str() {
        "utf8" | "utf-8" | "" => "utf-8",
        "utf-8-bom" => "utf-8-bom",
        "utf16" | "utf-16" | "utf16le" | "utf-16le" => "utf-16le",
        "utf16be" | "utf-16be" => "utf-16be",
        "gb18030" => "gb18030",
        "gbk" => "gbk",
        "big5" | "cp950" => "big5",
        "euc-jp" | "eucjp" => "euc-jp",
        "shift-jis" | "shiftjis" | "shift_jis" | "sjis" => "shift_jis",
        "iso-2022-jp" => "iso-2022-jp",
        "euc-kr" | "euckr" | "cp949" => "euc-kr",
        "windows-1252" | "cp1252" => "windows-1252",
        "latin1" | "iso-8859-1" => "iso-8859-1",
        "windows-1251" | "cp1251" => "windows-1251",
        _ => "utf-8",
    }
}

/// Decode raw bytes into a string using the given encoding. Mirrors
/// Electron's `decodeBuffer` (iconv-lite + BOM stripping).
fn decode_bytes(buf: &[u8], encoding: &str) -> Result<String, String> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => {
            let text = std::str::from_utf8(buf)
                .map_err(|error| format!("utf-8 decode failed: {error}"))?;
            Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_string())
        }
        "utf-8-bom" => {
            let start = if buf.starts_with(&[0xef, 0xbb, 0xbf]) {
                3
            } else {
                0
            };
            String::from_utf8(buf[start..].to_vec())
                .map_err(|e| format!("utf-8 decode failed: {}", e))
        }
        "utf-16le" => {
            let start = if buf.starts_with(&[0xff, 0xfe]) { 2 } else { 0 };
            decode_utf16(&buf[start..], true)
        }
        "utf-16be" => {
            let start = if buf.starts_with(&[0xfe, 0xff]) { 2 } else { 0 };
            decode_utf16(&buf[start..], false)
        }
        "gb18030" => decode_with_encoding(encoding_rs::GB18030, buf, normalized),
        "gbk" => decode_with_encoding(encoding_rs::GBK, buf, normalized),
        "big5" => decode_with_encoding(encoding_rs::BIG5, buf, normalized),
        "euc-jp" => decode_with_encoding(encoding_rs::EUC_JP, buf, normalized),
        "shift_jis" => decode_with_encoding(encoding_rs::SHIFT_JIS, buf, normalized),
        "iso-2022-jp" => decode_with_encoding(encoding_rs::ISO_2022_JP, buf, normalized),
        "euc-kr" => decode_with_encoding(encoding_rs::EUC_KR, buf, normalized),
        "windows-1252" => decode_with_encoding(encoding_rs::WINDOWS_1252, buf, normalized),
        "iso-8859-1" => decode_with_encoding(encoding_rs::WINDOWS_1252, buf, normalized),
        "windows-1251" => decode_with_encoding(encoding_rs::WINDOWS_1251, buf, normalized),
        _ => Err(format!("unsupported text encoding: {normalized}")),
    }
}

fn decode_with_encoding(
    encoding: &'static encoding_rs::Encoding,
    bytes: &[u8],
    label: &str,
) -> Result<String, String> {
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(format!("{label} decode failed: invalid byte sequence"));
    }
    Ok(text.into_owned())
}

/// Decode UTF-16 bytes (little-endian or big-endian) into a string.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("utf-16 data length is odd".to_string());
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|e| format!("utf-16 decode failed: {}", e))
}

/// Encode a string into bytes using the given encoding. Mirrors Electron's
/// `encodeText` (iconv-lite + BOM prefixing for utf-8-bom / utf-16le / utf-16be).
fn encode_text(content: &str, encoding: &str) -> Result<Vec<u8>, String> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => Ok(content.as_bytes().to_vec()),
        "utf-8-bom" => {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(content.as_bytes());
            Ok(bytes)
        }
        "utf-16le" => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Ok(bytes)
        }
        "utf-16be" => {
            let mut bytes = vec![0xfe, 0xff];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            Ok(bytes)
        }
        "gb18030" => encode_with_encoding(encoding_rs::GB18030, content, normalized),
        "gbk" => encode_with_encoding(encoding_rs::GBK, content, normalized),
        "big5" => encode_with_encoding(encoding_rs::BIG5, content, normalized),
        "euc-jp" => encode_with_encoding(encoding_rs::EUC_JP, content, normalized),
        "shift_jis" => encode_with_encoding(encoding_rs::SHIFT_JIS, content, normalized),
        "iso-2022-jp" => encode_with_encoding(encoding_rs::ISO_2022_JP, content, normalized),
        "euc-kr" => encode_with_encoding(encoding_rs::EUC_KR, content, normalized),
        "windows-1252" => encode_with_encoding(encoding_rs::WINDOWS_1252, content, normalized),
        "iso-8859-1" => encode_with_encoding(encoding_rs::WINDOWS_1252, content, normalized),
        "windows-1251" => encode_with_encoding(encoding_rs::WINDOWS_1251, content, normalized),
        _ => Err(format!("unsupported text encoding: {normalized}")),
    }
}

fn encode_with_encoding(
    encoding: &'static encoding_rs::Encoding,
    content: &str,
    label: &str,
) -> Result<Vec<u8>, String> {
    let (bytes, _, had_errors) = encoding.encode(content);
    if had_errors {
        return Err(format!("{label} cannot encode one or more characters"));
    }
    Ok(bytes.into_owned())
}
