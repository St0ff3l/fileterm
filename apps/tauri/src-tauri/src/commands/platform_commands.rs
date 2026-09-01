// Platform, local CLI discovery, architecture, clipboard, and update commands.
#[tauri::command]
pub fn app_get_platform() -> String {
    std::env::consts::OS.to_string()
}

fn shell_quote_path(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    if cfg!(target_os = "windows") {
        format!("\"{}\"", raw.replace('"', "\\\""))
    } else {
        format!("'{}'", raw.replace('\'', "'\\\"'\\\"'"))
    }
}

/// Resolve a CLI from the inherited PATH and the installation directories that
/// desktop launchers commonly omit from PATH. Finder-launched macOS apps do not
/// source the user's shell profile, so npm/nvm-installed clients must also be
/// discoverable without spawning a shell or executing the client.
fn resolve_local_cli(command: &str) -> Option<std::path::PathBuf> {
    let mut search_paths = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();

    // OpenCode's official installer may place the native binary in a directory
    // that a GUI-launched process does not inherit. Keep these paths ahead of
    // the generic Node manager fallbacks, matching the installer priority used
    // by CC Switch without executing the client or a shell profile.
    if command.eq_ignore_ascii_case("opencode") {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        for path in opencode_extra_search_paths(
            &home,
            std::env::var_os("OPENCODE_INSTALL_DIR"),
            std::env::var_os("XDG_BIN_DIR"),
            std::env::var_os("GOPATH"),
        ) {
            push_unique_cli_search_path(&mut search_paths, path);
        }
    }

    append_local_cli_search_paths(&mut search_paths);
    resolve_local_cli_from_paths(command, search_paths)
}

fn resolve_local_cli_from_paths<I>(command: &str, directories: I) -> Option<std::path::PathBuf>
where
    I: IntoIterator<Item = std::path::PathBuf>,
{
    let direct = std::path::PathBuf::from(command);
    if direct.components().count() > 1
        && direct.is_file()
        && is_usable_local_cli_candidate(command, &direct)
    {
        return Some(direct);
    }

    let extensions: &[&str] = if cfg!(target_os = "windows") {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };

    for directory in directories {
        for extension in extensions {
            let candidate = directory.join(format!("{command}{extension}"));
            if candidate.is_file() && is_usable_local_cli_candidate(command, &candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_usable_local_cli_candidate(command: &str, candidate: &Path) -> bool {
    if is_embedded_desktop_app_cli(command, candidate) || is_invalid_windows_executable(candidate) {
        return false;
    }

    if command.eq_ignore_ascii_case("claude") {
        let native_binary = candidate
            .parent()
            .map(|parent| {
                parent
                    .join("node_modules")
                    .join("@anthropic-ai")
                    .join("claude-code")
                    .join("bin")
                    .join("claude.exe")
            })
            .filter(|path| path != candidate);

        if is_claude_native_stub(candidate)
            || native_binary.as_deref().is_some_and(|path| {
                path.is_file()
                    && (is_claude_native_stub(path) || is_invalid_windows_executable(path))
            })
        {
            return false;
        }
    }

    true
}

/// Claude Code's npm wrapper leaves this small shell script in place when its
/// platform-native optional dependency is missing. It has an `.exe` suffix on
/// Windows, but is not an executable at all; do not report it as an installed
/// CLI. The same marker can also be reached through npm's generated shims.
fn is_claude_native_stub(candidate: &Path) -> bool {
    const STUB_MARKER: &[u8] = b"claude native binary not installed.";
    let Ok(metadata) = std::fs::metadata(candidate) else {
        return false;
    };
    if metadata.len() > 4096 {
        return false;
    }

    std::fs::read(candidate)
        .map(|contents| {
            contents
                .windows(STUB_MARKER.len())
                .any(|window| window == STUB_MARKER)
        })
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn is_invalid_windows_executable(candidate: &Path) -> bool {
    if !candidate
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return false;
    }

    let Ok(mut file) = std::fs::File::open(candidate) else {
        return true;
    };
    let mut header = [0_u8; 2];
    file.read_exact(&mut header).is_err() || header != [b'M', b'Z']
}

#[cfg(not(target_os = "windows"))]
fn is_invalid_windows_executable(_candidate: &Path) -> bool {
    false
}

/// ChatGPT for macOS ships an internal `codex` executable in its app bundle.
/// That binary is not the user-facing Codex CLI, even when the desktop app
/// exposes its Resources directory through PATH. Do not report it as an
/// installed CLI; also cover symlinks that resolve into an app bundle.
fn is_embedded_desktop_app_cli(command: &str, candidate: &std::path::Path) -> bool {
    if !command.eq_ignore_ascii_case("codex") {
        return false;
    }

    fn is_macos_app_internal_path(path: &std::path::Path) -> bool {
        let components = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();

        components.windows(3).any(|window| {
            window[0].ends_with(".app")
                && window[1] == "contents"
                && matches!(window[2].as_str(), "resources" | "macos")
        })
    }

    if is_macos_app_internal_path(candidate) {
        return true;
    }

    candidate
        .canonicalize()
        .map(|resolved| is_macos_app_internal_path(&resolved))
        .unwrap_or(false)
}

fn append_local_cli_search_paths(search_paths: &mut Vec<std::path::PathBuf>) {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(std::path::PathBuf::from);

    if let Some(home) = home.as_ref() {
        append_home_cli_search_paths(search_paths, home);
    }

    #[cfg(target_os = "macos")]
    {
        for path in [
            std::path::PathBuf::from("/opt/homebrew/bin"),
            std::path::PathBuf::from("/usr/local/bin"),
        ] {
            push_unique_cli_search_path(search_paths, path);
        }
    }

    #[cfg(target_os = "linux")]
    {
        for path in [
            std::path::PathBuf::from("/usr/local/bin"),
            std::path::PathBuf::from("/usr/bin"),
        ] {
            push_unique_cli_search_path(search_paths, path);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            push_unique_cli_search_path(
                search_paths,
                std::path::PathBuf::from(app_data).join("npm"),
            );
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            let local_app_data = std::path::PathBuf::from(local_app_data);
            push_unique_cli_search_path(search_paths, local_app_data.join("pnpm"));
            push_unique_cli_search_path(search_paths, local_app_data.join("Volta/bin"));
        }
        if let Some(volta_home) = std::env::var_os("VOLTA_HOME") {
            push_unique_cli_search_path(
                search_paths,
                std::path::PathBuf::from(volta_home).join("bin"),
            );
        }
        if let Some(nvm_symlink) = std::env::var_os("NVM_SYMLINK") {
            push_unique_cli_search_path(search_paths, std::path::PathBuf::from(nvm_symlink));
        }
        if let Some(nvm_home) = std::env::var_os("NVM_HOME") {
            let nvm_home = std::path::PathBuf::from(nvm_home);
            push_unique_cli_search_path(search_paths, nvm_home.clone());
            if let Ok(entries) = std::fs::read_dir(nvm_home) {
                for entry in entries.flatten() {
                    push_unique_cli_search_path(search_paths, entry.path());
                }
            }
        }
        if let Some(home) = home.as_ref() {
            push_unique_cli_search_path(search_paths, home.join("scoop/shims"));
        }
        push_unique_cli_search_path(
            search_paths,
            std::path::PathBuf::from(r"C:\Program Files\nodejs"),
        );
    }
}

fn append_home_cli_search_paths(
    search_paths: &mut Vec<std::path::PathBuf>,
    home: &std::path::Path,
) {
    // Native Claude Code installs and the common Node version managers.
    for relative in [
        ".local/bin",
        ".claude/local",
        ".claude/bin",
        ".npm-global/bin",
        "n/bin",
        ".volta/bin",
        ".bun/bin",
        ".asdf/shims",
        ".local/share/mise/shims",
        ".fnm/current/bin",
        ".nvm/current/bin",
    ] {
        push_unique_cli_search_path(search_paths, home.join(relative));
    }

    // nvm keeps each Node version in its own bin directory. The directory
    // order is deterministic so a packaged app gets the newest lexical
    // version first when PATH is unavailable.
    let nvm_versions = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(nvm_versions) {
        let mut version_bins = entries
            .flatten()
            .map(|entry| entry.path().join("bin"))
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        version_bins.sort_by(|left, right| right.cmp(left));
        for path in version_bins {
            push_unique_cli_search_path(search_paths, path);
        }
    }
}

/// OpenCode's official installer order is
/// `OPENCODE_INSTALL_DIR > XDG_BIN_DIR > ~/bin > ~/.opencode/bin`.
/// Include the default Bun and Go locations as well because those installs are
/// common on machines where a packaged desktop app receives a reduced PATH.
fn opencode_extra_search_paths(
    home: &std::path::Path,
    opencode_install_dir: Option<std::ffi::OsString>,
    xdg_bin_dir: Option<std::ffi::OsString>,
    gopath: Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for value in [opencode_install_dir, xdg_bin_dir].into_iter().flatten() {
        push_unique_cli_search_path(&mut paths, std::path::PathBuf::from(value));
    }

    if !home.as_os_str().is_empty() {
        for relative in ["bin", ".opencode/bin", ".bun/bin", "go/bin"] {
            push_unique_cli_search_path(&mut paths, home.join(relative));
        }
    }

    if let Some(gopath) = gopath {
        for path in std::env::split_paths(&gopath) {
            push_unique_cli_search_path(&mut paths, path.join("bin"));
        }
    }

    paths
}

fn push_unique_cli_search_path(
    search_paths: &mut Vec<std::path::PathBuf>,
    path: std::path::PathBuf,
) {
    if path.as_os_str().is_empty() || search_paths.iter().any(|existing| existing == &path) {
        return;
    }
    search_paths.push(path);
}

/// Discover locally installed Agent CLIs without launching them. This keeps
/// setup responsive and avoids invoking arbitrary shell startup files on all
/// three desktop platforms.
#[tauri::command]
pub fn app_get_mcp_agent_setup() -> Result<McpAgentSetup, AppError> {
    let fileterm_path = std::env::current_exe().map_err(|error| {
        AppError::Command(format!("Unable to locate the FileTerm executable: {error}"))
    })?;
    let fileterm_command = shell_quote_path(&fileterm_path);
    let make_client = |id: &str, label: &str, command: &str, registration_command: String| {
        let path = resolve_local_cli(command);
        McpAgentClientStatus {
            id: id.to_string(),
            label: label.to_string(),
            command: command.to_string(),
            available: path.is_some(),
            path: path.map(|path| path.to_string_lossy().to_string()),
            registration_command,
        }
    };

    Ok(McpAgentSetup {
        fileterm_command: fileterm_command.clone(),
        clients: vec![
            make_client(
                "claude-code",
                "Claude Code",
                "claude",
                format!("claude mcp add --scope user fileterm -- {fileterm_command} mcp"),
            ),
            make_client(
                "codex-cli",
                "Codex CLI",
                "codex",
                format!("codex mcp add fileterm -- {fileterm_command} mcp"),
            ),
            make_client(
                "opencode",
                "OpenCode",
                "opencode",
                format!("opencode mcp add fileterm -- {fileterm_command} mcp"),
            ),
        ],
    })
}

fn canonical_arch(arch: &str) -> String {
    match arch {
        "aarch64" => "arm64".to_string(),
        "x86_64" => "x64".to_string(),
        other => other.to_string(),
    }
}

fn resolve_native_arch(platform: &str, process_arch: &str, macos_arm64_capable: bool) -> String {
    if platform == "macos" && macos_arm64_capable {
        return "arm64".to_string();
    }

    canonical_arch(process_arch)
}

#[cfg(target_os = "macos")]
fn macos_arm64_capable() -> bool {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.optional.arm64"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|value| value.trim() == "1")
}

#[cfg(not(target_os = "macos"))]
fn macos_arm64_capable() -> bool {
    false
}

#[tauri::command]
pub fn app_get_arch() -> String {
    resolve_native_arch(
        std::env::consts::OS,
        std::env::consts::ARCH,
        macos_arm64_capable(),
    )
}

#[tauri::command]
pub fn app_get_runtime_version() -> String {
    tauri::VERSION.to_string()
}

#[tauri::command]
pub fn app_read_clipboard_text() -> Result<String, AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| AppError::Clipboard(error.to_string()))?;
    clipboard
        .get_text()
        .map_err(|error| AppError::Clipboard(error.to_string()))
}

#[tauri::command]
pub fn app_write_clipboard_text(text: String) -> Result<(), AppError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| AppError::Clipboard(error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| AppError::Clipboard(error.to_string()))
}

#[tauri::command]
pub fn app_open_external_url(url: String) -> Result<(), AppError> {
    let parsed = validate_external_url(&url)?;
    open::that(parsed.as_str()).map_err(|error| AppError::Command(error.to_string()))
}

fn validate_external_url(url: &str) -> Result<url::Url, AppError> {
    let parsed = url::Url::parse(url)
        .map_err(|error| AppError::Command(format!("外部链接无效: {error}")))?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(parsed)
    } else {
        Err(AppError::Command(
            "仅允许打开 http 或 https 外部链接".to_string(),
        ))
    }
}

#[tauri::command]
pub async fn app_get_update_status(app: AppHandle) -> Result<serde_json::Value, AppError> {
    Ok(crate::services::updates::get_status(&app).await)
}

#[tauri::command]
pub async fn app_check_for_updates(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::updates::check(&app).await
}

#[tauri::command]
pub async fn app_download_update(app: AppHandle) -> Result<(), AppError> {
    crate::services::updates::download(&app).await
}

#[tauri::command]
pub async fn app_install_update(app: AppHandle) -> Result<(), AppError> {
    crate::services::updates::install(&app).await
}

#[tauri::command]
pub fn app_open_logs_directory(app: AppHandle) -> Result<(), AppError> {
    let log_directory = crate::storage::state_path(&app)?.with_file_name("logs");
    std::fs::create_dir_all(&log_directory)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    open::that(log_directory).map_err(|error| AppError::Command(error.to_string()))
}

pub use crate::services::serial_ports::SerialPortSnapshot as SerialPortListItem;

#[tauri::command]
pub async fn app_list_serial_ports() -> Result<Vec<SerialPortListItem>, AppError> {
    crate::services::serial_ports::list()
        .await
        .map_err(AppError::Command)
}
