#[derive(Clone, Debug)]
pub struct LocalTerminalLaunch {
    pub shell: String,
    pub title: Option<String>,
    pub cwd: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalLaunchOptions {
    pub shell: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalShellOption {
    pub shell: String,
    pub label: String,
    pub path: String,
}

fn executable_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn shell_option(shell: String, label: &str, path: PathBuf) -> LocalTerminalShellOption {
    LocalTerminalShellOption {
        shell,
        label: label.to_string(),
        path: path.to_string_lossy().into_owned(),
    }
}

fn add_shell_option(options: &mut Vec<LocalTerminalShellOption>, option: LocalTerminalShellOption) {
    if options
        .iter()
        .any(|existing| existing.shell == option.shell || existing.path == option.path)
    {
        return;
    }
    options.push(option);
}

pub fn available_shells() -> Vec<LocalTerminalShellOption> {
    #[cfg(target_os = "windows")]
    {
        available_windows_shells()
    }

    #[cfg(not(target_os = "windows"))]
    available_posix_shells()
}

#[cfg(target_os = "windows")]
fn available_windows_shells() -> Vec<LocalTerminalShellOption> {
    let candidates = [
        ("PowerShell 7", "pwsh.exe"),
        ("Windows PowerShell", "powershell.exe"),
        ("Command Prompt", "cmd.exe"),
        ("Git Bash", "bash.exe"),
        ("WSL", "wsl.exe"),
        ("Nushell", "nu.exe"),
        ("Fish", "fish.exe"),
    ];
    let mut options = Vec::new();
    for (label, name) in candidates {
        let Some(shell) = resolve_windows_shell(name) else {
            continue;
        };
        let path = executable_on_path(name)
            .or_else(|| Path::new(&shell).is_file().then(|| PathBuf::from(&shell)));
        let Some(path) = path else {
            continue;
        };
        add_shell_option(&mut options, shell_option(shell, label, path));
    }
    options
}

#[cfg(not(target_os = "windows"))]
fn available_posix_shells() -> Vec<LocalTerminalShellOption> {
    // Keep the picker focused on shells users commonly choose for an
    // interactive terminal. `/bin/sh`, `dash`, and similar compatibility or
    // legacy entry points are intentionally omitted; users can still enter a
    // less common shell manually in the settings field.
    let candidates = [
        ("Bash", "bash"),
        ("Zsh", "zsh"),
        ("Fish", "fish"),
        ("Nushell", "nu"),
        ("PowerShell 7", "pwsh"),
        ("Elvish", "elvish"),
        ("Xonsh", "xonsh"),
    ];
    let mut options = Vec::new();

    if let Some(shell) = env::var_os("SHELL")
        .and_then(|value| value.into_string().ok())
        .filter(|value| Path::new(value).is_file())
    {
        let label = format_shell_label(&shell);
        add_shell_option(
            &mut options,
            shell_option(shell.clone(), &label, PathBuf::from(shell)),
        );
    }

    for (label, name) in candidates {
        let path = executable_on_path(name).or_else(|| {
            [
                "/bin",
                "/usr/bin",
                "/usr/local/bin",
                "/opt/homebrew/bin",
                "/opt/local/bin",
            ]
            .iter()
            .map(|directory| Path::new(directory).join(name))
            .find(|candidate| candidate.is_file())
        });
        let Some(path) = path else {
            continue;
        };
        add_shell_option(&mut options, shell_option(name.to_string(), label, path));
    }
    options
}

#[cfg(not(target_os = "windows"))]
fn format_shell_label(shell: &str) -> String {
    match shell_name(shell).as_str() {
        "bash" => "Bash".to_string(),
        "zsh" => "Zsh".to_string(),
        "fish" => "Fish".to_string(),
        "sh" => "POSIX sh".to_string(),
        "dash" => "Dash".to_string(),
        "ksh" => "KornShell".to_string(),
        "tcsh" => "Tcsh".to_string(),
        "nu" => "Nushell".to_string(),
        "pwsh" | "powershell" => "PowerShell 7".to_string(),
        "elvish" => "Elvish".to_string(),
        "xonsh" => "Xonsh".to_string(),
        _ => shell.to_string(),
    }
}

fn clamp_u16(value: u32, fallback: u16) -> u16 {
    if value == 0 {
        return fallback;
    }
    value.min(u16::MAX as u32) as u16
}

#[cfg(target_os = "windows")]
fn default_shell() -> String {
    // 优先 PowerShell 7，缺失时回退 Windows PowerShell，再回退 cmd.exe。
    // Server Core / 精简镜像可能没有其中某个 shell。
    for name in ["pwsh.exe", "powershell.exe", "cmd.exe"] {
        if let Some(shell) = resolve_windows_shell(name) {
            return shell;
        }
    }

    // CommandBuilder/CreateProcess can still resolve this in a normal Windows
    // environment. Keep the logical fallback even when the environment is so
    // restricted that none of the standard locations were readable.
    "cmd.exe".to_string()
}

#[cfg(target_os = "windows")]
fn resolve_windows_shell(name: &str) -> Option<String> {
    if let Some(path_var) = env::var_os("PATH") {
        if env::split_paths(&path_var).any(|dir| dir.join(name).is_file()) {
            return Some(name.to_string());
        }
    }

    standard_windows_shell_path(name).map(|path| path.to_string_lossy().into_owned())
}

#[cfg(target_os = "windows")]
fn standard_windows_shell_path(name: &str) -> Option<PathBuf> {
    use std::path::Path;

    // PATH 可能被桌面启动器、服务或企业策略裁剪。Windows PowerShell
    // 实际位于 WindowsPowerShell\v1.0，而不是直接位于 System32。
    let system32 = env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32");
    let system_candidate = if name.eq_ignore_ascii_case("powershell.exe") {
        system32.join("WindowsPowerShell").join("v1.0").join(name)
    } else {
        system32.join(name)
    };
    if system_candidate.is_file() {
        return Some(system_candidate);
    }

    if name.eq_ignore_ascii_case("pwsh.exe") {
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            let Some(root) = env::var_os(variable) else {
                continue;
            };
            let base = Path::new(&root).join("PowerShell");
            for version in ["7", "7-preview"] {
                let candidate = base.join(version).join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

#[cfg(not(target_os = "windows"))]
fn default_shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty() && !shell_path_is_unavailable(shell))
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

fn default_working_directory() -> String {
    let home = if cfg!(target_os = "windows") {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    };
    home.map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned()
}

fn shell_path_is_unavailable(shell: &str) -> bool {
    let has_path_separator = shell.contains('/') || shell.contains('\\');
    has_path_separator && !PathBuf::from(shell).is_file()
}

fn validate_launch(launch: &LocalTerminalLaunch) -> Result<(), String> {
    if launch.shell.trim().is_empty() {
        return Err("Local terminal shell is empty".to_string());
    }
    if launch.shell.contains('\0') {
        return Err("Local terminal shell contains a NUL byte".to_string());
    }
    if let Some(title) = &launch.title {
        const MAX_LOCAL_TITLE_BYTES: usize = 120;
        if title.contains('\0') || title.len() > MAX_LOCAL_TITLE_BYTES {
            return Err(format!(
                "Local terminal title is invalid or longer than {MAX_LOCAL_TITLE_BYTES} bytes"
            ));
        }
    }
    if shell_path_is_unavailable(&launch.shell) {
        return Err(format!(
            "Local terminal shell does not exist: {}",
            launch.shell
        ));
    }
    if launch.cwd.trim().is_empty() {
        return Err("Local terminal working directory is empty".to_string());
    }
    if launch.cwd.contains('\0') {
        return Err("Local terminal working directory contains a NUL byte".to_string());
    }
    const MAX_LOCAL_SHELL_ARGS: usize = 128;
    const MAX_LOCAL_SHELL_ARG_BYTES: usize = 32 * 1024;
    if launch.args.len() > MAX_LOCAL_SHELL_ARGS {
        return Err(format!(
            "Local terminal accepts at most {MAX_LOCAL_SHELL_ARGS} shell arguments"
        ));
    }
    if let Some((index, _)) = launch
        .args
        .iter()
        .enumerate()
        .find(|(_, arg)| arg.contains('\0') || arg.len() > MAX_LOCAL_SHELL_ARG_BYTES)
    {
        return Err(format!(
            "Local terminal shell argument {index} is invalid or too large"
        ));
    }
    const MAX_LOCAL_ENV_ENTRIES: usize = 128;
    const MAX_LOCAL_ENV_VALUE_BYTES: usize = 64 * 1024;
    if launch.env.len() > MAX_LOCAL_ENV_ENTRIES {
        return Err(format!(
            "Local terminal accepts at most {MAX_LOCAL_ENV_ENTRIES} environment overrides"
        ));
    }
    if let Some((name, _)) = launch.env.iter().find(|(name, value)| {
        name.is_empty()
            || name.contains('=')
            || name.contains('\0')
            || value.contains('\0')
            || value.len() > MAX_LOCAL_ENV_VALUE_BYTES
    }) {
        return Err(format!(
            "Local terminal environment override {name:?} is invalid or too large"
        ));
    }
    Ok(())
}

fn shell_name(shell: &str) -> String {
    shell
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(shell)
        .trim_start_matches('-')
        .to_ascii_lowercase()
}

#[cfg(not(target_os = "windows"))]
fn has_non_empty_env(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn set_default_env(
    command: &mut CommandBuilder,
    name: &str,
    value: &str,
    custom_env: &BTreeMap<String, String>,
) {
    if !custom_env.contains_key(name) {
        command.env(name, value);
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_shell_command(
    command: &mut CommandBuilder,
    shell: &str,
    extra_args: &[String],
    custom_env: &BTreeMap<String, String>,
) {
    let name = shell_name(shell);
    let inject_default_zsh_prompt = name == "zsh"
        && !custom_env.contains_key("PROMPT")
        && !custom_env.contains_key("PS1")
        && !has_non_empty_env("PROMPT")
        && !has_non_empty_env("PS1");
    if inject_default_zsh_prompt {
        // The prompt uses command substitution to percent-encode the current
        // directory for OSC 7. Enable it before user args so `-c`/`--` keep
        // their normal meaning.
        command.args(["-o", "promptsubst"]);
    }
    if matches!(
        name.as_str(),
        "bash" | "dash" | "fish" | "ksh" | "mksh" | "sh" | "zsh"
    ) {
        command.arg("-l");
    }
    command.args(extra_args);

    set_default_env(command, "TERM", "xterm-256color", custom_env);
    set_default_env(command, "COLORTERM", "truecolor", custom_env);
    set_default_env(command, "TERM_PROGRAM", "FileTerm", custom_env);

    // bash 原生读取 PROMPT_COMMAND，每次显示 prompt 前执行。zsh 默认使用
    // PROMPT；给它注入一个保持默认视觉样式的不可见 OSC 7 前缀。fish/sh 等
    // 没有安全的环境变量 prompt hook，用户需要在 rc 文件里手动加 hook。
    if name == "bash" {
        inject_bash_osc7_prompt_command(command, custom_env);
    } else if inject_default_zsh_prompt {
        inject_zsh_osc7_prompt(command, custom_env);
    }

    if !custom_env.contains_key("LANG")
        && !custom_env.contains_key("LC_ALL")
        && !has_non_empty_env("LANG")
        && !has_non_empty_env("LC_ALL")
    {
        command.env("LANG", default_utf8_locale());
    }
    if !custom_env.contains_key("LC_ALL")
        && !custom_env.contains_key("LC_CTYPE")
        && !has_non_empty_env("LC_ALL")
        && !has_non_empty_env("LC_CTYPE")
        && !custom_env
            .get("LANG")
            .cloned()
            .or_else(|| env::var("LANG").ok())
            .map(|value| value.to_ascii_lowercase().contains("utf-8"))
            .unwrap_or(false)
    {
        command.env("LC_CTYPE", default_utf8_locale());
    }
}

#[cfg(not(target_os = "windows"))]
fn inject_bash_osc7_prompt_command(
    command: &mut CommandBuilder,
    custom_env: &BTreeMap<String, String>,
) {
    // 用户显式传入 PROMPT_COMMAND 时不覆盖。
    if custom_env.contains_key("PROMPT_COMMAND") {
        return;
    }

    // PROMPT_COMMAND 在每次显示 prompt 前 emit 一次 OSC 7。将 `%` 先编码，
    // 避免目录名中的字面量 `%20` 被后端误解为一个空格。
    // 不使用 DEBUG trap：它会在每条简单命令（包括循环和子 shell）前产生
    // 额外 PTY 输出，反而会放大高输出场景的丢帧压力。
    const OSC7_HOOK: &str = "printf '\\033]7;file://%s\\007' \"${PWD//%/%25}\"";

    let combined = match env::var("PROMPT_COMMAND")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(existing) => format!("{OSC7_HOOK}; {existing}"),
        None => OSC7_HOOK.to_string(),
    };
    command.env("PROMPT_COMMAND", combined);
}

#[cfg(not(target_os = "windows"))]
fn inject_zsh_osc7_prompt(command: &mut CommandBuilder, custom_env: &BTreeMap<String, String>) {
    // 尊重用户显式提供的 prompt。没有自定义 prompt 时，使用 zsh 默认的
    // `%m%# ` 样式，仅在其前面加入不占列宽的 OSC 7 CWD 标记。
    if custom_env.contains_key("PROMPT")
        || custom_env.contains_key("PS1")
        || has_non_empty_env("PROMPT")
        || has_non_empty_env("PS1")
    {
        return;
    }

    const ZSH_PROMPT: &str = "%{$(printf '\\033]7;file://%s\\007' \"${PWD//%/%25}\")%}%m%# ";
    command.env("PROMPT", ZSH_PROMPT);
}

#[cfg(target_os = "windows")]
fn configure_shell_command(
    command: &mut CommandBuilder,
    shell: &str,
    extra_args: &[String],
    custom_env: &BTreeMap<String, String>,
) {
    let name = shell_name(shell);
    set_default_env(command, "TERM", "xterm-256color", custom_env);
    set_default_env(command, "COLORTERM", "truecolor", custom_env);
    set_default_env(command, "TERM_PROGRAM", "FileTerm", custom_env);

    match name.as_str() {
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            command.arg("-NoLogo");
            // The default Windows PowerShell profile is user-controlled and may
            // import modules, start a prompt helper, or wait on network state.
            // A stalled profile leaves the first local terminal permanently at
            // "Starting local shell...". Preserve profile behavior for callers
            // that explicitly provide launch arguments, while making the
            // default FileTerm shell deterministic and ready immediately.
            if extra_args.is_empty() {
                command.arg("-NoProfile");
            }
            command.arg("-NoExit");
            command.args(extra_args);
            // -Command / -CommandWithArgs / -File / -EncodedCommand 互斥：用户传了这些参数时
            // 不再自动追加 UTF-8 setup，避免 PowerShell 因参数冲突直接报错。
            // 用户需在自己的脚本/命令里设置 UTF-8 编码。
            if !powershell_args_have_explicit_command(extra_args) {
                command.args([
                    "-Command",
                    "$utf8 = [System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding = $utf8; [Console]::OutputEncoding = $utf8; $OutputEncoding = $utf8",
                ]);
            }
        }
        "cmd" | "cmd.exe" => {
            command.args(extra_args);
            // `/C` and `/K` consume the remaining command line. Appending our
            // own `/K` after an explicit command changes the user's command.
            if !cmd_args_have_explicit_command(extra_args) {
                command.args(["/K", "chcp 65001>nul"]);
            }
        }
        "bash" | "bash.exe" | "fish" | "fish.exe" | "zsh" | "zsh.exe" => {
            command.arg("--login");
            command.args(extra_args);
        }
        _ => {
            command.args(extra_args);
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn powershell_args_have_explicit_command(extra_args: &[String]) -> bool {
    // PowerShell 允许参数唯一前缀缩写，并把 `-c`、`-cwa`、`-f`、`-e`、`-ec`
    // 作为 Command/CommandWithArgs/File/EncodedCommand 的短写。命中任何显式命令模式后，
    // configure_shell_command 不再追加自己的 `-Command`。
    // ConfigurationFile/ConfigurationName 只是会话配置参数，仍可与
    // -Command 组合，不能把它们误判为命令模式。
    const EXPLICIT_FLAGS: &[&str] = &["command", "commandwithargs", "file", "encodedcommand"];
    const EXPLICIT_ALIASES: &[&str] = &["c", "cwa", "f", "e", "ec"];
    extra_args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        let Some(flag) = lower.strip_prefix('-') else {
            return false;
        };
        !flag.is_empty()
            && (EXPLICIT_ALIASES.contains(&flag)
                || EXPLICIT_FLAGS.iter().any(|known| known.starts_with(flag)))
    })
}

#[cfg(any(target_os = "windows", test))]
fn cmd_args_have_explicit_command(extra_args: &[String]) -> bool {
    extra_args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        matches!(lower.as_str(), "/c" | "/k")
    })
}

#[cfg(target_os = "macos")]
fn default_utf8_locale() -> &'static str {
    "en_US.UTF-8"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_utf8_locale() -> &'static str {
    "C.UTF-8"
}
