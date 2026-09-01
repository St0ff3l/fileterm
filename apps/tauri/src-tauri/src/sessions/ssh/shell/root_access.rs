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
