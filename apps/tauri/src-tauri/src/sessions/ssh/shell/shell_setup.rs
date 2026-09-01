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
