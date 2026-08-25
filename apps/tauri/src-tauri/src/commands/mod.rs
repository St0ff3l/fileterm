use crate::sessions::WorkerCmd;
use crate::storage::read_json_array;
use crate::AppError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{ipc::Channel, AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

/// 等待 worker 接收命令的最大时间。worker 主循环被 SFTP init / shell
/// channel 写阻塞 时，mpsc 一旦满，send 会永久 await，导致前端 invoke
/// 链路整体卡死（多窗口发送后续 tab 全部排队、Cmd+Q 退出无法完成）。
/// 超时后返回显式 busy 错误，绝不静默吞掉输入。SSH 终端输入已经走
/// 独立 channel；这里仍作为 Telnet / Serial 和通用 worker 命令的保护。
const WORKER_CMD_SEND_TIMEOUT: Duration = Duration::from_millis(500);

/// 文件/会话级操作（list/read/write/重连等）容忍更长延迟，但同样不能
/// 永久阻塞——一旦 worker 卡死，应当让前端拿到明确错误。
const WORKER_FILE_CMD_SEND_TIMEOUT: Duration = Duration::from_secs(5);

/// Worker 已接收命令后也必须在有限时间内答复。之前仅限制了 mpsc send，
/// 但某个后台 SFTP/exec task 丢失 reply 时，oneshot 会一直 await，导致
/// 删除、打开目录和 Root 弹窗永久 loading。
const WORKER_FILE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(20);

/// 退出时给 worker 的 Disconnect 命令留 1 秒，超时直接放弃发送：worker
/// 主循环卡死时 channel 满，send 不进去；强行 await 会让 Cmd+Q 整个
/// 退出链路 hang 住，用户只能强制杀进程。drop sender 后 worker 的
/// `cmd_rx.recv()` 会返回 None，自然走清理路径。
const WORKER_DISCONNECT_TIMEOUT: Duration = Duration::from_secs(1);

const SERIAL_TRANSFER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// A silent shell must not leave a local tab in `connecting` forever. The
/// startup task uses this bounded window to publish the first prompt when it
/// arrives; the main open command itself does not wait for the window, so the
/// workspace can switch immediately.
const LOCAL_TERMINAL_STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(2);

/// Let a child-window close command resolve its IPC callback before destroying
/// the calling WebView. Destroying synchronously makes WebView2 report a
/// missing callback id and can leave renderer cleanup half-finished.
const CHILD_WINDOW_DESTROY_DELAY: Duration = Duration::from_millis(25);

async fn send_terminal_input(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    data: String,
) -> Result<(), AppError> {
    if let Some(sender) = state.terminal_inputs.read().await.get(tab_id).cloned() {
        return sender
            .send(data)
            .map_err(|_| AppError::Storage("Terminal session closed".to_string()));
    }

    // Telnet and serial still use their protocol worker queue. SSH owns the
    // dedicated low-latency input channel above.
    let sender = state
        .workers
        .read()
        .await
        .get(tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("Terminal session not found".to_string()))?;
    timeout(
        WORKER_CMD_SEND_TIMEOUT,
        sender.send(WorkerCmd::WriteTerminal(data)),
    )
    .await
    .map_err(|_| AppError::Storage("Terminal worker busy".to_string()))?
    .map_err(|error| AppError::Storage(error.to_string()))
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectionDefaults {
    #[serde(default = "default_use_empty_password")]
    pub use_empty_password: bool,
    #[serde(default = "default_enable_exec_channel")]
    pub enable_exec_channel: bool,
    #[serde(default = "default_enable_resource_monitoring")]
    pub enable_resource_monitoring: bool,
    #[serde(default = "default_resource_monitoring_interval_seconds")]
    pub resource_monitoring_interval_seconds: u64,
    #[serde(default = "default_resource_monitoring_metrics")]
    pub resource_monitoring_metrics: Vec<String>,
    #[serde(default = "default_resource_monitoring_metric_order")]
    pub resource_monitoring_metric_order: Vec<String>,
    #[serde(default = "default_reconnect_mode")]
    pub reconnect_mode: String,
    #[serde(default = "default_legacy_algorithms")]
    pub legacy_algorithms: bool,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectionDefaultsInput {
    pub use_empty_password: Option<bool>,
    pub enable_exec_channel: Option<bool>,
    pub enable_resource_monitoring: Option<bool>,
    pub resource_monitoring_interval_seconds: Option<u64>,
    pub resource_monitoring_metrics: Option<Vec<String>>,
    pub resource_monitoring_metric_order: Option<Vec<String>>,
    pub reconnect_mode: Option<String>,
    pub legacy_algorithms: Option<bool>,
}

/// Non-secret boundary applied to MCP clients that are launched by external
/// Agents. It deliberately does not contain connection credentials or any
/// executable configuration path.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentPreferences {
    #[serde(default = "default_mcp_connection_scope")]
    pub connection_scope: String,
    #[serde(default = "default_mcp_operation_policy")]
    pub operation_policy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile_id: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentPreferencesInput {
    pub connection_scope: Option<String>,
    pub operation_policy: Option<String>,
    pub default_profile_id: Option<Option<String>>,
}

impl Default for McpAgentPreferences {
    fn default() -> Self {
        Self {
            connection_scope: default_mcp_connection_scope(),
            operation_policy: default_mcp_operation_policy(),
            default_profile_id: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentClientStatus {
    pub id: String,
    pub label: String,
    pub command: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub registration_command: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentSetup {
    pub fileterm_command: String,
    pub clients: Vec<McpAgentClientStatus>,
}

impl Default for SshConnectionDefaults {
    fn default() -> Self {
        Self {
            use_empty_password: default_use_empty_password(),
            enable_exec_channel: default_enable_exec_channel(),
            enable_resource_monitoring: default_enable_resource_monitoring(),
            resource_monitoring_interval_seconds: default_resource_monitoring_interval_seconds(),
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            reconnect_mode: default_reconnect_mode(),
            legacy_algorithms: default_legacy_algorithms(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeAnsiPalette {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSearchColors {
    pub match_background: String,
    pub match_ruler: String,
    pub active_match_background: String,
    pub active_match_text: String,
    pub active_match_border: String,
    pub active_match_ruler: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TerminalThemeConfig {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub cursor_accent: String,
    pub selection_background: String,
    pub selection_foreground: String,
    pub ansi: ThemeAnsiPalette,
    pub search: ThemeSearchColors,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFonts {
    pub code: Option<String>,
    pub ui: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSemanticColors {
    pub diff_added: String,
    pub diff_removed: String,
    pub skill: String,
    pub keyword: String,
    #[serde(default)]
    pub sftp: String,
    #[serde(default)]
    pub ftp: String,
    /// Newer semantic controls are optional on disk so older theme exports
    /// can still be deserialized and filled by `normalize_theme_config`.
    #[serde(default)]
    pub secondary: String,
    #[serde(default)]
    pub text_secondary: String,
    #[serde(default)]
    pub info: String,
    #[serde(default)]
    pub warning: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub success: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeBody {
    pub accent: String,
    pub contrast: u8,
    pub fonts: ThemeFonts,
    pub ink: String,
    pub opaque_windows: bool,
    pub semantic_colors: ThemeSemanticColors,
    pub surface: String,
    #[serde(default)]
    pub surface_secondary: String,
    #[serde(default)]
    pub surface_elevated: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, String>,
    pub terminal: TerminalThemeConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    pub schema_version: String,
    pub code_theme_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_theme_id: Option<String>,
    pub variant: String,
    pub theme: ThemeBody,
}

fn default_theme_ansi(variant: &str) -> ThemeAnsiPalette {
    let is_light = variant == "light";
    ThemeAnsiPalette {
        black: "#000000".to_string(),
        red: "#cd3131".to_string(),
        green: if is_light { "#008000" } else { "#0dbc79" }.to_string(),
        yellow: if is_light { "#795e26" } else { "#e5e510" }.to_string(),
        blue: if is_light { "#0451a5" } else { "#2472c8" }.to_string(),
        magenta: if is_light { "#bc05bc" } else { "#bc3fbc" }.to_string(),
        cyan: if is_light { "#0598bc" } else { "#11a8cd" }.to_string(),
        white: "#ffffff".to_string(),
        bright_black: "#666666".to_string(),
        bright_red: "#cd3131".to_string(),
        bright_green: if is_light { "#14a800" } else { "#23d18b" }.to_string(),
        bright_yellow: if is_light { "#795e26" } else { "#e5e510" }.to_string(),
        bright_blue: if is_light { "#0451a5" } else { "#3b8eea" }.to_string(),
        bright_magenta: if is_light { "#bc05bc" } else { "#d670d6" }.to_string(),
        bright_cyan: if is_light { "#0598bc" } else { "#29b8db" }.to_string(),
        bright_white: "#ffffff".to_string(),
    }
}

fn codex_theme_config_for_variant(variant: &str) -> ThemeConfig {
    let is_light = variant == "light";
    ThemeConfig {
        schema_version: "codex-theme-v1".to_string(),
        code_theme_id: "codex".to_string(),
        base_theme_id: Some("codex".to_string()),
        variant: if is_light { "light" } else { "dark" }.to_string(),
        theme: ThemeBody {
            accent: if is_light { "#339cff" } else { "#0169cc" }.to_string(),
            contrast: if is_light { 45 } else { 60 },
            fonts: ThemeFonts {
                code: None,
                ui: None,
            },
            ink: if is_light { "#1a1c1f" } else { "#fcfcfc" }.to_string(),
            opaque_windows: false,
            semantic_colors: ThemeSemanticColors {
                diff_added: "#00a240".to_string(),
                diff_removed: if is_light { "#ba2623" } else { "#e02e2a" }.to_string(),
                skill: if is_light { "#924ff7" } else { "#b06dff" }.to_string(),
                keyword: if is_light { "#b45309" } else { "#ffcc00" }.to_string(),
                sftp: if is_light { "#0284c7" } else { "#38bdf8" }.to_string(),
                ftp: if is_light { "#924ff7" } else { "#b06dff" }.to_string(),
                secondary: if is_light { "#3b82f6" } else { "#8bbfff" }.to_string(),
                text_secondary: if is_light { "#667085" } else { "#a9a9b2" }.to_string(),
                info: if is_light { "#339cff" } else { "#0169cc" }.to_string(),
                warning: if is_light { "#b45309" } else { "#ffcc00" }.to_string(),
                error: if is_light { "#ba2623" } else { "#e02e2a" }.to_string(),
                success: if is_light { "#059669" } else { "#34d399" }.to_string(),
            },
            surface: if is_light { "#ffffff" } else { "#111111" }.to_string(),
            surface_secondary: if is_light { "#ffffff" } else { "#1b1b1b" }.to_string(),
            surface_elevated: if is_light { "#ffffff" } else { "#242424" }.to_string(),
            overrides: BTreeMap::new(),
            terminal: TerminalThemeConfig {
                background: if is_light { "#ffffff" } else { "#111111" }.to_string(),
                foreground: if is_light { "#1a1c1f" } else { "#fcfcfc" }.to_string(),
                cursor: if is_light { "#339cff" } else { "#0169cc" }.to_string(),
                cursor_accent: if is_light { "#ffffff" } else { "#111111" }.to_string(),
                selection_background: if is_light { "#339cff42" } else { "#0169cc55" }.to_string(),
                selection_foreground: if is_light { "#1a1c1f" } else { "#fcfcfc" }.to_string(),
                ansi: default_theme_ansi(variant),
                search: ThemeSearchColors {
                    match_background: if is_light { "#f6cf57" } else { "#4b5563" }.to_string(),
                    match_ruler: if is_light { "#d39b16" } else { "#9ca3af" }.to_string(),
                    active_match_background: "#ffd43b".to_string(),
                    active_match_text: "#111111".to_string(),
                    active_match_border: "#8a5a00".to_string(),
                    active_match_ruler: "#f0b400".to_string(),
                },
            },
        },
    }
}

fn default_theme_config_for_variant(variant: &str) -> ThemeConfig {
    let is_light = variant == "light";
    ThemeConfig {
        schema_version: "codex-theme-v1".to_string(),
        code_theme_id: "fileterm".to_string(),
        base_theme_id: Some("fileterm".to_string()),
        variant: if is_light { "light" } else { "dark" }.to_string(),
        theme: ThemeBody {
            accent: if is_light { "#3b82f6" } else { "#1687e8" }.to_string(),
            contrast: if is_light { 52 } else { 60 },
            fonts: ThemeFonts {
                code: None,
                ui: None,
            },
            ink: if is_light { "#18181b" } else { "#e7e7e7" }.to_string(),
            opaque_windows: true,
            semantic_colors: ThemeSemanticColors {
                diff_added: if is_light { "#168a53" } else { "#39d98a" }.to_string(),
                diff_removed: if is_light { "#d94e4e" } else { "#ff5f57" }.to_string(),
                skill: if is_light { "#7c3aed" } else { "#b06dff" }.to_string(),
                keyword: if is_light { "#b45309" } else { "#ffcc00" }.to_string(),
                sftp: if is_light { "#0284c7" } else { "#38bdf8" }.to_string(),
                ftp: if is_light { "#9333ea" } else { "#c084fc" }.to_string(),
                secondary: if is_light { "#3b82f6" } else { "#8bbfff" }.to_string(),
                text_secondary: if is_light { "#5e5e61" } else { "#9b9b9b" }.to_string(),
                info: if is_light { "#3b82f6" } else { "#8bbfff" }.to_string(),
                warning: if is_light { "#d97706" } else { "#ffcc00" }.to_string(),
                error: if is_light { "#d94e4e" } else { "#ff5f57" }.to_string(),
                success: if is_light { "#168a53" } else { "#39d98a" }.to_string(),
            },
            surface: if is_light { "#F4F4F6" } else { "#151515" }.to_string(),
            surface_secondary: if is_light { "#ffffff" } else { "#1e1e1e" }.to_string(),
            surface_elevated: if is_light { "#ffffff" } else { "#2a2a2a" }.to_string(),
            overrides: BTreeMap::new(),
            terminal: TerminalThemeConfig {
                background: if is_light { "#f4f4f6" } else { "#181818" }.to_string(),
                foreground: if is_light { "#111827" } else { "#e0e0e0" }.to_string(),
                cursor: if is_light { "#3b82f6" } else { "#ffffff" }.to_string(),
                cursor_accent: if is_light { "#ffffff" } else { "#181818" }.to_string(),
                selection_background: if is_light { "#0969DA42" } else { "#388BFD85" }.to_string(),
                selection_foreground: if is_light { "#111827" } else { "#e0e0e0" }.to_string(),
                ansi: default_theme_ansi(variant),
                search: ThemeSearchColors {
                    match_background: if is_light { "#f6cf57" } else { "#4b5563" }.to_string(),
                    match_ruler: if is_light { "#d39b16" } else { "#9ca3af" }.to_string(),
                    active_match_background: "#ffd43b".to_string(),
                    active_match_text: "#111111".to_string(),
                    active_match_border: "#8a5a00".to_string(),
                    active_match_ruler: "#f0b400".to_string(),
                },
            },
        },
    }
}

fn default_theme_config() -> ThemeConfig {
    default_theme_config_for_variant("dark")
}

fn is_theme_hex_color(value: &str) -> bool {
    let length = value.len();
    (length == 4 || length == 5 || length == 7 || length == 9)
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn normalize_theme_color(value: &mut String, fallback: &str) {
    let trimmed = value.trim();
    if is_theme_hex_color(trimmed) {
        *value = trimmed.to_uppercase();
    } else {
        *value = fallback.to_uppercase();
    }
}

fn is_theme_font(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_alphanumeric()
                || character.is_whitespace()
                || matches!(character, '_' | '.' | '\'' | '-')
        })
}

fn normalize_theme_config(mut config: ThemeConfig, variant: &str) -> ThemeConfig {
    let variant = if variant == "light" { "light" } else { "dark" };
    let trimmed_id = config.code_theme_id.trim();
    let is_codex_code_theme = trimmed_id == "codex" || trimmed_id.starts_with("codex-");
    let is_fileterm_theme = matches!(trimmed_id, "fileterm" | "fileterm-dark" | "fileterm-light");
    let base_theme_id = if is_codex_code_theme {
        "codex"
    } else if is_fileterm_theme {
        "fileterm"
    } else if matches!(config.base_theme_id.as_deref(), Some("codex")) {
        "codex"
    } else {
        "fileterm"
    };
    let is_codex_theme = base_theme_id == "codex";
    let fallback = if is_codex_theme {
        codex_theme_config_for_variant(variant)
    } else {
        default_theme_config_for_variant(variant)
    };
    config.schema_version = "codex-theme-v1".to_string();
    config.variant = variant.to_string();
    config.base_theme_id = Some(base_theme_id.to_string());
    if trimmed_id.is_empty() || config.code_theme_id.len() > 256 {
        config.code_theme_id = fallback.code_theme_id;
    } else if is_fileterm_theme {
        config.code_theme_id = "fileterm".to_string();
    } else if matches!(trimmed_id, "codex-dark" | "codex-light") {
        config.code_theme_id = "codex".to_string();
    } else {
        config.code_theme_id = trimmed_id.to_string();
    }
    config.theme.contrast = config.theme.contrast.min(100);
    let migrate_legacy_codex_status_colors = is_codex_theme
        && config.theme.overrides.is_empty()
        && config
            .theme
            .semantic_colors
            .sftp
            .trim()
            .eq_ignore_ascii_case("#0169cc")
        && config
            .theme
            .semantic_colors
            .success
            .trim()
            .eq_ignore_ascii_case("#00a240");
    normalize_theme_color(&mut config.theme.accent, &fallback.theme.accent);
    normalize_theme_color(&mut config.theme.ink, &fallback.theme.ink);
    normalize_theme_color(&mut config.theme.surface, &fallback.theme.surface);
    normalize_theme_color(
        &mut config.theme.surface_secondary,
        &fallback.theme.surface_secondary,
    );
    normalize_theme_color(
        &mut config.theme.surface_elevated,
        &fallback.theme.surface_elevated,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.diff_added,
        &fallback.theme.semantic_colors.diff_added,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.diff_removed,
        &fallback.theme.semantic_colors.diff_removed,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.skill,
        &fallback.theme.semantic_colors.skill,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.keyword,
        &fallback.theme.semantic_colors.keyword,
    );
    if migrate_legacy_codex_status_colors {
        config.theme.semantic_colors.sftp = fallback.theme.semantic_colors.sftp.clone();
    } else {
        normalize_theme_color(
            &mut config.theme.semantic_colors.sftp,
            &fallback.theme.semantic_colors.sftp,
        );
    }
    if is_theme_hex_color(config.theme.semantic_colors.ftp.trim()) {
        config.theme.semantic_colors.ftp = config.theme.semantic_colors.ftp.trim().to_uppercase();
    } else if !is_fileterm_theme && is_theme_hex_color(config.theme.semantic_colors.skill.trim()) {
        // Preserve the old behavior for saved themes created before FTP had
        // its own persisted semantic color.
        config.theme.semantic_colors.ftp = config.theme.semantic_colors.skill.trim().to_uppercase();
    } else {
        config.theme.semantic_colors.ftp = fallback.theme.semantic_colors.ftp.clone();
    }
    normalize_theme_color(
        &mut config.theme.semantic_colors.secondary,
        &fallback.theme.semantic_colors.secondary,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.text_secondary,
        &fallback.theme.semantic_colors.text_secondary,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.info,
        &fallback.theme.semantic_colors.info,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.warning,
        &fallback.theme.semantic_colors.warning,
    );
    normalize_theme_color(
        &mut config.theme.semantic_colors.error,
        &fallback.theme.semantic_colors.error,
    );
    if migrate_legacy_codex_status_colors {
        config.theme.semantic_colors.success = fallback.theme.semantic_colors.success.clone();
    } else {
        normalize_theme_color(
            &mut config.theme.semantic_colors.success,
            &fallback.theme.semantic_colors.success,
        );
    }
    config.theme.overrides.retain(|key, value| {
        let valid_key = !key.trim().is_empty() && key.len() <= 128;
        let valid_color = is_theme_hex_color(value.trim());
        if valid_color {
            *value = value.trim().to_uppercase();
        }
        valid_key && valid_color
    });

    for font in [&mut config.theme.fonts.code, &mut config.theme.fonts.ui] {
        if let Some(value) = font.as_mut() {
            let trimmed = value.trim();
            if !is_theme_font(trimmed) {
                *font = None;
            } else {
                *value = trimmed.to_string();
            }
        }
    }

    normalize_theme_color(
        &mut config.theme.terminal.background,
        &fallback.theme.terminal.background,
    );
    normalize_theme_color(
        &mut config.theme.terminal.foreground,
        &fallback.theme.terminal.foreground,
    );
    normalize_theme_color(
        &mut config.theme.terminal.cursor,
        &fallback.theme.terminal.cursor,
    );
    normalize_theme_color(
        &mut config.theme.terminal.cursor_accent,
        &fallback.theme.terminal.cursor_accent,
    );
    normalize_theme_color(
        &mut config.theme.terminal.selection_background,
        &fallback.theme.terminal.selection_background,
    );
    normalize_theme_color(
        &mut config.theme.terminal.selection_foreground,
        &fallback.theme.terminal.selection_foreground,
    );

    let ansi = [
        (
            &mut config.theme.terminal.ansi.black,
            &fallback.theme.terminal.ansi.black,
        ),
        (
            &mut config.theme.terminal.ansi.red,
            &fallback.theme.terminal.ansi.red,
        ),
        (
            &mut config.theme.terminal.ansi.green,
            &fallback.theme.terminal.ansi.green,
        ),
        (
            &mut config.theme.terminal.ansi.yellow,
            &fallback.theme.terminal.ansi.yellow,
        ),
        (
            &mut config.theme.terminal.ansi.blue,
            &fallback.theme.terminal.ansi.blue,
        ),
        (
            &mut config.theme.terminal.ansi.magenta,
            &fallback.theme.terminal.ansi.magenta,
        ),
        (
            &mut config.theme.terminal.ansi.cyan,
            &fallback.theme.terminal.ansi.cyan,
        ),
        (
            &mut config.theme.terminal.ansi.white,
            &fallback.theme.terminal.ansi.white,
        ),
        (
            &mut config.theme.terminal.ansi.bright_black,
            &fallback.theme.terminal.ansi.bright_black,
        ),
        (
            &mut config.theme.terminal.ansi.bright_red,
            &fallback.theme.terminal.ansi.bright_red,
        ),
        (
            &mut config.theme.terminal.ansi.bright_green,
            &fallback.theme.terminal.ansi.bright_green,
        ),
        (
            &mut config.theme.terminal.ansi.bright_yellow,
            &fallback.theme.terminal.ansi.bright_yellow,
        ),
        (
            &mut config.theme.terminal.ansi.bright_blue,
            &fallback.theme.terminal.ansi.bright_blue,
        ),
        (
            &mut config.theme.terminal.ansi.bright_magenta,
            &fallback.theme.terminal.ansi.bright_magenta,
        ),
        (
            &mut config.theme.terminal.ansi.bright_cyan,
            &fallback.theme.terminal.ansi.bright_cyan,
        ),
        (
            &mut config.theme.terminal.ansi.bright_white,
            &fallback.theme.terminal.ansi.bright_white,
        ),
    ];
    for (value, fallback_value) in ansi {
        normalize_theme_color(value, fallback_value);
    }

    normalize_theme_color(
        &mut config.theme.terminal.search.match_background,
        &fallback.theme.terminal.search.match_background,
    );
    normalize_theme_color(
        &mut config.theme.terminal.search.match_ruler,
        &fallback.theme.terminal.search.match_ruler,
    );
    normalize_theme_color(
        &mut config.theme.terminal.search.active_match_background,
        &fallback.theme.terminal.search.active_match_background,
    );
    normalize_theme_color(
        &mut config.theme.terminal.search.active_match_text,
        &fallback.theme.terminal.search.active_match_text,
    );
    normalize_theme_color(
        &mut config.theme.terminal.search.active_match_border,
        &fallback.theme.terminal.search.active_match_border,
    );
    normalize_theme_color(
        &mut config.theme.terminal.search.active_match_ruler,
        &fallback.theme.terminal.search.active_match_ruler,
    );
    config
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SavedTheme {
    pub id: String,
    pub name: String,
    pub config: ThemeConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variants: BTreeMap<String, ThemeConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    pub theme: String,
    pub locale: String,
    #[serde(default = "default_theme_config")]
    pub theme_config: ThemeConfig,
    #[serde(default)]
    pub custom_themes: Vec<SavedTheme>,
    #[serde(default = "default_auto_check_updates")]
    pub auto_check_updates: bool,
    #[serde(default = "default_update_channel")]
    pub update_channel: String,
    #[serde(default)]
    pub terminal_zoom_locked: bool,
    #[serde(default = "default_file_panel_remember_ratio")]
    pub file_panel_remember_ratio: bool,
    #[serde(default = "default_resource_monitoring_metrics")]
    pub resource_monitoring_metrics: Vec<String>,
    #[serde(default = "default_resource_monitoring_metric_order")]
    pub resource_monitoring_metric_order: Vec<String>,
    #[serde(default)]
    pub connection_defaults: SshConnectionDefaults,
    #[serde(default)]
    pub mcp_agent: McpAgentPreferences,
    #[serde(default = "default_overview_show_stats")]
    pub overview_show_stats: bool,
    #[serde(default = "default_overview_show_recent")]
    pub overview_show_recent: bool,
    #[serde(default = "default_overview_show_all_connections")]
    pub overview_show_all_connections: bool,
    #[serde(default = "default_overview_show_quick_actions")]
    pub overview_show_quick_actions: bool,
    #[serde(default = "default_overview_section_order")]
    pub overview_section_order: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferencesInput {
    pub theme: Option<String>,
    pub locale: Option<String>,
    pub theme_config: Option<ThemeConfig>,
    pub custom_themes: Option<Vec<SavedTheme>>,
    pub auto_check_updates: Option<bool>,
    pub update_channel: Option<String>,
    pub terminal_zoom_locked: Option<bool>,
    pub file_panel_remember_ratio: Option<bool>,
    pub resource_monitoring_metrics: Option<Vec<String>>,
    pub resource_monitoring_metric_order: Option<Vec<String>>,
    pub connection_defaults: Option<SshConnectionDefaultsInput>,
    pub mcp_agent: Option<McpAgentPreferencesInput>,
    pub overview_show_stats: Option<bool>,
    pub overview_show_recent: Option<bool>,
    pub overview_show_all_connections: Option<bool>,
    pub overview_show_quick_actions: Option<bool>,
    pub overview_section_order: Option<Vec<String>>,
}

const DEFAULT_UI_THEME: &str = "default-dark";
const DEFAULT_UI_LOCALE: &str = "zhCN";
const DEFAULT_OVERVIEW_SECTION_ORDER: [&str; 4] =
    ["stats", "recent", "allConnections", "quickActions"];

fn default_auto_check_updates() -> bool {
    true
}

fn default_update_channel() -> String {
    "stable".to_string()
}

fn default_file_panel_remember_ratio() -> bool {
    true
}

fn default_resource_monitoring_metrics() -> Vec<String> {
    [
        "load",
        "cpu",
        "memory",
        "swap",
        "disk",
        "processes",
        "network",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_resource_monitoring_metric_order() -> Vec<String> {
    [
        "load",
        "cpu",
        "memory",
        "swap",
        "disk",
        "gpu",
        "gpuMemory",
        "gpuTemperature",
        "gpuPower",
        "processes",
        "network",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn default_use_empty_password() -> bool {
    false
}

fn default_enable_exec_channel() -> bool {
    true
}

fn default_enable_resource_monitoring() -> bool {
    true
}

fn default_resource_monitoring_interval_seconds() -> u64 {
    1
}

fn default_reconnect_mode() -> String {
    "none".to_string()
}

fn default_legacy_algorithms() -> bool {
    false
}

fn default_mcp_connection_scope() -> String {
    "all-saved-connections".to_string()
}

fn default_mcp_operation_policy() -> String {
    "approved-operations".to_string()
}

fn default_overview_show_stats() -> bool {
    true
}

fn default_overview_show_recent() -> bool {
    true
}

fn default_overview_show_all_connections() -> bool {
    true
}

fn default_overview_show_quick_actions() -> bool {
    true
}

fn default_overview_section_order() -> Vec<String> {
    DEFAULT_OVERVIEW_SECTION_ORDER
        .iter()
        .map(|section| (*section).to_string())
        .collect()
}

fn normalize_overview_section_order(order: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(DEFAULT_OVERVIEW_SECTION_ORDER.len());
    for section in order {
        if DEFAULT_OVERVIEW_SECTION_ORDER.contains(&section.as_str())
            && !normalized.iter().any(|existing| existing == &section)
        {
            normalized.push(section);
        }
    }
    for section in DEFAULT_OVERVIEW_SECTION_ORDER {
        if !normalized.iter().any(|existing| existing == section) {
            normalized.push(section.to_string());
        }
    }
    normalized
}

fn normalize_resource_monitoring_metrics(metrics: Vec<String>) -> Vec<String> {
    const VALID_METRICS: [&str; 11] = [
        "load",
        "cpu",
        "memory",
        "swap",
        "disk",
        "gpu",
        "gpuMemory",
        "gpuTemperature",
        "gpuPower",
        "processes",
        "network",
    ];

    let mut normalized = Vec::with_capacity(metrics.len());
    for metric in metrics {
        if VALID_METRICS.contains(&metric.as_str()) && !normalized.contains(&metric) {
            normalized.push(metric);
        }
    }
    normalized
}

fn normalize_resource_monitoring_metric_order(order: Vec<String>) -> Vec<String> {
    const VALID_METRICS: [&str; 11] = [
        "load",
        "cpu",
        "memory",
        "swap",
        "disk",
        "gpu",
        "gpuMemory",
        "gpuTemperature",
        "gpuPower",
        "processes",
        "network",
    ];

    let mut normalized = Vec::with_capacity(VALID_METRICS.len());
    for metric in order {
        if VALID_METRICS.contains(&metric.as_str()) && !normalized.contains(&metric) {
            normalized.push(metric);
        }
    }
    for metric in VALID_METRICS {
        if !normalized.iter().any(|existing| existing == metric) {
            normalized.push(metric.to_string());
        }
    }
    normalized
}

fn normalize_saved_themes(themes: Vec<SavedTheme>) -> Vec<SavedTheme> {
    let mut normalized = Vec::new();
    for mut saved in themes {
        let id = saved.id.trim();
        let name = saved.name.trim();
        if id.is_empty()
            || id.len() > 128
            || name.is_empty()
            || name.len() > 128
            || normalized
                .iter()
                .any(|existing: &SavedTheme| existing.id == id)
        {
            continue;
        }

        saved.id = id.to_string();
        saved.name = name.to_string();
        let variant = if saved.config.variant == "light" {
            "light"
        } else {
            "dark"
        };
        saved.config = normalize_theme_config(saved.config, variant);
        saved.variants = std::mem::take(&mut saved.variants)
            .into_iter()
            .filter_map(|(variant, config)| {
                if !matches!(variant.as_str(), "dark" | "light") {
                    return None;
                }
                Some((variant.clone(), normalize_theme_config(config, &variant)))
            })
            .collect();
        normalized.push(saved);
        if normalized.len() >= 64 {
            break;
        }
    }
    normalized
}

fn normalize_ui_preferences(mut preferences: UiPreferences) -> UiPreferences {
    if !matches!(preferences.theme.as_str(), "default-dark" | "default-light") {
        preferences.theme = DEFAULT_UI_THEME.to_string();
    }
    if !matches!(preferences.locale.as_str(), "zhCN" | "enUS") {
        preferences.locale = DEFAULT_UI_LOCALE.to_string();
    }
    if !matches!(preferences.update_channel.as_str(), "stable" | "beta") {
        preferences.update_channel = default_update_channel();
    }
    if !matches!(
        preferences
            .connection_defaults
            .resource_monitoring_interval_seconds,
        1 | 5 | 15 | 30 | 60
    ) {
        preferences
            .connection_defaults
            .resource_monitoring_interval_seconds = default_resource_monitoring_interval_seconds();
    }
    if !matches!(
        preferences.connection_defaults.reconnect_mode.as_str(),
        "none" | "enter" | "auto"
    ) {
        preferences.connection_defaults.reconnect_mode = default_reconnect_mode();
    }
    if !matches!(
        preferences.mcp_agent.connection_scope.as_str(),
        "all-saved-connections" | "active-session" | "default-connection"
    ) {
        preferences.mcp_agent.connection_scope = default_mcp_connection_scope();
    }
    if !matches!(
        preferences.mcp_agent.operation_policy.as_str(),
        "read-only" | "approved-operations"
    ) {
        preferences.mcp_agent.operation_policy = default_mcp_operation_policy();
    }
    preferences.mcp_agent.default_profile_id =
        preferences
            .mcp_agent
            .default_profile_id
            .and_then(|profile_id| {
                let trimmed = profile_id.trim();
                (!trimmed.is_empty() && trimmed.len() <= 256).then(|| trimmed.to_string())
            });
    if preferences.mcp_agent.connection_scope == "default-connection"
        && preferences.mcp_agent.default_profile_id.is_none()
    {
        preferences.mcp_agent.connection_scope = "active-session".to_string();
    }
    preferences.overview_section_order =
        normalize_overview_section_order(preferences.overview_section_order);
    preferences.resource_monitoring_metrics =
        normalize_resource_monitoring_metrics(preferences.resource_monitoring_metrics);
    preferences.resource_monitoring_metric_order =
        normalize_resource_monitoring_metric_order(preferences.resource_monitoring_metric_order);
    preferences.theme_config = normalize_theme_config(
        preferences.theme_config,
        if preferences.theme == "default-light" {
            "light"
        } else {
            "dark"
        },
    );
    preferences.custom_themes = normalize_saved_themes(preferences.custom_themes);
    preferences
}

/// Resolve the effective SSH behavior for a live session without mutating the
/// persisted profile. Global values are creation-time defaults; saved profile
/// fields remain authoritative and legacy explicit overrides take precedence.
/// Defaults are only a fallback for profiles that do not have a saved value.
fn resolve_profile_with_connection_defaults(
    profile: &Value,
    defaults: &SshConnectionDefaults,
) -> Value {
    if profile.get("type").and_then(Value::as_str) != Some("ssh") {
        return profile.clone();
    }

    let Some(mut resolved) = profile.as_object().cloned() else {
        return profile.clone();
    };
    let overrides = resolved
        .get("connectionOverrides")
        .and_then(Value::as_object)
        .cloned();
    let saved_values = resolved.clone();
    let value_for = |key: &str, fallback: Value| {
        overrides
            .as_ref()
            .and_then(|values| values.get(key).cloned())
            .or_else(|| saved_values.get(key).cloned())
            .unwrap_or(fallback)
    };

    resolved.insert(
        "useEmptyPassword".to_string(),
        value_for("useEmptyPassword", Value::Bool(defaults.use_empty_password)),
    );
    resolved.insert(
        "enableExecChannel".to_string(),
        value_for(
            "enableExecChannel",
            Value::Bool(defaults.enable_exec_channel),
        ),
    );
    resolved.insert(
        "enableResourceMonitoring".to_string(),
        value_for(
            "enableResourceMonitoring",
            Value::Bool(defaults.enable_resource_monitoring),
        ),
    );
    resolved.insert(
        "resourceMonitoringIntervalSeconds".to_string(),
        value_for(
            "resourceMonitoringIntervalSeconds",
            Value::Number(defaults.resource_monitoring_interval_seconds.into()),
        ),
    );
    resolved.insert(
        "reconnectMode".to_string(),
        value_for(
            "reconnectMode",
            Value::String(defaults.reconnect_mode.clone()),
        ),
    );
    resolved.insert(
        "legacyAlgorithms".to_string(),
        value_for("legacyAlgorithms", Value::Bool(defaults.legacy_algorithms)),
    );

    Value::Object(resolved)
}

fn resolve_profile_for_session(app: &AppHandle, profile: &Value) -> Result<Value, AppError> {
    let preferences = app_get_ui_preferences(app.clone())?;
    Ok(resolve_profile_with_connection_defaults(
        profile,
        &preferences.connection_defaults,
    ))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandHistoryEntry {
    pub command: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSendPreferences {
    pub remember_selection: bool,
    pub send_scope: String,
    pub selected_tab_ids: Vec<String>,
}

impl Default for CommandSendPreferences {
    fn default() -> Self {
        Self {
            remember_selection: false,
            send_scope: "current".to_string(),
            selected_tab_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSshKeyInput {
    pub source_path: Option<String>,
    pub content: Option<String>,
    pub note: Option<String>,
}

fn write_json_object(app: &AppHandle, name: &str, value: &Value) -> Result<(), AppError> {
    let path = crate::storage::workspace_file(app, name)?;
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(&temporary, content).map_err(|error| AppError::Storage(error.to_string()))?;
    crate::storage::replace_file_atomically(&temporary, &path)
}

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
        && !is_embedded_desktop_app_cli(command, &direct)
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
            if candidate.is_file() && !is_embedded_desktop_app_cli(command, &candidate) {
                return Some(candidate);
            }
        }
    }
    None
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

fn parse_serial_control_action(
    action: &str,
) -> Result<crate::sessions::SerialControlAction, AppError> {
    match action {
        "set-dtr" => Ok(crate::sessions::SerialControlAction::SetDtr),
        "set-rts" => Ok(crate::sessions::SerialControlAction::SetRts),
        "pulse-dtr" => Ok(crate::sessions::SerialControlAction::PulseDtr),
        "pulse-rts" => Ok(crate::sessions::SerialControlAction::PulseRts),
        "send-break" => Ok(crate::sessions::SerialControlAction::SendBreak),
        "clear-buffers" => Ok(crate::sessions::SerialControlAction::ClearBuffers),
        "reset" => Ok(crate::sessions::SerialControlAction::Reset),
        "status" => Ok(crate::sessions::SerialControlAction::Status),
        _ => Err(AppError::Command("串口控制操作无效".to_string())),
    }
}

fn parse_serial_transfer_direction(
    direction: &str,
) -> Result<crate::sessions::SerialTransferDirection, AppError> {
    match direction {
        "send" => Ok(crate::sessions::SerialTransferDirection::Send),
        "receive" => Ok(crate::sessions::SerialTransferDirection::Receive),
        _ => Err(AppError::Command("串口传输方向无效".to_string())),
    }
}

fn parse_serial_transfer_mode(mode: &str) -> Result<crate::sessions::SerialTransferMode, AppError> {
    match mode {
        "raw" => Ok(crate::sessions::SerialTransferMode::Raw),
        "xmodem" => Ok(crate::sessions::SerialTransferMode::Xmodem),
        "ymodem" => Ok(crate::sessions::SerialTransferMode::Ymodem),
        "zmodem" => Ok(crate::sessions::SerialTransferMode::Zmodem),
        "kermit" => Ok(crate::sessions::SerialTransferMode::Kermit),
        _ => Err(AppError::Command("串口传输协议无效".to_string())),
    }
}

fn resolve_serial_transfer_path(
    direction: crate::sessions::SerialTransferDirection,
    local_path: &str,
    file_name: Option<&str>,
) -> Result<String, AppError> {
    let path = Path::new(local_path);
    if local_path.trim().is_empty() {
        return Err(AppError::Command("串口传输路径不能为空".to_string()));
    }
    match direction {
        crate::sessions::SerialTransferDirection::Send => {
            if !path.is_file() {
                return Err(AppError::Command(
                    "串口发送文件不存在或不是文件".to_string(),
                ));
            }
            Ok(path.to_string_lossy().into_owned())
        }
        crate::sessions::SerialTransferDirection::Receive => {
            if !path.is_dir() {
                return Err(AppError::Command("串口接收目录不存在".to_string()));
            }
            let file_name = file_name
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::Command("串口接收文件名不能为空".to_string()))?;
            if !is_safe_serial_file_name(file_name) {
                return Err(AppError::Command("串口接收文件名无效".to_string()));
            }
            let target = path.join(file_name);
            if target.exists() {
                return Err(AppError::Command(
                    "串口接收目标文件已存在，请更换文件名".to_string(),
                ));
            }
            Ok(target.to_string_lossy().into_owned())
        }
    }
}

fn is_safe_serial_file_name(file_name: &str) -> bool {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        || file_name.ends_with('.')
        || file_name.ends_with(' ')
    {
        return false;
    }

    let stem = file_name
        .trim_end_matches(['.', ' '])
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !is_windows_numbered_device_name(&stem, "COM")
        && !is_windows_numbered_device_name(&stem, "LPT")
}

fn is_windows_numbered_device_name(stem: &str, prefix: &str) -> bool {
    stem.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 1 && suffix.as_bytes()[0].is_ascii_digit() && suffix != "0"
    })
}

fn resolve_serial_transfer_directory(local_path: &str) -> Result<String, AppError> {
    if local_path.trim().is_empty() {
        return Err(AppError::Command("串口传输路径不能为空".to_string()));
    }
    let path = Path::new(local_path);
    if !path.is_dir() {
        return Err(AppError::Command("串口文件传输接收目录不存在".to_string()));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn app_serial_control(
    app: AppHandle,
    tab_id: String,
    action: String,
    value: Option<bool>,
    duration_ms: Option<u64>,
) -> Result<crate::sessions::SerialLineStatus, AppError> {
    let control = parse_serial_control_action(&action)?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_serial = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .is_some_and(|tab| tab.session_type == "serial");
    if !is_serial {
        return Err(AppError::Command("当前会话不是串口会话".to_string()));
    }

    send_worker_cmd(&app, &tab_id, |respond_to| WorkerCmd::SerialControl {
        action: control,
        value,
        duration_ms,
        respond_to,
    })
    .await
}

#[tauri::command]
pub async fn app_serial_transfer(
    app: AppHandle,
    tab_id: String,
    direction: String,
    mode: String,
    local_path: String,
    file_name: Option<String>,
    local_paths: Option<Vec<String>>,
) -> Result<crate::sessions::SerialTransferResult, AppError> {
    let direction = parse_serial_transfer_direction(&direction)?;
    let mode = parse_serial_transfer_mode(&mode)?;
    let resolved_paths = match (direction, mode) {
        (
            crate::sessions::SerialTransferDirection::Send,
            crate::sessions::SerialTransferMode::Ymodem
            | crate::sessions::SerialTransferMode::Zmodem
            | crate::sessions::SerialTransferMode::Kermit,
        ) => {
            let candidates = local_paths
                .filter(|paths| !paths.is_empty())
                .unwrap_or_else(|| vec![local_path.clone()]);
            candidates
                .iter()
                .map(|path| resolve_serial_transfer_path(direction, path, None))
                .collect::<Result<Vec<_>, _>>()?
        }
        (
            crate::sessions::SerialTransferDirection::Receive,
            crate::sessions::SerialTransferMode::Ymodem
            | crate::sessions::SerialTransferMode::Zmodem
            | crate::sessions::SerialTransferMode::Kermit,
        ) => vec![resolve_serial_transfer_directory(&local_path)?],
        _ => vec![resolve_serial_transfer_path(
            direction,
            &local_path,
            file_name.as_deref(),
        )?],
    };
    let local_path = resolved_paths
        .first()
        .cloned()
        .ok_or_else(|| AppError::Command("串口传输路径不能为空".to_string()))?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_serial = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .is_some_and(|tab| tab.session_type == "serial");
    if !is_serial {
        return Err(AppError::Command("当前会话不是串口会话".to_string()));
    }
    let worker_cancellation = state
        .worker_controls
        .read()
        .await
        .get(&tab_id)
        .cloned()
        .ok_or_else(|| AppError::Storage("串口会话未运行".to_string()))?;
    let cancellation = worker_cancellation.child_token();
    {
        let mut active_transfers = state.serial_transfer_cancellations.write().await;
        if active_transfers.contains_key(&tab_id) {
            return Err(AppError::Command(
                "当前串口会话已有文件传输正在进行".to_string(),
            ));
        }
        active_transfers.insert(tab_id.clone(), cancellation.clone());
    }

    let result = send_worker_cmd_with_response_timeout(
        &app,
        &tab_id,
        SERIAL_TRANSFER_RESPONSE_TIMEOUT,
        |respond_to| WorkerCmd::SerialTransfer {
            request: crate::sessions::SerialTransferRequest {
                direction,
                mode,
                local_path,
                local_paths: resolved_paths,
            },
            cancellation: cancellation.clone(),
            respond_to,
        },
    )
    .await;
    if result.is_err() {
        cancellation.cancel();
    }
    state
        .serial_transfer_cancellations
        .write()
        .await
        .remove(&tab_id);
    result
}

#[tauri::command]
pub async fn app_serial_cancel_transfer(app: AppHandle, tab_id: String) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let cancellation = state
        .serial_transfer_cancellations
        .read()
        .await
        .get(&tab_id)
        .cloned()
        .ok_or_else(|| AppError::Command("当前没有进行中的串口文件传输".to_string()))?;
    cancellation.cancel();
    Ok(())
}

#[tauri::command]
pub async fn app_save_session_log(
    app: AppHandle,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    crate::services::session_logs::save_current_session(&app, &tab_id).await
}

#[tauri::command]
pub fn app_get_ui_preferences(app: AppHandle) -> Result<UiPreferences, AppError> {
    let path = crate::storage::state_path(&app)?;
    if path.exists() {
        let content =
            std::fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
        let preferences: UiPreferences = serde_json::from_str(&content)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        Ok(normalize_ui_preferences(preferences))
    } else {
        Ok(UiPreferences {
            theme: DEFAULT_UI_THEME.to_string(),
            locale: DEFAULT_UI_LOCALE.to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: default_auto_check_updates(),
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            file_panel_remember_ratio: default_file_panel_remember_ratio(),
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: default_overview_show_stats(),
            overview_show_recent: default_overview_show_recent(),
            overview_show_all_connections: default_overview_show_all_connections(),
            overview_show_quick_actions: default_overview_show_quick_actions(),
            overview_section_order: default_overview_section_order(),
        })
    }
}

#[tauri::command]
pub fn app_set_ui_preferences(
    app: AppHandle,
    input: UiPreferencesInput,
) -> Result<UiPreferences, AppError> {
    let path = crate::storage::state_path(&app)?;
    let mut preferences = app_get_ui_preferences(app.clone())?;
    let previous_locale = preferences.locale.clone();
    let previous_terminal_zoom_locked = preferences.terminal_zoom_locked;
    let theme_was_provided = input.theme.is_some();
    if let Some(theme) = input.theme {
        preferences.theme = theme;
    }
    if let Some(locale) = input.locale {
        preferences.locale = locale;
    }
    if let Some(theme_config) = input.theme_config {
        let theme_variant = theme_config.variant.clone();
        preferences.theme_config = theme_config;
        if !theme_was_provided {
            preferences.theme = if theme_variant == "light" {
                "default-light".to_string()
            } else {
                "default-dark".to_string()
            };
        }
    }
    if let Some(custom_themes) = input.custom_themes {
        preferences.custom_themes = custom_themes;
    }
    if let Some(auto_check_updates) = input.auto_check_updates {
        preferences.auto_check_updates = auto_check_updates;
    }
    if let Some(update_channel) = input.update_channel {
        preferences.update_channel = update_channel;
    }
    if let Some(terminal_zoom_locked) = input.terminal_zoom_locked {
        preferences.terminal_zoom_locked = terminal_zoom_locked;
    }
    if let Some(file_panel_remember_ratio) = input.file_panel_remember_ratio {
        preferences.file_panel_remember_ratio = file_panel_remember_ratio;
    }
    if let Some(resource_monitoring_metrics) = input.resource_monitoring_metrics {
        preferences.resource_monitoring_metrics = resource_monitoring_metrics;
    }
    if let Some(resource_monitoring_metric_order) = input.resource_monitoring_metric_order {
        preferences.resource_monitoring_metric_order = resource_monitoring_metric_order;
    }
    if let Some(connection_defaults) = input.connection_defaults {
        if let Some(value) = connection_defaults.use_empty_password {
            preferences.connection_defaults.use_empty_password = value;
        }
        if let Some(value) = connection_defaults.enable_exec_channel {
            preferences.connection_defaults.enable_exec_channel = value;
        }
        if let Some(value) = connection_defaults.enable_resource_monitoring {
            preferences.connection_defaults.enable_resource_monitoring = value;
        }
        if let Some(value) = connection_defaults.resource_monitoring_interval_seconds {
            preferences
                .connection_defaults
                .resource_monitoring_interval_seconds = value;
        }
        if let Some(value) = connection_defaults.resource_monitoring_metrics {
            preferences.connection_defaults.resource_monitoring_metrics = value;
        }
        if let Some(value) = connection_defaults.resource_monitoring_metric_order {
            preferences
                .connection_defaults
                .resource_monitoring_metric_order = value;
        }
        if let Some(value) = connection_defaults.reconnect_mode {
            preferences.connection_defaults.reconnect_mode = value;
        }
        if let Some(value) = connection_defaults.legacy_algorithms {
            preferences.connection_defaults.legacy_algorithms = value;
        }
    }
    if let Some(mcp_agent) = input.mcp_agent {
        if let Some(connection_scope) = mcp_agent.connection_scope {
            preferences.mcp_agent.connection_scope = connection_scope;
        }
        if let Some(operation_policy) = mcp_agent.operation_policy {
            preferences.mcp_agent.operation_policy = operation_policy;
        }
        if let Some(default_profile_id) = mcp_agent.default_profile_id {
            preferences.mcp_agent.default_profile_id = default_profile_id;
        }
    }
    if let Some(overview_show_stats) = input.overview_show_stats {
        preferences.overview_show_stats = overview_show_stats;
    }
    if let Some(overview_show_recent) = input.overview_show_recent {
        preferences.overview_show_recent = overview_show_recent;
    }
    if let Some(overview_show_all_connections) = input.overview_show_all_connections {
        preferences.overview_show_all_connections = overview_show_all_connections;
    }
    if let Some(overview_show_quick_actions) = input.overview_show_quick_actions {
        preferences.overview_show_quick_actions = overview_show_quick_actions;
    }
    if let Some(overview_section_order) = input.overview_section_order {
        preferences.overview_section_order = overview_section_order;
    }
    let preferences = normalize_ui_preferences(preferences);
    let content = serde_json::to_string_pretty(&preferences)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    std::fs::write(path, content).map_err(|error| AppError::Storage(error.to_string()))?;
    if previous_locale != preferences.locale
        || previous_terminal_zoom_locked != preferences.terminal_zoom_locked
    {
        if let Err(error) =
            crate::install_localized_application_menu(&app, preferences.locale == "enUS")
        {
            // Preferences are already durable at this point. Do not report the
            // whole save as failed (and invite a duplicate retry) merely because
            // native menu refresh failed on the current platform.
            crate::services::logging::warn(
                &app,
                "ui-preferences",
                format!("failed to refresh native menu: {error}"),
            );
        }
        if previous_locale != preferences.locale {
            if let Err(error) =
                crate::install_localized_tray_menu(&app, preferences.locale == "enUS")
            {
                crate::services::logging::warn(
                    &app,
                    "ui-preferences",
                    format!("failed to refresh tray menu: {error}"),
                );
            }
        }
    }
    let _ = app.emit("app:ui-preferences-changed", &preferences);
    Ok(preferences)
}

/// Toggle terminal font zoom from a native menu item while keeping the
/// renderer and settings page on the same persisted preference/event path.
pub fn app_toggle_terminal_zoom_lock(app: AppHandle) -> Result<UiPreferences, AppError> {
    let current = app_get_ui_preferences(app.clone())?;
    app_set_ui_preferences(
        app,
        UiPreferencesInput {
            theme: None,
            locale: None,
            theme_config: None,
            custom_themes: None,
            auto_check_updates: None,
            update_channel: None,
            terminal_zoom_locked: Some(!current.terminal_zoom_locked),
            file_panel_remember_ratio: None,
            resource_monitoring_metrics: None,
            resource_monitoring_metric_order: None,
            connection_defaults: None,
            mcp_agent: None,
            overview_show_stats: None,
            overview_show_recent: None,
            overview_show_all_connections: None,
            overview_show_quick_actions: None,
            overview_section_order: None,
        },
    )
}

fn normalize_ui_state(value: Value) -> Result<serde_json::Map<String, Value>, AppError> {
    match value {
        Value::Object(mut object) => {
            let mut states = object
                .remove("values")
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default();
            object.remove("version");
            states.extend(object);
            Ok(states)
        }
        Value::Array(items) => Ok(items
            .into_iter()
            .filter_map(|item| {
                let key = item.get("key")?.as_str()?.to_string();
                let value = item.get("value")?.clone();
                Some((key, value))
            })
            .collect()),
        _ => Err(AppError::Serialization("UI 状态文件格式无效".to_string())),
    }
}

fn read_ui_state(app: &AppHandle) -> Result<serde_json::Map<String, Value>, AppError> {
    normalize_ui_state(crate::storage::read_json_object(app, "ui-state.json")?)
}

fn write_ui_state(app: &AppHandle, states: serde_json::Map<String, Value>) -> Result<(), AppError> {
    write_json_object(app, "ui-state.json", &Value::Object(states))
}

#[tauri::command]
pub fn app_list_ai_providers(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    crate::services::ai::list_providers(&app)
}

#[tauri::command]
pub fn app_save_ai_provider(
    app: AppHandle,
    input: crate::services::ai::SaveAiProviderInput,
) -> Result<crate::services::ai::AiProviderSummary, AppError> {
    crate::services::ai::save_provider(&app, input)
}

#[tauri::command]
pub fn app_delete_ai_provider(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    crate::services::ai::delete_provider(&app, &provider_id)
}

#[tauri::command]
pub async fn app_test_ai_provider(
    app: AppHandle,
    input: crate::services::ai::TestAiProviderInput,
) -> Result<crate::services::ai::AiProviderTestResult, AppError> {
    crate::services::ai::test_provider(&app, input).await
}

#[tauri::command]
pub fn app_list_ai_conversations(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiConversationSummary>, AppError> {
    crate::services::ai::list_conversations(&app)
}

#[tauri::command]
pub fn app_get_ai_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::get_conversation(&app, &conversation_id)
}

#[tauri::command]
pub fn app_create_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::CreateAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::create_conversation(&app, input)
}

#[tauri::command]
pub fn app_rename_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::RenameAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::rename_conversation(&app, input)
}

#[tauri::command]
pub async fn app_summarize_ai_conversation_title(
    app: AppHandle,
    input: crate::services::ai::SummarizeAiConversationTitleInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::summarize_conversation_title(&app, input).await
}

#[tauri::command]
pub fn app_delete_ai_conversation(app: AppHandle, conversation_id: String) -> Result<(), AppError> {
    crate::services::ai::delete_conversation(&app, &conversation_id)
}

#[tauri::command]
pub fn app_get_ai_copilot_mode_state(
    window: WebviewWindow,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::get_copilot_mode_state(&window)
}

#[tauri::command]
pub fn app_set_ai_copilot_mode(
    window: WebviewWindow,
    input: crate::services::ai::SetAiCopilotModeInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_copilot_mode(&window, input)
}

#[tauri::command]
pub fn app_set_ai_context_attach(
    window: WebviewWindow,
    input: crate::services::ai::SetAiContextAttachInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_context_attach(&window, input)
}

#[tauri::command]
pub fn app_set_ai_dangerous_command_restrictions(
    window: WebviewWindow,
    input: crate::services::ai::SetAiDangerousCommandRestrictionsInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_dangerous_command_restrictions(&window, input)
}

#[tauri::command]
pub async fn app_create_ai_context_preview(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::CreateAiContextPreviewInput,
) -> Result<crate::services::ai::AiContextPreview, AppError> {
    crate::services::ai::create_context_preview(&app, &window, input).await
}

#[tauri::command]
pub async fn app_start_ai_chat(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::StartAiChatInput,
    channel: Channel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    crate::services::ai::start_chat(&app, &window, input, channel).await
}

#[tauri::command]
pub async fn app_retry_ai_chat(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::RetryAiChatInput,
    channel: Channel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    crate::services::ai::retry_chat(&app, &window, input, channel).await
}

#[tauri::command]
pub fn app_cancel_ai_chat(request_id: String) -> Result<(), AppError> {
    crate::services::ai::cancel_chat(&request_id)
}

#[tauri::command]
pub fn app_get_ui_state_item(app: AppHandle, key: String) -> Result<Option<String>, AppError> {
    Ok(read_ui_state(&app)?
        .get(&key)
        .and_then(Value::as_str)
        .map(ToString::to_string))
}

#[tauri::command]
pub fn app_set_ui_state_item(app: AppHandle, key: String, value: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.insert(key, Value::String(value));
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_remove_ui_state_item(app: AppHandle, key: String) -> Result<(), AppError> {
    let mut states = read_ui_state(&app)?;
    states.remove(&key);
    write_ui_state(&app, states)
}

#[tauri::command]
pub fn app_get_terminal_command_history(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<TerminalCommandHistoryEntry>, AppError> {
    let value = crate::storage::read_json_object(&app, "command-history.json")?;
    Ok(value
        .get(&profile_id)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|entry| serde_json::from_value::<TerminalCommandHistoryEntry>(entry).ok())
        .filter(|entry| !entry.command.trim().is_empty())
        .collect())
}

#[tauri::command]
pub fn app_set_terminal_command_history(
    app: AppHandle,
    profile_id: String,
    entries: Vec<TerminalCommandHistoryEntry>,
) -> Result<(), AppError> {
    let mut value = crate::storage::read_json_object(&app, "command-history.json")?;
    let sanitized = entries
        .into_iter()
        .filter(|entry| !entry.command.trim().is_empty())
        .take(500)
        .collect::<Vec<_>>();
    let object = value
        .as_object_mut()
        .ok_or_else(|| AppError::Serialization("命令历史文件格式无效".to_string()))?;
    object.insert(
        profile_id,
        serde_json::to_value(sanitized)
            .map_err(|error| AppError::Serialization(error.to_string()))?,
    );
    write_json_object(&app, "command-history.json", &value)
}

#[tauri::command]
pub fn app_get_command_send_preferences(
    app: AppHandle,
) -> Result<CommandSendPreferences, AppError> {
    let value = crate::storage::read_json_object(&app, "command-send-preferences.json")?;
    let preferences = serde_json::from_value::<CommandSendPreferences>(value).unwrap_or_default();
    Ok(CommandSendPreferences {
        send_scope: match preferences.send_scope.as_str() {
            "current" | "all-ssh" | "selected-ssh" => preferences.send_scope,
            _ => "current".to_string(),
        },
        ..preferences
    })
}

#[tauri::command]
pub fn app_set_command_send_preferences(
    app: AppHandle,
    preferences: CommandSendPreferences,
) -> Result<(), AppError> {
    if !matches!(
        preferences.send_scope.as_str(),
        "current" | "all-ssh" | "selected-ssh"
    ) {
        return Err(AppError::Command("命令发送范围无效".to_string()));
    }
    let selected_tab_ids = preferences
        .selected_tab_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .take(200)
        .collect::<Vec<_>>();
    write_json_object(
        &app,
        "command-send-preferences.json",
        &serde_json::to_value(CommandSendPreferences {
            selected_tab_ids,
            ..preferences
        })
        .map_err(|error| AppError::Serialization(error.to_string()))?,
    )
}

async fn lock_library_after_transfer_hydration(
    app: &AppHandle,
) -> Result<tokio::sync::OwnedMutexGuard<()>, AppError> {
    // Transfer hydration can emit a cleanup snapshot. Finish it before taking
    // the library lock so that nested snapshot cannot wait on this same lock.
    crate::services::transfers::ensure_loaded(app).await?;
    Ok(app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone()
        .lock_owned()
        .await)
}

#[tauri::command]
pub async fn app_get_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_get_connection_library(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let library_mutation = app
        .state::<crate::services::workspace::WorkspaceState>()
        .library_mutation
        .clone();
    let _guard = library_mutation.lock().await;
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "profiles": profiles,
        "folders": folders,
    }))
}

#[tauri::command]
pub fn app_list_imported_fonts(
    app: AppHandle,
) -> Result<Vec<crate::services::fonts::ImportedFont>, AppError> {
    crate::services::fonts::list(&app)
}

#[tauri::command]
pub async fn app_import_font(
    app: AppHandle,
) -> Result<Option<crate::services::fonts::ImportedFont>, AppError> {
    crate::services::fonts::import(&app).await
}

#[tauri::command]
pub fn app_get_imported_font_data(
    app: AppHandle,
    font_id: String,
) -> Result<Option<String>, AppError> {
    crate::services::fonts::data_url(&app, &font_id)
}

#[tauri::command]
pub fn app_delete_imported_font(app: AppHandle, font_id: String) -> Result<bool, AppError> {
    crate::services::fonts::delete(&app, &font_id)
}

#[tauri::command]
pub fn app_list_ssh_keys(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    crate::services::ssh_keys::list(&app)
}

#[tauri::command]
pub async fn app_select_ssh_key_file(
    app: AppHandle,
) -> Result<Option<serde_json::Value>, AppError> {
    crate::services::ssh_keys::select_file(&app).await
}

#[tauri::command]
pub fn app_import_ssh_key(
    app: AppHandle,
    input: Option<ImportSshKeyInput>,
) -> Result<Option<serde_json::Value>, AppError> {
    let input = input.unwrap_or(ImportSshKeyInput {
        source_path: None,
        content: None,
        note: None,
    });
    let result =
        crate::services::ssh_keys::import(&app, input.source_path, input.content, input.note)?;
    if result.is_some() {
        emit_ssh_keys_changed(&app)?;
    }
    Ok(result)
}

#[tauri::command]
pub fn app_update_ssh_key_note(
    app: AppHandle,
    key_id: String,
    note: String,
) -> Result<serde_json::Value, AppError> {
    let updated = crate::services::ssh_keys::update_note(&app, &key_id, note)?;
    emit_ssh_keys_changed(&app)?;
    Ok(updated)
}

#[tauri::command]
pub fn app_delete_ssh_key(app: AppHandle, key_id: String) -> Result<(), AppError> {
    crate::services::ssh_keys::delete(&app, &key_id)?;
    emit_ssh_keys_changed(&app)
}

fn emit_ssh_keys_changed(app: &AppHandle) -> Result<(), AppError> {
    app.emit("sshKeys:changed", crate::services::ssh_keys::list(app)?)
        .map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_preview_connection_import(
    app: AppHandle,
    source: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    let dialog = rfd::AsyncFileDialog::new()
        .add_filter("Connection files", &["json", "config", "txt"])
        .set_title("选择连接配置或目录");
    let paths = match source.as_deref() {
        Some("folder") => dialog
            .pick_folder()
            .await
            .map(|folder| vec![folder.path().to_path_buf()]),
        Some("files") | None => dialog.pick_files().await.map(|files| {
            files
                .into_iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        }),
        _ => return Err(AppError::Command("导入来源无效".to_string())),
    };
    let Some(paths) = paths else {
        return Ok(None);
    };
    crate::services::connections::create_import_plan_from_paths(&app, paths)
        .await
        .map(Some)
}

#[tauri::command]
pub async fn app_commit_connection_json_import(
    app: AppHandle,
    plan_id: String,
    options: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let selected_ids = options
        .get("selectedItemIds")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let strategy = options
        .get("conflictStrategy")
        .and_then(Value::as_str)
        .unwrap_or("skip");
    crate::services::connections::commit_import_plan(&app, &plan_id, &selected_ids, strategy).await
}

#[tauri::command]
pub async fn app_export_connections(app: AppHandle, format: String) -> Result<bool, AppError> {
    let extension = if format == "compatible" {
        "json"
    } else {
        "fileterm.json"
    };
    let Some(target) = rfd::AsyncFileDialog::new()
        .set_file_name(format!("fileterm-connections.{extension}"))
        .add_filter("JSON", &["json"])
        .save_file()
        .await
    else {
        return Ok(false);
    };
    let bytes = crate::services::connections::export_bundle(&app, &format)?;
    tokio::fs::write(target.path(), bytes)
        .await
        .map_err(|error| AppError::Storage(format!("无法写入导出文件: {error}")))?;
    Ok(true)
}

#[tauri::command]
pub async fn app_export_connections_as_files(
    app: AppHandle,
    format: String,
) -> Result<bool, AppError> {
    let Some(target) = rfd::AsyncFileDialog::new().pick_folder().await else {
        return Ok(false);
    };
    let (profiles, _) = crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let mut used_names = std::collections::HashSet::new();
    for profile in profiles {
        let id = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("connection");
        let name = profile.get("name").and_then(Value::as_str).unwrap_or(id);
        let filename = format!(
            "{}.json",
            crate::services::connections::export_filename(name, id, &mut used_names)
        );
        let payload = if format == "compatible" {
            serde_json::json!({
                "id": profile.get("id"), "name": profile.get("name"),
                "description": profile.get("note"), "conection_type": profile.get("type"),
                "host": profile.get("host"), "port": profile.get("port"),
                "user_name": profile.get("username"), "terminal_encoding": profile.get("encoding"),
                "authentication_type": profile.get("authType"), "password": profile.get("password"),
                "private_key_path": profile.get("privateKeyPath"), "passphrase": profile.get("passphrase"),
                "exec_channel_enable": profile.get("enableExecChannel"),
                "port_forwarding_list": profile.get("forwards"),
            })
        } else {
            serde_json::json!({
                "schemaVersion": 1,
                "generatedAt": crate::services::webdav::export_timestamp(),
                "profiles": [profile],
            })
        };
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        tokio::fs::write(target.path().join(filename), bytes)
            .await
            .map_err(|error| AppError::Storage(format!("无法写入单连接导出: {error}")))?;
    }
    Ok(true)
}

#[tauri::command]
pub fn app_get_webdav_sync_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::get_config(&app)
}

#[tauri::command]
pub fn app_set_webdav_sync_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::save_config(&app, input)
}

#[tauri::command]
pub async fn app_test_webdav_sync(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::test_connection(&app).await
}

#[tauri::command]
pub async fn app_upload_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::webdav::upload(&app, mode.as_deref()).await
}

#[tauri::command]
pub async fn app_download_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let result = crate::services::webdav::download(&app, mode.as_deref()).await?;
    let changed = result.get("imported").and_then(Value::as_u64).unwrap_or(0)
        + result.get("updated").and_then(Value::as_u64).unwrap_or(0);
    if changed > 0 || result.get("mode").and_then(Value::as_str) == Some("overwrite-local") {
        if let Ok(snapshot) = get_workspace_snapshot_unlocked(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn app_get_s3_backup_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::get_config(&app)
}

#[tauri::command]
pub fn app_set_s3_backup_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::save_config(&app, input)
}

#[tauri::command]
pub async fn app_test_s3_backup(app: AppHandle) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::test_connection(&app).await
}

#[tauri::command]
pub async fn app_upload_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::s3_backup::upload(&app, mode.as_deref()).await
}

#[tauri::command]
pub async fn app_download_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let result = crate::services::s3_backup::download(&app, mode.as_deref()).await?;
    let changed = result.get("imported").and_then(Value::as_u64).unwrap_or(0)
        + result.get("updated").and_then(Value::as_u64).unwrap_or(0);
    if changed > 0 || result.get("mode").and_then(Value::as_str) == Some("overwrite-local") {
        if let Ok(snapshot) = get_workspace_snapshot_unlocked(app.clone()).await {
            let _ = app.emit("workspace:snapshot", snapshot);
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn app_workspace_mutation(
    app: AppHandle,
    operation: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    match operation.as_str() {
        "create-profile" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_profile(&app, input)?;
            }
        }
        "create-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_folder(&app, name, parent_id)?;
        }
        "create-command-folder" => {
            let name = payload
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("新建命令分类");
            let parent_id = payload.get("parentId").and_then(|id| id.as_str());
            crate::services::profile_ops::create_command_folder(&app, name, parent_id)?;
        }
        "create-command" => {
            if let Some(input) = payload.get("input").cloned() {
                crate::services::profile_ops::create_command_template(&app, input)?;
            }
        }
        _ => {
            return Err(AppError::Command(format!(
                "Unsupported operation: {operation}"
            )))
        }
    }
    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenWindowInput {
    pub kind: String,
    pub mode: Option<String>,
    pub profile_id: Option<String>,
    pub command_id: Option<String>,
    pub folder_id: Option<String>,
    pub command: Option<String>,
    pub source: Option<String>,
    pub path: Option<String>,
    pub name: Option<String>,
    pub tab_id: Option<String>,
    pub encoding: Option<String>,
}

#[tauri::command]
pub async fn app_open_window(app: AppHandle, input: OpenWindowInput) -> Result<(), AppError> {
    // WebView2 can deadlock when WebviewWindowBuilder is used from a
    // synchronous Tauri command on Windows. Keep the command asynchronous and
    // perform the blocking builder call on a worker thread so the native event
    // loop remains able to finish WebView2 initialization and service every
    // other invoke request.
    tauri::async_runtime::spawn_blocking(move || crate::open_child_window(&app, input))
        .await
        .map_err(|error| AppError::Window(format!("子窗口创建任务失败: {error}")))?
}

fn renderer_approved_close_should_destroy(window_label: &str) -> bool {
    window_label != "main"
}

fn destroy_child_window_after_invoke_reply(window: WebviewWindow) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CHILD_WINDOW_DESTROY_DELAY).await;
        let _ = window.destroy();
    });
}

#[tauri::command]
pub async fn app_window_action(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), AppError> {
    match action.as_str() {
        "show" => {
            if let Err(error) = window.unminimize() {
                crate::services::logging::warn(
                    &app,
                    "window",
                    format!(
                        "unminimize before show failed label={}: {error}",
                        window.label()
                    ),
                );
            }
            window
                .show()
                .map_err(|error| AppError::Window(error.to_string()))?;
            window
                .set_focus()
                .map_err(|error| AppError::Window(error.to_string()))?;
        }
        "minimize" => {
            let _ = window.minimize();
        }
        "toggle-maximize" => {
            if let Ok(true) = window.is_maximized() {
                let _ = window.unmaximize();
            } else {
                let _ = window.maximize();
            }
            let _ = app.emit(
                "app:window-maximized-change",
                window.is_maximized().unwrap_or(false),
            );
        }
        "close" => {
            if !renderer_approved_close_should_destroy(window.label()) {
                // Match Electron: closing the last workspace item requests a
                // normal main-window close. The CloseRequested guard decides
                // whether to hide to tray, quit, or cancel.
                let _ = window.close();
            } else {
                // A child renderer has already approved this close. Destroy it
                // after this command's invoke reply so WebView2 does not try to
                // resolve the callback in an already-destroyed renderer.
                crate::resolve_file_editor_close(&app, &window);
                destroy_child_window_after_invoke_reply(window);
            }
        }
        "hide" => {
            crate::hide_main_window_and_children(&app);
        }
        "request-quit" => {
            crate::request_main_window_close(&app, true);
        }
        "reload" => {
            window
                .reload()
                .map_err(|error| AppError::Window(error.to_string()))?;
        }
        "toggle-devtools" => {
            #[cfg(debug_assertions)]
            {
                if window.is_devtools_open() {
                    window.close_devtools();
                } else {
                    window.open_devtools();
                }
            }
            #[cfg(not(debug_assertions))]
            {
                let _ = window;
            }
        }
        "request-close-window" => crate::request_close_focused_window(&app),
        "quit" => {
            let quit_registry = app.state::<crate::QuitPreparationRegistry>();
            if !quit_registry.try_begin() {
                return Ok(());
            }
            let editors_approved = match crate::request_file_editors_for_quit(&app).await {
                Ok(approved) => approved,
                Err(error) => {
                    quit_registry.cancel();
                    return Err(error);
                }
            };
            if !editors_approved {
                quit_registry.cancel();
                return Ok(());
            }
            // Quit the entire app. Used by the renderer when the user
            // confirms a Cmd+Q / tray-quit request. Persist paused transfer
            // checkpoints before exiting so a restart never silently loses a
            // resumable file.
            if let Err(error) = crate::services::transfers::shutdown(&app).await {
                quit_registry.cancel();
                return Err(error);
            }
            shutdown_session_workers(&app).await;
            // 清除持久化的 home tab UI 状态，确保下次启动只恢复一个默认新建标签页，
            // 而不是上一次退出前残留的所有 home tab。失败仅记录警告，不阻断退出。
            if let Err(error) = app_remove_ui_state_item(app.clone(), "main.tab-ui".to_string()) {
                crate::services::logging::warn(
                    &app,
                    "workspace",
                    format!("failed to clear home tab ui state on quit: {error}"),
                );
            }
            app.exit(0);
        }
        _ => {}
    }
    Ok(())
}

#[tauri::command]
pub fn app_is_window_maximized(window: WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

#[tauri::command]
pub fn app_cancel_file_editor_close(app: AppHandle, window: WebviewWindow) {
    crate::cancel_file_editor_close(&app, &window);
}

#[tauri::command]
pub fn app_show_window_menu(
    app: AppHandle,
    window: WebviewWindow,
    menu_type: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    let kind = crate::WindowMenuKind::try_from(menu_type.as_str())?;
    crate::show_window_context_menu(&app, &window, kind, x, y)
}

// ==========================================
// Phase 3 commands implementation
// ==========================================

pub(crate) async fn get_workspace_snapshot_unlocked(
    app: AppHandle,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    let tabs = state.tabs.read().await.clone();
    let active_tab_id = state.active_tab_id.read().await.clone();
    let mut sessions = state.sessions.read().await.clone();
    let ai_session_revisions = state.ai_session_revisions.read().await.clone();
    for (tab_id, session) in &mut sessions {
        session.ai_session_revision = ai_session_revisions
            .get(tab_id)
            .copied()
            .unwrap_or_default()
            .to_string();
    }
    let transfers = state.transfers.read().await.clone();
    let active_pane_tab_id_by_root = state.active_pane_tab_id_by_root.read().await.clone();

    // Read + heal profiles, then strip secrets before exposing in snapshot.
    let (profiles_with_secrets, folders) =
        crate::services::profile_ops::read_and_heal_profiles(&app)?;
    let profiles: Vec<serde_json::Value> = profiles_with_secrets
        .iter()
        .map(crate::services::profile_ops::strip_secret_fields_public)
        .collect();
    let (command_folders, commands) =
        crate::services::profile_ops::read_and_heal_command_library(&app)?;

    Ok(serde_json::json!({
        "profiles": profiles,
        "folders": folders,
        "commandFolders": command_folders,
        "commandTemplates": commands,
        "tabs": tabs,
        "activeTabId": active_tab_id,
        "transfers": transfers,
        "sessions": sessions,
        "activePaneTabIdByRoot": active_pane_tab_id_by_root,
    }))
}

pub async fn get_workspace_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    get_workspace_snapshot_unlocked(app).await
}

async fn get_workspace_snapshot_and_emit(app: &AppHandle) -> Result<serde_json::Value, AppError> {
    let snapshot = get_workspace_snapshot_unlocked(app.clone()).await?;
    if let Err(error) = app.emit("workspace:snapshot", snapshot.clone()) {
        // Persistence has already succeeded. A failed best-effort broadcast
        // must not turn a successful mutation into a retryable renderer error
        // that can create duplicate folders/commands/profiles.
        crate::services::logging::warn(
            app,
            "workspace",
            format!("failed to broadcast workspace snapshot: {error}"),
        );
    }
    Ok(snapshot)
}

async fn send_worker_cmd<T>(
    app: &AppHandle,
    tab_id: &str,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    send_worker_cmd_with_response_timeout(app, tab_id, WORKER_FILE_RESPONSE_TIMEOUT, make_cmd).await
}

pub(crate) async fn send_worker_cmd_with_response_timeout<T>(
    app: &AppHandle,
    tab_id: &str,
    response_timeout: Duration,
    make_cmd: impl FnOnce(oneshot::Sender<Result<T, String>>) -> WorkerCmd,
) -> Result<T, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let workers = state.workers.read().await;
    let sender = workers
        .get(tab_id)
        .ok_or_else(|| AppError::Storage("Session not found".to_string()))?
        .clone();
    drop(workers);

    let (tx, rx) = oneshot::channel();
    let cmd = make_cmd(tx);
    // 不持有 workers 读锁跨 await：clone sender 后立即释放，避免后续写锁死锁。
    // send 必须超时，worker 卡死时前端能拿到明确错误而不是永久 hang。
    timeout(WORKER_FILE_CMD_SEND_TIMEOUT, sender.send(cmd))
        .await
        .map_err(|_| AppError::Storage("Worker busy: command send timeout".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?;

    let res = timeout(response_timeout, rx)
        .await
        .map_err(|_| AppError::Storage("远程操作超时，请检查连接后重试".to_string()))?
        .map_err(|e| AppError::Storage(e.to_string()))?
        .map_err(AppError::Storage)?;
    Ok(res)
}

async fn refresh_remote_files(app: &AppHandle, tab_id: &str, path: &str) -> Result<(), AppError> {
    let files = send_worker_cmd(app, tab_id, |tx| WorkerCmd::ListRemoteFiles {
        path: path.to_string(),
        respond_to: tx,
    })
    .await?;

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    if let Some(session) = sessions.get_mut(tab_id) {
        session.remote_files = files;
    }
    Ok(())
}

/// Read-only MCP surface for browsing an already-open file-capable session.
/// The MCP adapter intentionally cannot open profiles or access profile
/// secrets; the desktop UI owns both actions and this helper only delegates to
/// an existing protocol worker.
pub(crate) async fn mcp_list_remote_directory(
    app: AppHandle,
    tab_id: String,
    requested_path: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let path = {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let sessions = state.sessions.read().await;
        let session = sessions
            .get(&tab_id)
            .ok_or_else(|| AppError::Command("FileTerm session was not found".to_string()))?;
        if !session.capabilities.files {
            return Err(AppError::Command(
                "This FileTerm session does not provide remote file access".to_string(),
            ));
        }
        requested_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| session.remote_path.clone())
    };

    if path.len() > 4_096 {
        return Err(AppError::Command(
            "Remote path exceeds the FileTerm MCP limit".to_string(),
        ));
    }

    refresh_remote_files(&app, &tab_id, &path).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sessions = state.sessions.read().await;
    let session = sessions.get(&tab_id).ok_or_else(|| {
        AppError::Command("FileTerm session closed while listing directory".to_string())
    })?;
    Ok(serde_json::json!({
        "tabId": tab_id,
        "path": path,
        "items": session.remote_files,
    }))
}

/// Execute a bounded command through a dedicated SSH exec channel. This is
/// separate from the interactive terminal so an external CLI/MCP caller
/// receives deterministic output without stealing the user's PTY input.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn app_execute_remote_command(
    app: AppHandle,
    tab_id: String,
    command: String,
    cwd: Option<String>,
    timeout_ms: Option<u64>,
    sudo_password: Option<String>,
    su_password: Option<String>,
    save_sudo_password: Option<bool>,
    save_su_password: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    let result = crate::services::action_review::execute_remote_command(
        &app,
        crate::services::action_review::RemoteExecRequest {
            tab_id,
            command,
            cwd,
            timeout_ms,
            expected_session_revision: None,
            sudo_password,
            su_password,
            save_sudo_password: save_sudo_password.unwrap_or(false),
            save_su_password: save_su_password.unwrap_or(false),
            allow_local_privileged_prompt: true,
            privileged_prompt_notice: None,
        },
    )
    .await?;
    serde_json::to_value(result).map_err(|error| AppError::Serialization(error.to_string()))
}

fn create_tab_layout(profile_type: &str) -> String {
    match profile_type {
        "ssh" => "terminal-file".to_string(),
        "ftp" => "file-only".to_string(),
        _ => "terminal-only".to_string(),
    }
}

fn start_session_worker(
    tab_id: String,
    profile: serde_json::Value,
    receiver: mpsc::Receiver<WorkerCmd>,
    terminal_input_receiver: Option<mpsc::UnboundedReceiver<String>>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    match profile.get("type").and_then(Value::as_str).unwrap_or("ssh") {
        "ftp" => crate::sessions::ftp::start_ftp_worker(tab_id, profile, receiver, app),
        "telnet" => crate::sessions::telnet::start_telnet_worker(tab_id, profile, receiver, app),
        "serial" => crate::sessions::serial::start_serial_worker(
            tab_id,
            profile,
            receiver,
            app,
            cancellation,
        ),
        _ => crate::sessions::ssh::start_ssh_worker(
            tab_id,
            profile,
            receiver,
            terminal_input_receiver.expect("SSH worker requires a terminal input channel"),
            app,
            cancellation,
        ),
    }
}

async fn stop_session_worker(state: &crate::services::workspace::WorkspaceState, tab_id: &str) {
    crate::sessions::local_terminal::deactivate_local_terminal_runtime(state, tab_id).await;
    if let Some(cancellation) = state
        .serial_transfer_cancellations
        .write()
        .await
        .remove(tab_id)
    {
        cancellation.cancel();
    }
    if let Some(control) = state.worker_controls.write().await.remove(tab_id) {
        // Cancel first: a command sender cannot wake a worker which is inside
        // an SSH read/metrics parse. This also prevents a stale worker from
        // emitting state over a replacement connection after reconnect.
        control.cancel();
    }
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .remove(tab_id);
    state.terminal_inputs.write().await.remove(tab_id);
    let sender = state.workers.write().await.remove(tab_id);
    if let Some(sender) = sender {
        // 超时即放弃：worker 主循环卡死时 channel 已满，send 不进去；
        // 但 sender 已经从 workers map 移除并即将 drop，worker 的
        // `cmd_rx.recv()` 会返回 None 走清理路径，无需依赖这条 Disconnect。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
}

/// Roll back a session that was created for a split pane but could not be
/// attached to the current pane tree. Split creation awaits PTY/SSH startup,
/// so the source tab may be closed or moved by another command before the
/// tree update gets the write lock. Leaving the newly created worker in that
/// case would leak a background PTY that is no longer reachable from the UI.
async fn cleanup_unattached_session(
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
) {
    stop_session_worker(state, tab_id).await;
    crate::services::session_logs::stop_for_tab(state, tab_id).await;
    state.serial_reconnect_attempts.write().await.remove(tab_id);

    state.tabs.write().await.retain(|tab| tab.id != tab_id);
    state.sessions.write().await.remove(tab_id);
    state.local_terminal_launches.write().await.remove(tab_id);
    state.remote_forwards.write().await.remove(tab_id);
    state
        .active_pane_tab_id_by_root
        .write()
        .await
        .retain(|root_id, active_tab_id| root_id != tab_id && active_tab_id != tab_id);
    state.remove_ai_session_revision(tab_id).await;
}

pub async fn shutdown_session_workers(app: &AppHandle) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let transfer_cancellations = state
        .serial_transfer_cancellations
        .write()
        .await
        .drain()
        .map(|(_, cancellation)| cancellation)
        .collect::<Vec<_>>();
    for cancellation in transfer_cancellations {
        cancellation.cancel();
    }
    let controls = state
        .worker_controls
        .write()
        .await
        .drain()
        .map(|(_, control)| control)
        .collect::<Vec<_>>();
    for control in controls {
        control.cancel();
    }
    state.local_terminal_runtime_ids.write().await.clear();
    let local_gates = state
        .local_terminal_runtime_gates
        .write()
        .await
        .drain()
        .map(|(_, gate)| gate)
        .collect::<Vec<_>>();
    for gate in local_gates {
        gate.deactivate().await;
    }
    state.local_terminal_launches.write().await.clear();
    state.terminal_inputs.write().await.clear();
    state.pending_backup_passwords.write().await.clear();
    let senders = state
        .workers
        .write()
        .await
        .drain()
        .map(|(_, sender)| sender)
        .collect::<Vec<_>>();
    for sender in senders {
        // Cmd+Q 退出链路：任何单个卡死 worker 都不能阻塞整体退出。
        // 超时后直接 drop sender，worker 收到 recv()==None 自动清理。
        let _ = timeout(
            WORKER_DISCONNECT_TIMEOUT,
            sender.send(WorkerCmd::Disconnect),
        )
        .await;
    }
    crate::services::session_logs::shutdown(&state).await;
}

/// 为指定 profile 创建并启动一个新的 session（tab + session + worker）。
/// 返回新 tab_id。调用者负责更新 active_tab_id、paneRoot 以及 emit snapshot。
///
/// 抽取自 `app_open_profile`，供 `app_split_tab` 复用：分屏时基于当前 profile
/// 新建一个独立 session，不共享 PTY。
async fn spawn_session_for_profile(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    profile: &serde_json::Value,
    profile_id: &str,
    pane_root_tab_id: Option<String>,
) -> Result<String, AppError> {
    let resolved_profile = resolve_profile_for_session(app, profile)?;
    let profile = &resolved_profile;
    let profile_type = profile
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("ssh");
    let name = profile
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("SSH Session");

    let tab_id = format!("tab-{}", uuid::Uuid::new_v4());
    let capabilities =
        crate::services::workspace::ConnectionCapabilities::for_session_type(profile_type);
    let new_tab = crate::services::WorkspaceTab {
        id: tab_id.clone(),
        profile_id: profile_id.to_string(),
        session_type: profile_type.to_string(),
        title: name.to_string(),
        layout: create_tab_layout(profile_type),
        status: crate::services::WorkspaceTabStatus::Connecting,
        pane_root: None,
        pane_root_tab_id,
    };

    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| profile.get("devicePath").and_then(Value::as_str))
        .unwrap_or("127.0.0.1");
    let port = profile.get("port").and_then(|p| p.as_i64()).unwrap_or(22) as u16;
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root");
    let initial_remote_path = crate::services::workspace::initial_remote_path_for_profile(profile);

    {
        let mut tabs = state.tabs.write().await;
        tabs.push(new_tab);
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            tab_id.clone(),
            crate::services::SessionSnapshot {
                profile_id: profile_id.to_string(),
                ai_session_revision: "0".to_string(),
                access_host: format!("{}:{}", host, port),
                summary: format!("{}@{}", username, host),
                terminal_transcript: "连接主机...\r\n".to_string(),
                remote_path: initial_remote_path,
                shell_cwd: None,
                follow_shell_cwd: true,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: None,
                shell_user: None,
                connected: false,
                system_metrics: None,
                capabilities,
                reconnect_mode: crate::services::workspace::reconnect_mode_for_profile(profile),
            },
        );
    }

    let (tx, rx) = mpsc::channel(100);
    let (terminal_input_tx, terminal_input_rx) = if profile_type == "ssh" {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Some(sender), Some(receiver))
    } else {
        (None, None)
    };
    let worker_control = CancellationToken::new();
    {
        let mut workers = state.workers.write().await;
        workers.insert(tab_id.clone(), tx);
    }
    if let Some(sender) = terminal_input_tx {
        state
            .terminal_inputs
            .write()
            .await
            .insert(tab_id.clone(), sender);
    }
    state
        .worker_controls
        .write()
        .await
        .insert(tab_id.clone(), worker_control.clone());

    if let Err(error) =
        crate::services::session_logs::start_for_tab(app, state, &tab_id, profile).await
    {
        crate::services::logging::warn(
            app,
            "session-log",
            format!("启动会话日志失败 tab={tab_id}: {error}"),
        );
    }

    start_session_worker(
        tab_id.clone(),
        profile.clone(),
        rx,
        terminal_input_rx,
        app.clone(),
        worker_control,
    );

    Ok(tab_id)
}

/// Creates one isolated local PTY and exposes it through the same runtime
/// workspace model as a remote session. A local terminal is deliberately not
/// persisted as a connection profile.
async fn spawn_local_terminal_tab(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    launch: crate::sessions::local_terminal::LocalTerminalLaunch,
    pane_root_tab_id: Option<String>,
    wait_for_startup: bool,
) -> String {
    let tab_id = format!("local-{}", uuid::Uuid::new_v4());
    let is_split_pane = pane_root_tab_id.is_some();
    let capabilities =
        crate::services::workspace::ConnectionCapabilities::for_session_type("local");

    {
        let mut tabs = state.tabs.write().await;
        tabs.push(crate::services::WorkspaceTab {
            id: tab_id.clone(),
            profile_id: "__local_terminal__".to_string(),
            session_type: "local".to_string(),
            title: launch
                .title
                .clone()
                .unwrap_or_else(|| "Local Terminal".to_string()),
            layout: "terminal-only".to_string(),
            status: crate::services::WorkspaceTabStatus::Connecting,
            pane_root: None,
            pane_root_tab_id,
        });
        let mut sessions = state.sessions.write().await;
        sessions.insert(
            tab_id.clone(),
            crate::services::SessionSnapshot {
                profile_id: "__local_terminal__".to_string(),
                ai_session_revision: "0".to_string(),
                access_host: launch.cwd.clone(),
                summary: launch.shell.clone(),
                terminal_transcript: "Starting local shell...\r\n".to_string(),
                remote_path: launch.cwd.clone(),
                shell_cwd: Some(launch.cwd.clone()),
                follow_shell_cwd: false,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: None,
                shell_user: None,
                connected: false,
                system_metrics: None,
                capabilities,
                reconnect_mode: None,
            },
        );
    }

    match start_local_terminal_for_tab(app, state, &tab_id, launch).await {
        Ok(startup) if wait_for_startup => {
            finish_local_terminal_startup(app, &tab_id, startup, !is_split_pane).await;
        }
        Ok(startup) => {
            let startup_app = app.clone();
            let startup_tab_id = tab_id.clone();
            tauri::async_runtime::spawn(async move {
                finish_local_terminal_startup(&startup_app, &startup_tab_id, startup, true).await;
            });
        }
        Err(error) => {
            if is_split_pane {
                crate::sessions::terminal::set_terminal_state_without_snapshot(
                    app,
                    &tab_id,
                    error,
                    crate::services::WorkspaceTabStatus::Error,
                )
                .await;
            } else {
                crate::sessions::terminal::set_terminal_state(
                    app,
                    &tab_id,
                    error,
                    crate::services::WorkspaceTabStatus::Error,
                )
                .await;
            }
        }
    }

    tab_id
}

struct LocalTerminalStartup {
    runtime_id: String,
    ready: oneshot::Receiver<()>,
}

async fn finish_local_terminal_startup(
    app: &AppHandle,
    tab_id: &str,
    startup: LocalTerminalStartup,
    emit_snapshot: bool,
) {
    let _ = timeout(LOCAL_TERMINAL_STARTUP_READY_TIMEOUT, startup.ready).await;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_current_runtime = state
        .local_terminal_runtime_ids
        .read()
        .await
        .get(tab_id)
        .is_some_and(|runtime_id| runtime_id == &startup.runtime_id);
    if !is_current_runtime {
        return;
    }

    if emit_snapshot {
        crate::sessions::terminal::set_terminal_state(
            app,
            tab_id,
            "Local shell started".to_string(),
            crate::services::WorkspaceTabStatus::Connected,
        )
        .await;
    } else {
        crate::sessions::terminal::set_terminal_state_without_snapshot(
            app,
            tab_id,
            "Local shell started".to_string(),
            crate::services::WorkspaceTabStatus::Connected,
        )
        .await;
    }
}

async fn start_local_terminal_for_tab(
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    tab_id: &str,
    launch: crate::sessions::local_terminal::LocalTerminalLaunch,
) -> Result<LocalTerminalStartup, String> {
    let (worker_tx, worker_rx) = mpsc::channel(16);
    let (terminal_input_tx, terminal_input_rx) = mpsc::unbounded_channel();
    let worker_control = CancellationToken::new();
    let runtime_id = uuid::Uuid::new_v4().to_string();
    let runtime_gate = Arc::new(crate::services::workspace::LocalTerminalRuntimeGate::new());
    state
        .workers
        .write()
        .await
        .insert(tab_id.to_string(), worker_tx);
    state
        .terminal_inputs
        .write()
        .await
        .insert(tab_id.to_string(), terminal_input_tx);
    state
        .worker_controls
        .write()
        .await
        .insert(tab_id.to_string(), worker_control.clone());
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .insert(tab_id.to_string(), runtime_id.clone());
    state
        .local_terminal_runtime_gates
        .write()
        .await
        .insert(tab_id.to_string(), runtime_gate.clone());
    state
        .local_terminal_launches
        .write()
        .await
        .insert(tab_id.to_string(), launch.clone());

    let startup_ready = match crate::sessions::local_terminal::start_local_terminal_worker(
        tab_id.to_string(),
        runtime_id.clone(),
        worker_rx,
        terminal_input_rx,
        app.clone(),
        worker_control,
        launch,
        runtime_gate,
    ) {
        Ok(startup_ready) => startup_ready,
        Err(error) => {
            state.workers.write().await.remove(tab_id);
            state.terminal_inputs.write().await.remove(tab_id);
            state.worker_controls.write().await.remove(tab_id);
            state
                .local_terminal_runtime_ids
                .write()
                .await
                .remove(tab_id);
            crate::sessions::local_terminal::deactivate_local_terminal_runtime(state, tab_id).await;
            return Err(error);
        }
    };

    Ok(LocalTerminalStartup {
        runtime_id,
        ready: startup_ready,
    })
}

fn supports_split_panes(session_type: &str) -> bool {
    matches!(session_type, "ssh" | "local")
}

/// Atomically attach a newly created session to the current pane tree.
///
/// This function does not start or stop any session. Keeping the tree
/// mutation synchronous makes it possible for `app_split_tab` to distinguish
/// a successful attachment from a stale source/tree and roll the new session
/// back in the latter case.
fn attach_split_pane_to_tabs(
    tabs: &mut [crate::services::WorkspaceTab],
    source_tab_id: &str,
    new_tab_id: &str,
    split_direction: crate::services::SplitDirection,
) -> Result<String, AppError> {
    if source_tab_id == new_tab_id {
        return Err(AppError::Storage(
            "Source and new pane tab IDs must be different".to_string(),
        ));
    }
    if !tabs.iter().any(|tab| tab.id == new_tab_id) {
        return Err(AppError::Storage("New pane tab vanished".to_string()));
    }

    // 先找 source 是否已经是 root（有 paneRoot）。
    let root_idx = tabs
        .iter()
        .position(|tab| tab.id == source_tab_id && tab.pane_root.is_some());

    if let Some(idx) = root_idx {
        let root_tab = &mut tabs[idx];
        let pane_root = root_tab
            .pane_root
            .as_mut()
            .expect("root_idx only matches tabs with pane_root");
        let replacement = crate::services::PaneNode::Split {
            direction: split_direction,
            children: vec![
                crate::services::PaneNode::Leaf {
                    tab_id: source_tab_id.to_string(),
                },
                crate::services::PaneNode::Leaf {
                    tab_id: new_tab_id.to_string(),
                },
            ],
            weights: vec![0.5, 0.5],
        };
        if !pane_root.replace_leaf(source_tab_id, replacement) {
            return Err(AppError::Storage(
                "Source pane is not present in its root layout".to_string(),
            ));
        }
        return Ok(source_tab_id.to_string());
    }

    // source 可能是某个 root 的 leaf。
    if let Some(idx) = tabs.iter().position(|tab| {
        tab.pane_root
            .as_ref()
            .map(|root| root.leaf_tab_ids().iter().any(|id| id == source_tab_id))
            .unwrap_or(false)
    }) {
        let root_tab = &mut tabs[idx];
        let pane_root = root_tab
            .pane_root
            .as_mut()
            .expect("containing root always has pane_root");
        let replacement = crate::services::PaneNode::Split {
            direction: split_direction,
            children: vec![
                crate::services::PaneNode::Leaf {
                    tab_id: source_tab_id.to_string(),
                },
                crate::services::PaneNode::Leaf {
                    tab_id: new_tab_id.to_string(),
                },
            ],
            weights: vec![0.5, 0.5],
        };
        if !pane_root.replace_leaf(source_tab_id, replacement) {
            return Err(AppError::Storage(
                "Source pane disappeared from its root layout".to_string(),
            ));
        }
        return Ok(root_tab.id.clone());
    }

    // source 是独立 tab，变成新的 split root。
    let source_idx = tabs
        .iter()
        .position(|tab| tab.id == source_tab_id)
        .ok_or_else(|| AppError::Storage("Source tab vanished".to_string()))?;
    tabs[source_idx].pane_root = Some(crate::services::PaneNode::Split {
        direction: split_direction,
        children: vec![
            crate::services::PaneNode::Leaf {
                tab_id: source_tab_id.to_string(),
            },
            crate::services::PaneNode::Leaf {
                tab_id: new_tab_id.to_string(),
            },
        ],
        weights: vec![0.5, 0.5],
    });
    Ok(source_tab_id.to_string())
}

#[tauri::command]
pub async fn app_open_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;
    let profiles = read_json_array(&app, "profiles.json")?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;

    // Match Electron's open lifecycle: recency is about the user's intent to
    // open a connection, not whether the later network handshake succeeds.
    crate::services::profile_ops::touch_profile(&app, &profile_id)?;

    let tab_id = spawn_session_for_profile(&app, &state, profile, &profile_id, None).await?;

    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(tab_id);
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// 分屏：基于当前 profile 新建一个独立 session，并在 pane tree 中把 source leaf
/// 替换为 split(source_leaf, new_leaf)。
///
/// - `direction = "row"`：左右分（垂直分屏），新 pane 在右
/// - `direction = "column"`：上下分（水平分屏），新 pane 在下
///
/// 支持 SSH 与 Local Terminal session。两者都会创建独立 PTY / runtime；
/// FTP / Telnet / Serial 暂不支持分屏。
#[tauri::command]
pub async fn app_split_tab(
    app: AppHandle,
    source_tab_id: String,
    direction: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    let split_direction = match direction.as_str() {
        "row" => crate::services::SplitDirection::Row,
        "column" => crate::services::SplitDirection::Column,
        _ => {
            return Err(AppError::Storage(format!(
                "Invalid split direction: {}",
                direction
            )))
        }
    };

    // 找到 source tab，并解析其所属的顶层 workspace tab。分屏 leaf 始终
    // 归属一个 root，而不是第二个顶栏 tab。
    let (profile_id, session_type, pane_root_tab_id) = {
        let tabs = state.tabs.read().await;
        let source = tabs
            .iter()
            .find(|t| t.id == source_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Tab not found: {}", source_tab_id)))?;
        if !supports_split_panes(&source.session_type) {
            return Err(AppError::Storage(format!(
                "Split pane is only supported for SSH and local sessions, got: {}",
                source.session_type
            )));
        }
        let root_tab_id = source.pane_root_tab_id.clone().unwrap_or_else(|| {
            if source.pane_root.is_some() {
                source.id.clone()
            } else {
                tabs.iter()
                    .find(|tab| {
                        tab.pane_root
                            .as_ref()
                            .map(|root| root.leaf_tab_ids().iter().any(|id| id == &source_tab_id))
                            .unwrap_or(false)
                    })
                    .map(|tab| tab.id.clone())
                    .unwrap_or_else(|| source.id.clone())
            }
        });
        if let Some(root) = tabs.iter().find(|tab| tab.id == root_tab_id) {
            if let Some(pane_root) = &root.pane_root {
                let has_mixed_session_types = pane_root.leaf_tab_ids().iter().any(|leaf_tab_id| {
                    tabs.iter()
                        .find(|tab| &tab.id == leaf_tab_id)
                        .map(|tab| tab.session_type != source.session_type)
                        .unwrap_or(true)
                });
                if has_mixed_session_types {
                    return Err(AppError::Storage(
                        "Split pane tree contains incompatible session types".to_string(),
                    ));
                }
            }
        }
        (
            source.profile_id.clone(),
            source.session_type.clone(),
            root_tab_id,
        )
    };

    // 创建新 session（不 touch_profile，分屏不算独立打开）。本地终端复用当前
    // pane 的启动参数及已捕获 CWD，但始终新建 runtime、worker 与 PTY。
    let new_tab_id = match session_type.as_str() {
        "ssh" => {
            let profiles = read_json_array(&app, "profiles.json")?;
            let profile = profiles
                .iter()
                .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&profile_id))
                .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
            spawn_session_for_profile(&app, &state, profile, &profile_id, Some(pane_root_tab_id))
                .await?
        }
        "local" => {
            let mut launch = state
                .local_terminal_launches
                .read()
                .await
                .get(&source_tab_id)
                .cloned()
                .ok_or_else(|| {
                    AppError::Storage("Local terminal launch settings are unavailable".to_string())
                })?;
            if let Some(cwd) = state
                .sessions
                .read()
                .await
                .get(&source_tab_id)
                .and_then(|session| session.shell_cwd.clone())
            {
                launch.cwd = cwd;
            }
            spawn_local_terminal_tab(&app, &state, launch, Some(pane_root_tab_id), true).await
        }
        _ => unreachable!("session type is checked before creating a split pane"),
    };

    // 更新 paneRoot，并保留承载该分屏树的 root tab id。若 source 在异步
    // 创建新会话期间消失，必须回收刚创建的 worker/PTY，不能留下孤儿会话。
    let root_tab_id = {
        let mut tabs = state.tabs.write().await;
        match attach_split_pane_to_tabs(&mut tabs, &source_tab_id, &new_tab_id, split_direction) {
            Ok(root_tab_id) => {
                let new_tab = tabs
                    .iter_mut()
                    .find(|tab| tab.id == new_tab_id)
                    .expect("attach_split_pane_to_tabs validates the new tab");
                // The source may have been promoted into another root while
                // the new worker was starting. Persist the root that actually
                // accepted the pane instead of the root captured beforehand.
                new_tab.pane_root_tab_id = Some(root_tab_id.clone());
                root_tab_id
            }
            Err(error) => {
                drop(tabs);
                cleanup_unattached_session(&state, &new_tab_id).await;
                return Err(error);
            }
        }
    };

    // 无论当前 source 是 root、leaf 还是独立 tab，顶层都必须停留在 root。
    // 新 session 只作为 active pane，不得成为一个新的顶栏 tab。
    {
        let mut active_tab = state.active_tab_id.write().await;
        *active_tab = Some(root_tab_id.clone());

        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id.clone(), new_tab_id.clone());
    }
    state.touch_ai_session_revision(&source_tab_id).await;
    state.touch_ai_session_revision(&new_tab_id).await;

    get_workspace_snapshot_and_emit(&app).await
}

#[derive(Debug, PartialEq, Eq)]
struct PaneCloseOutcome {
    root_tab_id: String,
    remaining_pane_tab_ids: Vec<String>,
    keeps_split: bool,
}

/// Remove a leaf from a split root while preserving the invariant that a
/// top-level tab is always backed by a live session. When the original root
/// leaf is closed, a surviving leaf is promoted to be the new root rather
/// than leaving a tree whose container points at a closed terminal.
fn remove_split_pane_from_tabs(
    tabs: &mut Vec<crate::services::WorkspaceTab>,
    root_tab_id: &str,
    pane_tab_id: &str,
) -> Result<PaneCloseOutcome, AppError> {
    let root_idx = tabs
        .iter()
        .position(|tab| {
            tab.id == root_tab_id
                || tab
                    .pane_root
                    .as_ref()
                    .map(|r| r.leaf_tab_ids().iter().any(|id| id == pane_tab_id))
                    .unwrap_or(false)
        })
        .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
    let actual_root_tab_id = tabs[root_idx].id.clone();
    let mut next_pane_root = tabs[root_idx]
        .pane_root
        .clone()
        .ok_or_else(|| AppError::Storage("Tab does not have a split pane layout".to_string()))?;
    let before = next_pane_root.leaf_tab_ids();
    if before.len() < 2 {
        return Err(AppError::Storage(
            "Cannot close the only pane through the split-pane command".to_string(),
        ));
    }
    if !before.iter().any(|id| id == pane_tab_id) {
        return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
    }

    next_pane_root.remove_leaf(pane_tab_id);
    let remaining_pane_tab_ids = next_pane_root.leaf_tab_ids();
    let keeps_split = remaining_pane_tab_ids.len() > 1;

    if pane_tab_id != actual_root_tab_id {
        let root_tab = &mut tabs[root_idx];
        root_tab.pane_root = keeps_split.then_some(next_pane_root);
        tabs.retain(|tab| tab.id != pane_tab_id);
        return Ok(PaneCloseOutcome {
            root_tab_id: actual_root_tab_id,
            remaining_pane_tab_ids,
            keeps_split,
        });
    }

    // The original root session is itself a leaf. If it is the pane being
    // closed, turn the first surviving session into the new top-level tab and
    // repoint all remaining leaves at it. This mirrors Ghostty's separation of
    // tab container and terminal surface without requiring us to rename a
    // running SSH worker.
    let promoted_root_tab_id = remaining_pane_tab_ids
        .first()
        .cloned()
        .ok_or_else(|| AppError::Storage("No surviving pane to promote as root tab".to_string()))?;

    let _root_tab = tabs.remove(root_idx);
    let promoted_idx = tabs
        .iter()
        .position(|tab| tab.id == promoted_root_tab_id)
        .ok_or_else(|| {
            AppError::Storage(format!(
                "Promoted pane tab not found: {promoted_root_tab_id}"
            ))
        })?;

    let mut promoted_tab = tabs.remove(promoted_idx);
    promoted_tab.pane_root_tab_id = None;
    promoted_tab.pane_root = keeps_split.then_some(next_pane_root);

    for tab in tabs.iter_mut() {
        if tab.pane_root_tab_id.as_deref() == Some(actual_root_tab_id.as_str()) {
            tab.pane_root_tab_id = Some(promoted_root_tab_id.clone());
        }
    }

    let insert_idx = root_idx.min(tabs.len());
    tabs.insert(insert_idx, promoted_tab);

    Ok(PaneCloseOutcome {
        root_tab_id: promoted_root_tab_id,
        remaining_pane_tab_ids,
        keeps_split,
    })
}

/// 关闭分屏中的单个 pane。若 pane tree 只剩一个 leaf，退化回普通 tab。
#[tauri::command]
pub async fn app_close_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let _library_guard = lock_library_after_transfer_hydration(&app).await?;

    // Validate before stopping the session. Closing the final pane is a
    // top-level-tab action and must go through its confirmation flow instead.
    let resolved_root_tab_id = {
        let tabs = state.tabs.read().await;
        let root_tab = tabs
            .iter()
            .find(|tab| {
                tab.id == root_tab_id
                    || tab
                        .pane_root
                        .as_ref()
                        .map(|r| r.leaf_tab_ids().iter().any(|id| id == &pane_tab_id))
                        .unwrap_or(false)
            })
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {root_tab_id}")))?;
        let leaves = root_tab
            .pane_root
            .as_ref()
            .map(crate::services::PaneNode::leaf_tab_ids)
            .ok_or_else(|| {
                AppError::Storage("Tab does not have a split pane layout".to_string())
            })?;
        if leaves.len() < 2 {
            return Err(AppError::Storage(
                "Cannot close the only pane through the split-pane command".to_string(),
            ));
        }
        if !leaves.iter().any(|id| id == &pane_tab_id) {
            return Err(AppError::Storage(format!("Pane not found: {pane_tab_id}")));
        }
        root_tab.id.clone()
    };

    let previous_active_pane = state
        .active_pane_tab_id_by_root
        .read()
        .await
        .get(&resolved_root_tab_id)
        .cloned();

    // 暂停该 pane 关联的传输，并清理对应 worker。
    let _ = crate::services::transfers::pause_for_tab(
        &app,
        &pane_tab_id,
        "Pane 关闭后已暂停，可在重连后继续传输",
    )
    .await;
    stop_session_worker(&state, &pane_tab_id).await;
    crate::services::session_logs::stop_for_tab(&state, &pane_tab_id).await;
    state
        .serial_reconnect_attempts
        .write()
        .await
        .remove(&pane_tab_id);

    let outcome = {
        let mut tabs = state.tabs.write().await;
        remove_split_pane_from_tabs(&mut tabs, &resolved_root_tab_id, &pane_tab_id)?
    };

    state.sessions.write().await.remove(&pane_tab_id);
    state.remove_ai_session_revision(&pane_tab_id).await;

    {
        let mut active_tab = state.active_tab_id.write().await;
        if active_tab.as_deref() == Some(root_tab_id.as_str()) {
            *active_tab = Some(outcome.root_tab_id.clone());
        }
    }

    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.remove(&root_tab_id);
        if outcome.keeps_split {
            let next_active_pane = previous_active_pane
                .filter(|id| id != &pane_tab_id && outcome.remaining_pane_tab_ids.contains(id))
                .unwrap_or_else(|| outcome.remaining_pane_tab_ids[0].clone());
            active_panes.insert(outcome.root_tab_id, next_active_pane);
        }
    }
    for remaining_tab_id in &outcome.remaining_pane_tab_ids {
        state.touch_ai_session_revision(remaining_tab_id).await;
    }

    get_workspace_snapshot_and_emit(&app).await
}

/// 设置分屏中的活跃 pane。
#[tauri::command]
pub async fn app_set_active_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let previous_pane_tab_id = {
        let active_panes = state.active_pane_tab_id_by_root.read().await;
        active_panes.get(&root_tab_id).cloned()
    };
    {
        let mut active_panes = state.active_pane_tab_id_by_root.write().await;
        active_panes.insert(root_tab_id, pane_tab_id.clone());
    }
    if previous_pane_tab_id.as_deref() != Some(pane_tab_id.as_str()) {
        if let Some(previous_pane_tab_id) = previous_pane_tab_id {
            state.touch_ai_session_revision(&previous_pane_tab_id).await;
        }
        state.touch_ai_session_revision(&pane_tab_id).await;
    }
    get_workspace_snapshot(app).await
}

/// 持久化分屏 weights（拖拽 resize 结束时调用）。
#[tauri::command]
pub async fn app_set_pane_weights(
    app: AppHandle,
    root_tab_id: String,
    pane_path: Vec<usize>,
    weights: Vec<f32>,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut tabs = state.tabs.write().await;
        let root_idx = tabs
            .iter()
            .position(|t| t.id == root_tab_id)
            .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_tab_id)))?;
        let root_tab = &mut tabs[root_idx];
        if let Some(ref mut pane_root) = root_tab.pane_root {
            if !pane_root.set_split_weights_at_path(&pane_path, &weights) {
                return Err(AppError::Storage(
                    "Split pane path or weights are invalid".to_string(),
                ));
            }
        } else {
            return Err(AppError::Storage(
                "Tab does not have a split pane layout".to_string(),
            ));
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_activate_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    // A pane leaf owns a session but never owns a top-level tab. Normalizing
    // here makes the invariant hold even for stale UI events.
    let top_level_tab_id = state
        .tabs
        .read()
        .await
        .iter()
        .find(|tab| tab.id == tab_id)
        .and_then(|tab| tab.pane_root_tab_id.clone())
        .unwrap_or(tab_id);
    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(top_level_tab_id);
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_reconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let tab_metadata = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .map(|t| (t.profile_id.clone(), t.session_type.clone()))
    };

    if let Some((profile_id, session_type)) = tab_metadata {
        if session_type == "local" {
            let should_start = {
                let mut tabs = state.tabs.write().await;
                claim_reconnect_tab(&mut tabs, &tab_id)
            };
            if !should_start {
                return get_workspace_snapshot(app).await;
            }

            stop_session_worker(&state, &tab_id).await;
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    session.connected = false;
                    if !session.terminal_transcript.is_empty() {
                        session
                            .terminal_transcript
                            .push_str("\r\n--- Local shell restarted ---\r\n");
                    }
                    session
                        .terminal_transcript
                        .push_str("Starting local shell...\r\n");
                }
            }
            state.touch_ai_session_revision(&tab_id).await;

            let mut launch = state
                .local_terminal_launches
                .read()
                .await
                .get(&tab_id)
                .cloned()
                .unwrap_or_else(crate::sessions::local_terminal::default_launch);
            if let Some(cwd) = state
                .sessions
                .read()
                .await
                .get(&tab_id)
                .and_then(|session| session.shell_cwd.clone())
            {
                launch.cwd = cwd;
            }
            match start_local_terminal_for_tab(&app, &state, &tab_id, launch).await {
                Ok(startup) => {
                    let startup_app = app.clone();
                    let startup_tab_id = tab_id.clone();
                    tauri::async_runtime::spawn(async move {
                        finish_local_terminal_startup(&startup_app, &startup_tab_id, startup, true)
                            .await;
                    });
                }
                Err(error) => {
                    crate::sessions::terminal::set_terminal_state(
                        &app,
                        &tab_id,
                        error,
                        crate::services::WorkspaceTabStatus::Error,
                    )
                    .await;
                }
            }
            return get_workspace_snapshot(app).await;
        }

        let pid = profile_id;
        let profiles = read_json_array(&app, "profiles.json")?;
        if let Some(profile) = profiles
            .iter()
            .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(&pid))
        {
            let resolved_profile = resolve_profile_for_session(&app, profile)?;
            let profile = &resolved_profile;
            // Claim the reconnect before awaiting worker shutdown. Tauri can
            // dispatch Enter/button/auto-reconnect commands concurrently; a
            // status check performed after an await lets each caller replace
            // the worker and append another reconnect banner.
            let should_start = {
                let mut tabs = state.tabs.write().await;
                claim_reconnect_tab(&mut tabs, &tab_id)
            };
            if !should_start {
                return get_workspace_snapshot(app).await;
            }

            // Terminate existing worker
            stop_session_worker(&state, &tab_id).await;

            // Set connecting status. Preserve the existing transcript so the
            // renderer can re-hydrate the terminal with prior history on
            // reconnect (mirrors Electron's BoundedTextBuffer retention).
            // We only append a separator + "连接主机..." notice so the user
            // sees that a reconnect is in progress.
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    session.connected = false;
                    session.remote_files_loading = false;
                    session.shell_user = None;
                    session.file_access_mode = "user".to_string();
                    session.has_reusable_sudo_auth = false;
                    session.reconnect_mode =
                        crate::services::workspace::reconnect_mode_for_profile(profile);
                    // Append a reconnect separator instead of wiping history.
                    if !session.terminal_transcript.is_empty() {
                        session
                            .terminal_transcript
                            .push_str("\r\n--- 重新连接 ---\r\n");
                    }
                    session.terminal_transcript.push_str("连接主机...\r\n");
                    // Cap to 200k chars (matches Electron's BoundedTextBuffer).
                    if session.terminal_transcript.len() > 200_000 {
                        let mut cut = session.terminal_transcript.len() - 180_000;
                        while cut < session.terminal_transcript.len()
                            && !session.terminal_transcript.is_char_boundary(cut)
                        {
                            cut += 1;
                        }
                        session.terminal_transcript =
                            session.terminal_transcript[cut..].to_string();
                    }
                    session.remote_files = Vec::new();
                    session.system_metrics = None;
                }
            }
            state.touch_ai_session_revision(&tab_id).await;

            // Renderer-triggered reconnects apply the returned snapshot, but
            // auto-reconnect is initiated by the worker and has no renderer
            // caller to apply it. Broadcast the connecting snapshot for both
            // paths so the terminal/file panes cannot remain on stale state.
            if let Ok(snapshot) = get_workspace_snapshot(app.clone()).await {
                let _ = app.emit("workspace:snapshot", snapshot);
            }

            let (tx, rx) = mpsc::channel(100);
            let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("ssh");
            let (terminal_input_tx, terminal_input_rx) = if profile_type == "ssh" {
                let (sender, receiver) = mpsc::unbounded_channel();
                (Some(sender), Some(receiver))
            } else {
                (None, None)
            };
            let worker_control = CancellationToken::new();
            {
                let mut workers = state.workers.write().await;
                workers.insert(tab_id.clone(), tx);
            }
            if let Some(sender) = terminal_input_tx {
                state
                    .terminal_inputs
                    .write()
                    .await
                    .insert(tab_id.clone(), sender);
            }
            state
                .worker_controls
                .write()
                .await
                .insert(tab_id.clone(), worker_control.clone());

            if let Err(error) =
                crate::services::session_logs::start_for_tab(&app, &state, &tab_id, profile).await
            {
                crate::services::logging::warn(
                    &app,
                    "session-log",
                    format!("启动会话日志失败 tab={tab_id}: {error}"),
                );
            }

            start_session_worker(
                tab_id,
                profile.clone(),
                rx,
                terminal_input_rx,
                app.clone(),
                worker_control,
            );
        }
    }

    get_workspace_snapshot(app).await
}

fn claim_reconnect_tab(tabs: &mut [crate::services::WorkspaceTab], tab_id: &str) -> bool {
    let Some(tab) = tabs.iter_mut().find(|tab| tab.id == tab_id) else {
        return false;
    };
    if tab.status == crate::services::WorkspaceTabStatus::Connecting {
        return false;
    }
    tab.status = crate::services::WorkspaceTabStatus::Connecting;
    true
}

#[tauri::command]
pub async fn app_disconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    let is_local_terminal = app
        .state::<crate::services::workspace::WorkspaceState>()
        .tabs
        .read()
        .await
        .iter()
        .any(|tab| tab.id == tab_id && tab.session_type == "local");
    if is_local_terminal {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        stop_session_worker(&state, &tab_id).await;
        crate::sessions::terminal::set_terminal_state(
            &app,
            &tab_id,
            "Local shell stopped".to_string(),
            crate::services::WorkspaceTabStatus::Closed,
        )
        .await;
        return get_workspace_snapshot(app).await;
    }

    crate::services::transfers::pause_for_tab(&app, &tab_id, "连接断开，可在重连后继续传输")
        .await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let was_connected = state
        .sessions
        .read()
        .await
        .get(&tab_id)
        .map(|session| session.connected)
        .unwrap_or(false);
    stop_session_worker(&state, &tab_id).await;
    state
        .serial_reconnect_attempts
        .write()
        .await
        .remove(&tab_id);
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = crate::services::WorkspaceTabStatus::Closed;
        }
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.connected = false;
            session.remote_files_loading = false;
            session.remote_files = Vec::new();
            session.shell_user = None;
            session.file_access_mode = "user".to_string();
            session.has_reusable_sudo_auth = false;
            session.system_metrics = None;
        }
    }
    state.touch_ai_session_revision(&tab_id).await;

    // Cancelling an SSH worker intentionally suppresses its normal worker
    // shutdown callback. Emit the same terminal notice/state that a network
    // disconnect would have emitted, otherwise the renderer only receives a
    // workspace snapshot and keeps showing the last shell prompt forever.
    if was_connected {
        crate::sessions::terminal::emit_terminal_data(&app, &tab_id, "\r\n连接已断开\r\n").await;
    }
    crate::sessions::terminal::set_terminal_state(
        &app,
        &tab_id,
        "连接已断开".to_string(),
        crate::services::WorkspaceTabStatus::Closed,
    )
    .await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_close_tab(app: AppHandle, tab_id: String) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();

    // 检查是否是分屏 root：如果是，关闭所有 leaf 的 worker 和传输
    let pane_leaf_ids: Vec<String> = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.pane_root.as_ref())
            .map(|root| root.leaf_tab_ids())
            .unwrap_or_default()
    };

    // 检查是否是某个 root 的 leaf
    let containing_root_id: Option<String> = if pane_leaf_ids.is_empty() {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|t| {
                t.pane_root
                    .as_ref()
                    .map(|root| root.leaf_tab_ids().iter().any(|id| id == &tab_id))
                    .unwrap_or(false)
            })
            .map(|t| t.id.clone())
    } else {
        None
    };

    if let Some(root_id) = containing_root_id {
        // tab_id 是某个 root 的 leaf，等价于 close_pane
        // 暂停传输
        let _ = crate::services::transfers::pause_for_tab(
            &app,
            &tab_id,
            "Pane 关闭后已暂停，可在重连后继续传输",
        )
        .await;
        stop_session_worker(&state, &tab_id).await;
        crate::services::session_logs::stop_for_tab(&state, &tab_id).await;
        state
            .serial_reconnect_attempts
            .write()
            .await
            .remove(&tab_id);
        {
            let mut tabs = state.tabs.write().await;
            let root_idx = tabs
                .iter()
                .position(|t| t.id == root_id)
                .ok_or_else(|| AppError::Storage(format!("Root tab not found: {}", root_id)))?;
            {
                let root_tab = &mut tabs[root_idx];
                if let Some(ref mut pane_root) = root_tab.pane_root {
                    pane_root.remove_leaf(&tab_id);
                    if let crate::services::PaneNode::Leaf { .. } = pane_root {
                        root_tab.pane_root = None;
                    }
                }
            }
            tabs.retain(|t| t.id != tab_id);
            let mut sessions = state.sessions.write().await;
            sessions.remove(&tab_id);
            state.local_terminal_launches.write().await.remove(&tab_id);
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            if let Some(root_tab) = tabs.get(root_idx) {
                if root_tab.pane_root.is_none() {
                    active_panes.remove(&root_id);
                } else if let Some(ref pane_root) = root_tab.pane_root {
                    let leaves = pane_root.leaf_tab_ids();
                    if active_panes
                        .get(&root_id)
                        .map(|id| id == &tab_id || !leaves.contains(id))
                        .unwrap_or(true)
                    {
                        if let Some(first) = leaves.first() {
                            active_panes.insert(root_id.clone(), first.clone());
                        }
                    }
                }
            }
        }
        state.remove_ai_session_revision(&tab_id).await;
    } else {
        // 普通关闭（可能是独立 tab 或分屏 root）
        let all_ids_to_close = if pane_leaf_ids.is_empty() {
            vec![tab_id.clone()]
        } else {
            pane_leaf_ids
        };

        for id in &all_ids_to_close {
            crate::services::transfers::pause_for_tab(
                &app,
                id,
                "标签关闭后已暂停，可在重连后继续传输",
            )
            .await?;
            stop_session_worker(&state, id).await;
            crate::services::session_logs::stop_for_tab(&state, id).await;
            state.serial_reconnect_attempts.write().await.remove(id);
        }
        {
            let mut tabs = state.tabs.write().await;
            tabs.retain(|t| !all_ids_to_close.contains(&t.id));

            let mut active = state.active_tab_id.write().await;
            if *active == Some(tab_id.clone()) {
                *active = tabs.last().map(|t| t.id.clone());
            }

            let mut sessions = state.sessions.write().await;
            for id in &all_ids_to_close {
                sessions.remove(id);
                state.local_terminal_launches.write().await.remove(id);
            }
            let mut active_panes = state.active_pane_tab_id_by_root.write().await;
            active_panes.remove(&tab_id);
        }
        for id in &all_ids_to_close {
            state.remove_ai_session_revision(id).await;
        }
    }

    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_open_local_terminal(
    app: AppHandle,
    options: Option<crate::sessions::local_terminal::LocalTerminalLaunchOptions>,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let launch =
        crate::sessions::local_terminal::resolve_launch(options).map_err(AppError::Command)?;
    let tab_id = spawn_local_terminal_tab(&app, &state, launch, None, false).await;
    {
        let mut active = state.active_tab_id.write().await;
        *active = Some(tab_id);
    }
    // The renderer replaces the active home tab with the returned session in
    // the same turn. Emitting here races that replacement and briefly exposes
    // the new session as an additional tab before the old placeholder closes.
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_write_terminal(
    app: AppHandle,
    tab_id: String,
    data: String,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    send_terminal_input(&state, &tab_id, data).await
}

#[tauri::command]
pub fn app_subscribe_terminal_data(app: AppHandle, channel: Channel<serde_json::Value>) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.register_terminal_output_channel(channel);
}

#[tauri::command]
pub async fn app_resize_terminal(
    app: AppHandle,
    tab_id: String,
    cols: u32,
    rows: u32,
    width: u32,
    height: u32,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let workers = state.workers.read().await;
    if let Some(sender) = workers.get(&tab_id) {
        let _ = timeout(
            WORKER_CMD_SEND_TIMEOUT,
            sender.send(WorkerCmd::ResizeTerminal {
                cols,
                rows,
                width,
                height,
            }),
        )
        .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn app_open_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
) -> Result<serde_json::Value, AppError> {
    refresh_remote_files(&app, &tab_id, &target_path).await?;
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let target_changed = {
        let sessions = state.sessions.read().await;
        sessions
            .get(&tab_id)
            .map(|session| session.shell_cwd.is_none() && session.remote_path != target_path)
            .unwrap_or(false)
    };
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.remote_path = target_path;
        }
    }
    if target_changed {
        state.touch_ai_session_revision(&tab_id).await;
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    enabled: bool,
) -> Result<serde_json::Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let is_local_terminal = state
        .tabs
        .read()
        .await
        .iter()
        .any(|tab| tab.id == tab_id && tab.session_type == "local");
    if is_local_terminal {
        {
            let mut sessions = state.sessions.write().await;
            if let Some(session) = sessions.get_mut(&tab_id) {
                session.follow_shell_cwd = enabled;
                if enabled {
                    if let Some(cwd) = session.shell_cwd.clone() {
                        session.remote_path = cwd;
                    }
                }
            }
        }
        return get_workspace_snapshot(app).await;
    }

    let cwd_to_follow = {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(&tab_id) {
            session.follow_shell_cwd = enabled;
            if enabled && session.shell_cwd.as_deref() != Some(session.remote_path.as_str()) {
                session.shell_cwd.clone()
            } else {
                None
            }
        } else {
            None
        }
    };

    // Match Electron's recovery behaviour: enabling follow must immediately
    // catch the file pane up to the most recently reported shell directory.
    // Waiting for another `cd` leaves the toggle active while the pane remains
    // stale forever when the initial listing happened to fail.
    if let Some(cwd) = cwd_to_follow {
        match refresh_remote_files(&app, &tab_id, &cwd).await {
            Ok(()) => {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(&tab_id) {
                    if session.follow_shell_cwd
                        && session.shell_cwd.as_deref() == Some(cwd.as_str())
                    {
                        session.remote_path = cwd;
                    }
                }
            }
            Err(error) => {
                // CWD reporting is best-effort in Electron too. A directory
                // the SFTP user cannot read must not make the toggle itself
                // fail or interfere with the interactive terminal.
                crate::services::logging::ssh_debug(
                    &app,
                    &tab_id,
                    format!("CWD follow recovery failed for {cwd}: {error}"),
                );
            }
        }
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_read_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ReadRemoteFile {
        path: target_path,
        encoding: enc,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_write_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::WriteRemoteFile {
        path: target_path.clone(),
        content,
        encoding: enc,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_directory(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateRemoteDirectory {
        parent_path: parent_path.clone(),
        name,
        respond_to: tx,
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_create_remote_file(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateRemoteFile {
        parent_path: parent_path.clone(),
        name,
        respond_to: tx,
    })
    .await?;

    let _ = refresh_remote_files(&app, &tab_id, &parent_path).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_copy_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CopyRemotePath {
        target_path,
        destination_path: destination_path.clone(),
        target_type,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_move_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::MoveRemotePath {
        target_path: target_path.clone(),
        destination_path: destination_path.clone(),
        respond_to: tx,
    })
    .await?;

    let parent_src = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let parent_dest = std::path::Path::new(&destination_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());

    let _ = refresh_remote_files(&app, &tab_id, &parent_src).await;
    if parent_src != parent_dest {
        let _ = refresh_remote_files(&app, &tab_id, &parent_dest).await;
    }
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_rename_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::RenameRemotePath {
        target_path: target_path.clone(),
        new_name,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_delete_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::DeleteRemotePath {
        target_path: target_path.clone(),
        target_type,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PermissionApplyTarget {
    All,
    Files,
    Directories,
}

impl PermissionApplyTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Directories => "directories",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemotePermissionChangeOptions {
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: Option<PermissionApplyTarget>,
}

fn parse_remote_permission_mode(mode: &str) -> Result<u32, AppError> {
    let trimmed = mode.trim();
    if !(3..=4).contains(&trimmed.len())
        || !trimmed
            .chars()
            .all(|character| matches!(character, '0'..='7'))
    {
        return Err(AppError::Command(
            "权限值必须是 3 到 4 位八进制数字，例如 755".to_string(),
        ));
    }
    u32::from_str_radix(trimmed, 8).map_err(|error| AppError::Command(error.to_string()))
}

#[tauri::command]
pub async fn app_change_remote_permissions(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    options: RemotePermissionChangeOptions,
) -> Result<serde_json::Value, AppError> {
    let permissions = parse_remote_permission_mode(&options.mode)?;
    let recursive = options.recursive;
    let apply_to = options
        .apply_to
        .unwrap_or(PermissionApplyTarget::All)
        .as_str()
        .to_string();
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ChangeRemotePermissions {
        target_path: target_path.clone(),
        permissions,
        recursive,
        apply_to,
        respond_to: tx,
    })
    .await?;

    let parent = std::path::Path::new(&target_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let _ = refresh_remote_files(&app, &tab_id, &parent).await;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_set_remote_file_access_mode(
    app: AppHandle,
    tab_id: String,
    mode: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let sudo_user = options
        .as_ref()
        .and_then(|o| o.get("sudoUser"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let sudo_password = options
        .as_ref()
        .and_then(|o| o.get("sudoPassword"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let root_access_method = options
        .as_ref()
        .and_then(|o| o.get("rootAccessMethod"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let use_saved_password = options
        .as_ref()
        .and_then(|o| o.get("useSavedPassword"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::SetRemoteFileAccessMode {
        mode,
        root_access_method,
        sudo_user,
        sudo_password,
        use_saved_password,
        respond_to: tx,
    })
    .await?;

    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_queue_upload(
    app: AppHandle,
    file_names: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::queue_upload(&app, file_names).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_upload_file(
    app: AppHandle,
    tab_id: String,
    local_path: String,
    remote_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_upload(
        &app,
        tab_id,
        local_path,
        remote_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_file(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    crate::services::transfers::create_download(
        &app,
        tab_id,
        remote_path,
        local_directory,
        target_name,
    )
    .await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_download_remote_path(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    target_type: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let target_name = options
        .as_ref()
        .and_then(|value| value.get("targetName"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned);
    match target_type.as_str() {
        "file" => app_download_file(app, tab_id, remote_path, local_directory, options).await,
        "folder" => {
            crate::services::transfers::create_download_directory(
                &app,
                tab_id,
                remote_path,
                local_directory,
                target_name,
            )
            .await?;
            get_workspace_snapshot(app).await
        }
        _ => Err(AppError::Command("远端传输目标类型无效".to_string())),
    }
}

#[tauri::command]
pub async fn app_cancel_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_pause_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::pause(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_resume_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::resume(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_discard_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::discard(&app, transfer_id).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_clear_transfers(
    app: AppHandle,
    transfer_ids: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    crate::services::transfers::clear(&app, transfer_ids).await?;
    get_workspace_snapshot(app).await
}

#[tauri::command]
pub async fn app_list_ssh_tunnels(
    app: AppHandle,
    tab_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::ListSshTunnels {
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_create_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule: serde_json::Value,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::CreateSshTunnel {
        rule,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_start_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::StartSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_stop_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::StopSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_delete_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    send_worker_cmd(&app, &tab_id, |tx| WorkerCmd::DeleteSshTunnel {
        rule_id,
        respond_to: tx,
    })
    .await
}

#[tauri::command]
pub async fn app_resolve_ssh_interaction(
    app: AppHandle,
    request_id: String,
    response: serde_json::Value,
) -> Result<(), AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let sender = {
        let mut pending = state.pending_interactions.write().await;
        pending.remove(&request_id)
    };
    if let Some(tx) = sender {
        // Sender error means the receiver was dropped (handshake timed out
        // or the worker exited) — not actionable, ignore.
        let _ = tx.send(response);
    }
    Ok(())
}

#[tauri::command]
pub async fn app_resolve_backup_password(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid backup password request".to_string(),
        ));
    }
    let value = if cancelled {
        None
    } else {
        let value =
            value.ok_or_else(|| AppError::Command("Backup password is required".to_string()))?;
        if value.is_empty()
            || value.len() > 8 * 1024
            || value
                .chars()
                .any(|character| matches!(character, '\0' | '\r' | '\n' | '\u{1b}'))
        {
            return Err(AppError::Command("Backup password is invalid".to_string()));
        }
        Some(value)
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let pending = state
        .pending_backup_passwords
        .write()
        .await
        .remove(request_id);
    if let Some(pending) = pending {
        let _ = pending
            .sender
            .send(crate::services::workspace::BackupPasswordResponse { cancelled, value });
    }
    Ok(())
}

/// Resolve a one-time sudo/su password prompt. The value is accepted only by
/// the main renderer and is forwarded to the waiting exec task through a
/// single-use channel; it never enters terminal input, chat history, or logs.
#[tauri::command]
pub async fn app_resolve_sudo_password_prompt(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
    save: Option<bool>,
) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 || request_id.chars().any(char::is_control) {
        return Err(AppError::Command(
            "Invalid privileged password request".to_string(),
        ));
    }
    let value = if cancelled {
        None
    } else {
        let value = value.ok_or_else(|| {
            AppError::Command("Privileged command password is required".to_string())
        })?;
        if value.is_empty() || value.len() > 4 * 1024 || value.chars().any(char::is_control) {
            return Err(AppError::Command(
                "Privileged command password is invalid".to_string(),
            ));
        }
        Some(value)
    };
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let pending = state
        .pending_sudo_passwords
        .write()
        .await
        .remove(request_id);
    if let Some(pending) = pending {
        let current_revision = state.ai_session_revision(&pending.tab_id).await.to_string();
        let session_is_still_connected = state
            .sessions
            .read()
            .await
            .get(&pending.tab_id)
            .is_some_and(|session| session.connected);
        let target_is_current =
            session_is_still_connected && current_revision == pending.expected_session_revision;
        let _ = pending
            .sender
            .send(crate::services::workspace::SudoPasswordResponse {
                cancelled: cancelled || !target_is_current,
                value: target_is_current.then_some(value).flatten(),
                save: target_is_current && !cancelled && save.unwrap_or(false),
            });
    }
    Ok(())
}

#[tauri::command]
pub async fn app_set_sudo_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    if window.label() != "main" {
        return Err(AppError::Window(
            "Only the FileTerm main window may receive privileged password input".to_string(),
        ));
    }
    let registration_id = registration_id.trim();
    if registration_id.is_empty() || registration_id.len() > 200 {
        return Err(AppError::Command(
            "Invalid privileged password renderer registration".to_string(),
        ));
    }
    app.state::<crate::services::workspace::WorkspaceState>()
        .set_sudo_password_renderer_ready(registration_id, ready)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn app_set_backup_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    if window.label() != "main" {
        return Err(AppError::Window(
            "Only the FileTerm main window may receive backup password input".to_string(),
        ));
    }
    let registration_id = registration_id.trim();
    if registration_id.is_empty() || registration_id.len() > 200 {
        return Err(AppError::Command(
            "Invalid backup password renderer registration".to_string(),
        ));
    }
    app.state::<crate::services::workspace::WorkspaceState>()
        .set_backup_password_renderer_ready(registration_id, ready)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn app_resolve_mcp_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    crate::services::action_review::resolve_action_approval(&app, &request_id, approved).await
}

#[tauri::command]
pub async fn app_resolve_action_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    crate::services::action_review::resolve_action_approval(&app, &request_id, approved).await
}

#[tauri::command]
pub async fn app_resolve_ai_terminal_handoff(
    app: AppHandle,
    request_id: String,
) -> Result<(), AppError> {
    crate::services::action_review::resolve_action_approval_as_terminal(&app, &request_id).await
}

// ==========================================
// Phase 2 commands: profile / folder / command CRUD
// ==========================================
//
// These commands delegate to `services::profile_ops`, which mirrors the
// Electron `FileProfileRepository` semantics (group/parentId self-healing,
// secrets stripping, cascade rename / delete, ordering).

#[tauri::command]
pub async fn app_create_profile(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::create_profile(&app, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_profile(
    app: AppHandle,
    profile_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    let profile = crate::services::profile_ops::update_profile(&app, &profile_id, input)?;
    let resolved_profile = resolve_profile_for_session(&app, &profile)?;
    let reconnect_mode = crate::services::workspace::reconnect_mode_for_profile(&resolved_profile);
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    for session in sessions.values_mut() {
        if session.profile_id == profile_id {
            session.reconnect_mode = reconnect_mode.clone();
        }
    }
    drop(sessions);
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_profile(&app, &profile_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_entity_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_entity_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_folder(&app, &folder_id, updates)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_folder(&app, &folder_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_order(&app, &id, new_parent_id, new_order)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_update_command_template(
    app: AppHandle,
    command_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::update_command_template(&app, &command_id, input)?;
    get_workspace_snapshot_and_emit(&app).await
}

#[tauri::command]
pub async fn app_delete_command_template(
    app: AppHandle,
    command_id: String,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::profile_ops::delete_command_template(&app, &command_id)?;
    get_workspace_snapshot_and_emit(&app).await
}

/// Render and send a command template to an active SSH session.
///
/// This intentionally performs the rendering in the main process: the command
/// source is durable storage, while the renderer only supplies positional
/// arguments and whether the final carriage return is desired. It mirrors the
/// Electron workspace service and keeps arbitrary command text out of the IPC
/// surface.
#[tauri::command]
pub async fn app_execute_command_template(
    app: AppHandle,
    tab_id: String,
    command_id: String,
    args: Option<Vec<String>>,
    options: Option<Value>,
) -> Result<Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let session_type = {
        let tabs = state.tabs.read().await;
        tabs.iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.session_type.clone())
    };
    if session_type.as_deref() != Some("ssh") {
        return Err(AppError::Command("只有 SSH 会话支持快捷命令".to_string()));
    }

    let commands = read_json_array(&app, "commands.json")?;
    let command = commands
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(command_id.as_str()))
        .ok_or_else(|| AppError::Storage(format!("Command not found: {command_id}")))?;
    let template = command
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::Storage(format!("Command is invalid: {command_id}")))?;
    let rendered_command = render_command_template(template, args.as_deref().unwrap_or_default());
    let append_carriage_return = options
        .as_ref()
        .and_then(|value| value.get("appendCarriageReturn"))
        .and_then(Value::as_bool)
        .or_else(|| command.get("appendCarriageReturn").and_then(Value::as_bool))
        .unwrap_or(true);

    let payload = if append_carriage_return {
        format!("{rendered_command}\r")
    } else {
        rendered_command.clone()
    };
    send_terminal_input(&state, &tab_id, payload).await?;

    Ok(serde_json::json!({ "renderedCommand": rendered_command }))
}

fn render_command_template(template: &str, args: &[String]) -> String {
    // `[p#1]` is the durable command-template placeholder format shared with
    // Electron. Invalid/out-of-range references deliberately render as an
    // empty string so existing command libraries retain their behavior.
    let placeholder = Regex::new(r"\[p#(\d+)\]").expect("constant placeholder regex must compile");
    placeholder
        .replace_all(template, |captures: &regex::Captures<'_>| {
            captures
                .get(1)
                .and_then(|index| index.as_str().parse::<usize>().ok())
                .and_then(|index| index.checked_sub(1))
                .and_then(|index| args.get(index))
                .cloned()
                .unwrap_or_default()
        })
        .into_owned()
}

#[cfg(test)]
mod command_template_tests {
    use super::render_command_template;

    #[test]
    fn renders_positional_command_template_arguments() {
        assert_eq!(
            render_command_template(
                "deploy [p#1] --region [p#2] --empty=[p#3]",
                &["api".to_string(), "cn-north".to_string(),]
            ),
            "deploy api --region cn-north --empty="
        );
    }
}

#[cfg(test)]
mod mcp_agent_setup_tests {
    use super::{
        app_get_mcp_agent_setup, append_home_cli_search_paths, resolve_local_cli_from_paths,
    };

    #[test]
    fn resolves_cli_from_ordered_search_paths_without_running_it() {
        let root =
            std::env::temp_dir().join(format!("fileterm-cli-discovery-{}", uuid::Uuid::new_v4()));
        let first_dir = root.join("first");
        let second_dir = root.join("second");
        std::fs::create_dir_all(&first_dir).expect("first search directory should be created");
        std::fs::create_dir_all(&second_dir).expect("second search directory should be created");
        let first_cli = first_dir.join("claude");
        let second_cli = second_dir.join("claude");
        std::fs::write(&first_cli, b"placeholder")
            .expect("first CLI placeholder should be written");
        std::fs::write(&second_cli, b"placeholder")
            .expect("second CLI placeholder should be written");

        let resolved = resolve_local_cli_from_paths("claude", vec![first_dir, second_dir]);

        assert_eq!(resolved, Some(first_cli));
        std::fs::remove_dir_all(root).expect("temporary CLI discovery directory should be removed");
    }

    #[test]
    fn includes_nvm_node_bins_for_desktop_launcher_fallback() {
        let root = std::env::temp_dir().join(format!("fileterm-cli-home-{}", uuid::Uuid::new_v4()));
        let nvm_bin = root.join(".nvm/versions/node/v24.15.0/bin");
        std::fs::create_dir_all(&nvm_bin).expect("nvm bin directory should be created");
        let claude = nvm_bin.join("claude");
        std::fs::write(&claude, b"placeholder").expect("Claude placeholder should be written");

        let mut search_paths = Vec::new();
        append_home_cli_search_paths(&mut search_paths, &root);
        let resolved = resolve_local_cli_from_paths("claude", search_paths);

        assert_eq!(resolved, Some(claude));
        std::fs::remove_dir_all(root).expect("temporary CLI home should be removed");
    }

    #[test]
    fn ignores_codex_bundled_inside_a_macos_desktop_app() {
        let root =
            std::env::temp_dir().join(format!("fileterm-codex-app-{}", uuid::Uuid::new_v4()));
        let app_resources = root.join("ChatGPT.app/Contents/Resources");
        std::fs::create_dir_all(&app_resources)
            .expect("desktop app Resources directory should be created");
        let bundled_codex = app_resources.join("codex");
        std::fs::write(&bundled_codex, b"desktop helper")
            .expect("bundled Codex placeholder should be written");

        let resolved = resolve_local_cli_from_paths("codex", vec![app_resources]);

        assert_eq!(resolved, None);
        std::fs::remove_dir_all(root).expect("temporary desktop app directory should be removed");
    }

    #[test]
    fn still_resolves_user_codex_cli_outside_a_desktop_app() {
        let root =
            std::env::temp_dir().join(format!("fileterm-codex-cli-{}", uuid::Uuid::new_v4()));
        let cli_dir = root.join(".local/bin");
        std::fs::create_dir_all(&cli_dir).expect("user CLI directory should be created");
        let codex = cli_dir.join("codex");
        std::fs::write(&codex, b"user CLI").expect("user Codex placeholder should be written");

        let resolved = resolve_local_cli_from_paths("codex", vec![cli_dir]);

        assert_eq!(resolved, Some(codex));
        std::fs::remove_dir_all(root).expect("temporary user CLI directory should be removed");
    }

    #[test]
    fn generates_stdio_registration_commands_for_supported_clients() {
        let setup = app_get_mcp_agent_setup().expect("MCP Agent setup should be readable");
        assert!(!setup.fileterm_command.is_empty());
        assert!(
            setup.fileterm_command.starts_with('\'') || setup.fileterm_command.starts_with('"')
        );

        let claude = setup
            .clients
            .iter()
            .find(|client| client.id == "claude-code")
            .expect("Claude Code client should be exposed");
        assert!(claude
            .registration_command
            .starts_with("claude mcp add --scope user fileterm -- "));
        assert!(claude.registration_command.ends_with(" mcp"));

        let codex = setup
            .clients
            .iter()
            .find(|client| client.id == "codex-cli")
            .expect("Codex CLI client should be exposed");
        assert!(codex
            .registration_command
            .starts_with("codex mcp add fileterm -- "));
        assert!(codex.registration_command.ends_with(" mcp"));
    }
}

#[cfg(test)]
mod split_pane_close_tests {
    use super::{attach_split_pane_to_tabs, remove_split_pane_from_tabs, supports_split_panes};
    use crate::services::{PaneNode, SplitDirection, WorkspaceTab, WorkspaceTabStatus};

    fn tab(id: &str, pane_root: Option<PaneNode>, pane_root_tab_id: Option<&str>) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status: WorkspaceTabStatus::Connected,
            pane_root,
            pane_root_tab_id: pane_root_tab_id.map(str::to_string),
        }
    }

    fn local_tab(
        id: &str,
        pane_root: Option<PaneNode>,
        pane_root_tab_id: Option<&str>,
    ) -> WorkspaceTab {
        WorkspaceTab {
            id: id.to_string(),
            profile_id: "__local_terminal__".to_string(),
            session_type: "local".to_string(),
            title: "Local Terminal".to_string(),
            layout: "terminal-only".to_string(),
            status: WorkspaceTabStatus::Connected,
            pane_root,
            pane_root_tab_id: pane_root_tab_id.map(str::to_string),
        }
    }

    #[test]
    fn closing_a_child_pane_keeps_the_existing_root_tab() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "child".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("child", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "child").unwrap();

        assert_eq!(outcome.root_tab_id, "root");
        assert!(!outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["root"]);
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "root");
        assert!(tabs[0].pane_root.is_none());
    }

    #[test]
    fn only_ssh_and_local_sessions_can_be_split() {
        assert!(supports_split_panes("ssh"));
        assert!(supports_split_panes("local"));
        assert!(!supports_split_panes("ftp"));
        assert!(!supports_split_panes("telnet"));
        assert!(!supports_split_panes("serial"));
    }

    #[test]
    fn closing_a_local_child_pane_preserves_the_local_root() {
        let mut tabs = vec![
            local_tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Column,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "child".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            local_tab("child", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "child").unwrap();

        assert_eq!(outcome.root_tab_id, "root");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].session_type, "local");
        assert!(tabs[0].pane_root.is_none());
    }

    #[test]
    fn closing_the_original_root_leaf_promotes_a_surviving_pane() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Split {
                            direction: SplitDirection::Column,
                            children: vec![
                                PaneNode::Leaf {
                                    tab_id: "second".to_string(),
                                },
                                PaneNode::Leaf {
                                    tab_id: "third".to_string(),
                                },
                            ],
                            weights: vec![0.5, 0.5],
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("second", None, Some("root")),
            tab("third", None, Some("root")),
        ];

        let outcome = remove_split_pane_from_tabs(&mut tabs, "root", "root").unwrap();

        assert_eq!(outcome.root_tab_id, "second");
        assert!(outcome.keeps_split);
        assert_eq!(outcome.remaining_pane_tab_ids, vec!["second", "third"]);
        assert_eq!(tabs.len(), 2);

        let promoted_root = tabs.iter().find(|tab| tab.id == "second").unwrap();
        assert!(promoted_root.pane_root.is_some());
        assert!(promoted_root.pane_root_tab_id.is_none());
        assert_eq!(
            tabs.iter()
                .find(|tab| tab.id == "third")
                .and_then(|tab| tab.pane_root_tab_id.as_deref()),
            Some("second")
        );
    }

    #[test]
    fn attaching_a_pane_to_an_independent_tab_creates_a_root_tree() {
        let mut tabs = vec![tab("root", None, None), tab("child", None, Some("root"))];

        let root_id =
            attach_split_pane_to_tabs(&mut tabs, "root", "child", SplitDirection::Row).unwrap();

        assert_eq!(root_id, "root");
        let root = tabs.iter().find(|tab| tab.id == "root").unwrap();
        assert_eq!(
            root.pane_root.as_ref().unwrap().leaf_tab_ids(),
            vec!["root", "child"]
        );
    }

    #[test]
    fn attaching_a_pane_to_an_existing_leaf_preserves_the_root_id() {
        let mut tabs = vec![
            tab(
                "root",
                Some(PaneNode::Split {
                    direction: SplitDirection::Row,
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "root".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "other".to_string(),
                        },
                    ],
                    weights: vec![0.5, 0.5],
                }),
                None,
            ),
            tab("other", None, Some("root")),
            tab("child", None, Some("root")),
        ];

        let root_id =
            attach_split_pane_to_tabs(&mut tabs, "other", "child", SplitDirection::Column).unwrap();

        assert_eq!(root_id, "root");
        assert_eq!(
            tabs[0].pane_root.as_ref().unwrap().leaf_tab_ids(),
            vec!["root", "other", "child"]
        );
    }

    #[test]
    fn attaching_a_pane_fails_without_mutating_tabs_when_source_vanished() {
        let mut tabs = vec![tab("root", None, None)];
        let before = tabs.clone();

        let result = attach_split_pane_to_tabs(&mut tabs, "missing", "child", SplitDirection::Row);

        assert!(result.is_err());
        assert_eq!(
            serde_json::to_value(&tabs).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
    }
}

#[cfg(test)]
mod reconnect_tests {
    use super::claim_reconnect_tab;
    use crate::services::{WorkspaceTab, WorkspaceTabStatus};

    fn tab(status: WorkspaceTabStatus) -> WorkspaceTab {
        WorkspaceTab {
            id: "tab-1".to_string(),
            profile_id: "profile-1".to_string(),
            session_type: "ssh".to_string(),
            title: "Server".to_string(),
            layout: "terminal-file".to_string(),
            status,
            pane_root: None,
            pane_root_tab_id: None,
        }
    }

    #[test]
    fn reconnect_can_only_be_claimed_once_while_connecting() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(claim_reconnect_tab(&mut tabs, "tab-1"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Connecting);
        assert!(!claim_reconnect_tab(&mut tabs, "tab-1"));
    }

    #[test]
    fn reconnect_does_not_claim_an_unknown_tab() {
        let mut tabs = vec![tab(WorkspaceTabStatus::Closed)];

        assert!(!claim_reconnect_tab(&mut tabs, "missing"));
        assert_eq!(tabs[0].status, WorkspaceTabStatus::Closed);
    }
}

#[cfg(test)]
mod architecture_tests {
    use super::resolve_native_arch;

    #[test]
    fn reports_apple_silicon_when_x64_process_runs_under_rosetta() {
        assert_eq!(resolve_native_arch("macos", "x86_64", true), "arm64");
    }

    #[test]
    fn canonicalizes_native_rust_architecture_names() {
        assert_eq!(resolve_native_arch("macos", "aarch64", true), "arm64");
        assert_eq!(resolve_native_arch("macos", "x86_64", false), "x64");
        assert_eq!(resolve_native_arch("linux", "x86_64", false), "x64");
    }
}

#[cfg(test)]
mod window_lifecycle_tests {
    use super::renderer_approved_close_should_destroy;

    #[test]
    fn main_window_close_keeps_the_lifecycle_guard() {
        assert!(!renderer_approved_close_should_destroy("main"));
        assert!(renderer_approved_close_should_destroy(
            "file-editor-local-1"
        ));
        assert!(renderer_approved_close_should_destroy("connection-manager"));
    }
}

#[cfg(test)]
mod ui_state_tests {
    use super::normalize_ui_state;

    #[test]
    fn reads_current_object_ui_state() {
        let states = normalize_ui_state(serde_json::json!({ "main.tab-ui": "tabs" })).unwrap();
        assert_eq!(
            states.get("main.tab-ui").and_then(|value| value.as_str()),
            Some("tabs")
        );
    }

    #[test]
    fn migrates_electron_and_legacy_array_ui_state() {
        let electron = normalize_ui_state(serde_json::json!({
            "version": 1,
            "values": { "ssh-key-manager-ui": "folders" }
        }))
        .unwrap();
        assert_eq!(
            electron
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("folders")
        );

        let legacy = normalize_ui_state(serde_json::json!([
            { "key": "ssh-key-manager-ui", "value": "legacy-folders" }
        ]))
        .unwrap();
        assert_eq!(
            legacy
                .get("ssh-key-manager-ui")
                .and_then(|value| value.as_str()),
            Some("legacy-folders")
        );
    }
}

#[cfg(test)]
mod ui_preferences_tests {
    use std::collections::BTreeMap;

    use super::{
        default_overview_section_order, default_resource_monitoring_metric_order,
        default_resource_monitoring_metrics, default_theme_config, default_update_channel,
        normalize_resource_monitoring_metric_order, normalize_theme_config,
        normalize_ui_preferences, resolve_profile_with_connection_defaults, McpAgentPreferences,
        SavedTheme, SshConnectionDefaults, UiPreferences, UiPreferencesInput,
    };

    #[test]
    fn normalizes_theme_config_colors_fonts_and_variant() {
        let mut config = default_theme_config();
        config.variant = "light".to_string();
        config.theme.contrast = 255;
        config.theme.accent = "not-a-color".to_string();
        config.theme.surface_secondary = "not-a-color".to_string();
        config.theme.semantic_colors.text_secondary = "not-a-color".to_string();
        config.theme.terminal.ansi.red = "#abc".to_string();
        config
            .theme
            .overrides
            .insert("--bg-main".to_string(), "#abc".to_string());
        config.theme.fonts.ui = Some("Inter".to_string());
        config.theme.fonts.code = Some("font-family: unsafe".to_string());

        let normalized = normalize_theme_config(config, "light");

        assert_eq!(normalized.variant, "light");
        assert_eq!(normalized.theme.contrast, 100);
        assert_eq!(normalized.theme.accent, "#3B82F6");
        assert_eq!(normalized.theme.surface_secondary, "#FFFFFF");
        assert_eq!(normalized.theme.semantic_colors.text_secondary, "#5E5E61");
        assert_eq!(normalized.theme.terminal.ansi.red, "#ABC");
        assert_eq!(
            normalized.theme.overrides.get("--bg-main"),
            Some(&"#ABC".to_string())
        );
        assert_eq!(normalized.theme.fonts.ui.as_deref(), Some("Inter"));
        assert_eq!(normalized.theme.fonts.code, None);
    }

    #[test]
    fn default_theme_config_keeps_the_compact_contract_and_terminal_selection_alpha() {
        let dark = default_theme_config();
        let light = super::default_theme_config_for_variant("light");
        let serialized = serde_json::to_value(&dark).expect("default theme should serialize");

        assert!(serialized["theme"].get("overrides").is_none());
        assert_eq!(dark.theme.terminal.selection_background, "#388BFD85");
        assert_eq!(light.theme.terminal.selection_background, "#0969DA42");
    }

    #[test]
    fn legacy_component_color_table_does_not_override_the_default_css_theme() {
        let mut legacy =
            serde_json::to_value(default_theme_config()).expect("default theme should serialize");
        for key in ["surfaceSecondary", "surfaceElevated"] {
            legacy["theme"]
                .as_object_mut()
                .expect("theme should be an object")
                .remove(key);
        }
        for key in [
            "secondary",
            "textSecondary",
            "info",
            "warning",
            "error",
            "success",
        ] {
            legacy["theme"]["semanticColors"]
                .as_object_mut()
                .expect("semantic colors should be an object")
                .remove(key);
        }
        legacy["theme"]["ui"] = serde_json::json!({
            "surfaces": { "app": "#FF00FF" },
            "dialog": { "surface": "#00FF00" }
        });

        let config: super::ThemeConfig =
            serde_json::from_value(legacy).expect("legacy theme should still deserialize");
        let normalized = normalize_theme_config(config, "dark");

        assert!(normalized.theme.overrides.is_empty());
        assert_eq!(normalized.code_theme_id, "fileterm");
        assert_eq!(normalized.theme.surface_secondary, "#1E1E1E");
        assert_eq!(normalized.theme.surface_elevated, "#2A2A2A");
        assert_eq!(normalized.theme.semantic_colors.secondary, "#8BBFFF");
        assert_eq!(normalized.theme.semantic_colors.success, "#39D98A");
    }

    #[test]
    fn canonicalizes_legacy_fileterm_variant_id() {
        let mut config = default_theme_config();
        config.code_theme_id = "fileterm-dark".to_string();

        let normalized = normalize_theme_config(config, "dark");

        assert_eq!(normalized.code_theme_id, "fileterm");
    }

    #[test]
    fn normalizes_saved_theme_identity_and_inherited_base() {
        let mut custom = default_theme_config();
        custom.code_theme_id = "custom".to_string();
        custom.base_theme_id = Some("codex".to_string());
        custom.theme.accent = "not-a-color".to_string();

        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            custom_themes: vec![
                SavedTheme {
                    id: "  custom-one  ".to_string(),
                    name: "  My Codex Tweak  ".to_string(),
                    config: custom,
                    variants: BTreeMap::new(),
                },
                SavedTheme {
                    id: "custom-one".to_string(),
                    name: "Duplicate".to_string(),
                    config: default_theme_config(),
                    variants: BTreeMap::new(),
                },
            ],
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(preferences.custom_themes.len(), 1);
        assert_eq!(preferences.custom_themes[0].id, "custom-one");
        assert_eq!(preferences.custom_themes[0].name, "My Codex Tweak");
        assert_eq!(
            preferences.custom_themes[0].config.base_theme_id.as_deref(),
            Some("codex")
        );
        assert_eq!(preferences.custom_themes[0].config.theme.accent, "#0169CC");
    }

    #[test]
    fn falls_back_to_safe_values_for_unknown_preferences() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "unknown-theme".to_string(),
            locale: "unknown-locale".to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "nightly".to_string(),
            terminal_zoom_locked: false,
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: vec![
                "unknown".to_string(),
                "stats".to_string(),
                "stats".to_string(),
            ],
        });

        assert_eq!(preferences.theme, "default-dark");
        assert_eq!(preferences.locale, "zhCN");
        assert_eq!(preferences.update_channel, "stable");
        assert!(preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert_eq!(
            preferences.overview_section_order,
            default_overview_section_order()
        );
    }

    #[test]
    fn resource_monitoring_defaults_keep_gpu_metrics_opt_in_and_order_complete() {
        let enabled = default_resource_monitoring_metrics();
        let order = default_resource_monitoring_metric_order();

        assert!(!enabled.iter().any(|metric| metric == "gpu"));
        assert!(!enabled.iter().any(|metric| metric == "gpuMemory"));
        assert!(!enabled.iter().any(|metric| metric == "gpuTemperature"));
        assert!(!enabled.iter().any(|metric| metric == "gpuPower"));
        assert_eq!(order.len(), 11);
        let expected_order: Vec<String> = [
            "load",
            "cpu",
            "memory",
            "swap",
            "disk",
            "gpu",
            "gpuMemory",
            "gpuTemperature",
            "gpuPower",
            "processes",
            "network",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(order, expected_order);
    }

    #[test]
    fn resource_monitoring_metric_order_deduplicates_and_appends_missing_items() {
        let normalized = normalize_resource_monitoring_metric_order(vec![
            "network".to_string(),
            "network".to_string(),
            "not-a-metric".to_string(),
            "cpu".to_string(),
        ]);

        assert_eq!(normalized[0], "network");
        assert_eq!(normalized[1], "cpu");
        assert_eq!(normalized.len(), 11);
        assert_eq!(
            normalized
                .iter()
                .filter(|metric| metric.as_str() == "network")
                .count(),
            1
        );
    }

    #[test]
    fn keeps_supported_preferences_unchanged() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-light".to_string(),
            locale: "enUS".to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "beta".to_string(),
            terminal_zoom_locked: true,
            file_panel_remember_ratio: false,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: false,
            overview_show_recent: false,
            overview_show_all_connections: true,
            overview_show_quick_actions: false,
            overview_section_order: vec![
                "allConnections".to_string(),
                "stats".to_string(),
                "recent".to_string(),
                "quickActions".to_string(),
            ],
        });

        assert_eq!(preferences.theme, "default-light");
        assert_eq!(preferences.locale, "enUS");
        assert_eq!(preferences.update_channel, "beta");
        assert!(!preferences.auto_check_updates);
        assert!(preferences.terminal_zoom_locked);
        assert!(!preferences.overview_show_stats);
        assert!(!preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert!(!preferences.overview_show_quick_actions);
        assert_eq!(
            preferences.overview_section_order,
            vec![
                "allConnections".to_string(),
                "stats".to_string(),
                "recent".to_string(),
                "quickActions".to_string()
            ]
        );
    }

    #[test]
    fn normalizes_invalid_mcp_agent_preferences_fail_closed() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences {
                connection_scope: "not-a-scope".to_string(),
                operation_policy: "not-a-policy".to_string(),
                default_profile_id: Some("  ".to_string()),
            },
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(
            preferences.mcp_agent.connection_scope,
            "all-saved-connections"
        );
        assert_eq!(
            preferences.mcp_agent.operation_policy,
            "approved-operations"
        );
        assert_eq!(preferences.mcp_agent.default_profile_id, None);
    }

    #[test]
    fn default_connection_scope_without_profile_falls_back_to_active_session() {
        let preferences = normalize_ui_preferences(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: true,
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            file_panel_remember_ratio: true,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences {
                connection_scope: "default-connection".to_string(),
                operation_policy: "read-only".to_string(),
                default_profile_id: None,
            },
            overview_show_stats: true,
            overview_show_recent: true,
            overview_show_all_connections: true,
            overview_show_quick_actions: true,
            overview_section_order: default_overview_section_order(),
        });

        assert_eq!(preferences.mcp_agent.connection_scope, "active-session");
        assert_eq!(preferences.mcp_agent.operation_policy, "read-only");
    }

    #[test]
    fn preserves_saved_connection_values_and_explicit_overrides() {
        let defaults = SshConnectionDefaults {
            use_empty_password: true,
            enable_exec_channel: false,
            enable_resource_monitoring: false,
            resource_monitoring_interval_seconds: 15,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            reconnect_mode: "enter".to_string(),
            legacy_algorithms: false,
        };
        let profile = serde_json::json!({
            "type": "ssh",
            "enableExecChannel": true,
            "enableResourceMonitoring": true,
            "resourceMonitoringIntervalSeconds": 5,
            "reconnectMode": "none",
            "legacyAlgorithms": false,
            "connectionOverrides": {
                "reconnectMode": "auto",
                "legacyAlgorithms": true
            }
        });

        let resolved = resolve_profile_with_connection_defaults(&profile, &defaults);

        assert_eq!(resolved["useEmptyPassword"], true);
        assert_eq!(resolved["enableExecChannel"], true);
        assert_eq!(resolved["enableResourceMonitoring"], true);
        assert_eq!(resolved["resourceMonitoringIntervalSeconds"], 5);
        assert_eq!(resolved["reconnectMode"], "auto");
        assert_eq!(resolved["legacyAlgorithms"], true);
        assert_eq!(profile["enableExecChannel"], true);
    }

    #[test]
    fn preserves_legacy_profile_values_without_override_metadata() {
        let defaults = SshConnectionDefaults::default();
        let profile = serde_json::json!({
            "type": "ssh",
            "enableExecChannel": false,
            "reconnectMode": "auto"
        });

        let resolved = resolve_profile_with_connection_defaults(&profile, &defaults);

        assert_eq!(resolved["enableExecChannel"], false);
        assert_eq!(resolved["reconnectMode"], "auto");
        assert_eq!(resolved["enableResourceMonitoring"], true);
        assert_eq!(resolved["resourceMonitoringIntervalSeconds"], 1);
    }

    #[test]
    fn defaults_auto_update_checks_for_existing_preferences() {
        let preferences: UiPreferences = serde_json::from_value(serde_json::json!({
            "theme": "default-dark",
            "locale": "zhCN"
        }))
        .expect("legacy UI preferences should still deserialize");

        assert!(preferences.auto_check_updates);
        assert_eq!(preferences.update_channel, "stable");
        assert!(preferences.overview_show_stats);
        assert!(preferences.overview_show_recent);
        assert!(preferences.overview_show_all_connections);
        assert!(preferences.overview_show_quick_actions);
        assert_eq!(preferences.theme_config.schema_version, "codex-theme-v1");
        assert_eq!(
            preferences.overview_section_order,
            default_overview_section_order()
        );
    }

    #[test]
    fn uses_camel_case_for_the_update_check_preference_contract() {
        let input: UiPreferencesInput = serde_json::from_value(serde_json::json!({
            "autoCheckUpdates": false,
            "updateChannel": "beta",
            "overviewShowStats": false,
            "overviewShowRecent": false,
            "overviewShowAllConnections": true,
            "overviewShowQuickActions": false,
            "overviewSectionOrder": ["recent", "allConnections", "stats", "quickActions"]
        }))
        .expect("renderer preference input should deserialize");
        assert_eq!(input.auto_check_updates, Some(false));
        assert_eq!(input.update_channel.as_deref(), Some("beta"));
        assert_eq!(input.overview_show_stats, Some(false));
        assert_eq!(input.overview_show_recent, Some(false));
        assert_eq!(input.overview_show_all_connections, Some(true));
        assert_eq!(input.overview_show_quick_actions, Some(false));
        assert_eq!(
            input.overview_section_order,
            Some(vec![
                "recent".to_string(),
                "allConnections".to_string(),
                "stats".to_string(),
                "quickActions".to_string()
            ])
        );

        let preferences = serde_json::to_value(UiPreferences {
            theme: "default-dark".to_string(),
            locale: "zhCN".to_string(),
            theme_config: default_theme_config(),
            custom_themes: Vec::new(),
            auto_check_updates: false,
            update_channel: "beta".to_string(),
            terminal_zoom_locked: true,
            file_panel_remember_ratio: false,
            resource_monitoring_metrics: default_resource_monitoring_metrics(),
            resource_monitoring_metric_order: default_resource_monitoring_metric_order(),
            connection_defaults: SshConnectionDefaults::default(),
            mcp_agent: McpAgentPreferences::default(),
            overview_show_stats: false,
            overview_show_recent: false,
            overview_show_all_connections: true,
            overview_show_quick_actions: false,
            overview_section_order: vec![
                "recent".to_string(),
                "allConnections".to_string(),
                "stats".to_string(),
                "quickActions".to_string(),
            ],
        })
        .expect("preferences should serialize");
        assert_eq!(preferences["autoCheckUpdates"], false);
        assert_eq!(preferences["updateChannel"], "beta");
        assert_eq!(preferences["overviewShowStats"], false);
        assert_eq!(preferences["overviewShowRecent"], false);
        assert_eq!(preferences["overviewShowAllConnections"], true);
        assert_eq!(preferences["overviewShowQuickActions"], false);
        assert_eq!(
            preferences["themeConfig"]["schemaVersion"],
            "codex-theme-v1"
        );
        assert!(preferences["themeConfig"]["theme"]["terminal"]["ansi"]["brightBlack"].is_string());
        assert_eq!(
            preferences["overviewSectionOrder"],
            serde_json::json!(["recent", "allConnections", "stats", "quickActions"])
        );
    }
}

#[cfg(test)]
mod permission_contract_tests {
    use super::{
        parse_remote_permission_mode, PermissionApplyTarget, RemotePermissionChangeOptions,
    };

    #[test]
    fn reads_shared_camel_case_permission_contract() {
        let options: RemotePermissionChangeOptions = serde_json::from_value(serde_json::json!({
            "mode": "0640",
            "recursive": true,
            "applyTo": "files"
        }))
        .expect("shared permission options should deserialize");

        assert_eq!(parse_remote_permission_mode(&options.mode).unwrap(), 0o640);
        assert!(options.recursive);
        assert!(matches!(
            options.apply_to,
            Some(PermissionApplyTarget::Files)
        ));
    }

    #[test]
    fn rejects_legacy_permissions_field_instead_of_defaulting_to_0755() {
        let options = serde_json::from_value::<RemotePermissionChangeOptions>(serde_json::json!({
            "permissions": 384,
            "recursive": false
        }));
        assert!(options.is_err());
    }

    #[test]
    fn validates_octal_permission_modes() {
        assert_eq!(parse_remote_permission_mode("600").unwrap(), 0o600);
        assert_eq!(parse_remote_permission_mode("755").unwrap(), 0o755);
        assert!(parse_remote_permission_mode("888").is_err());
        assert!(parse_remote_permission_mode("75").is_err());
    }
}

#[cfg(test)]
mod serial_port_contract_tests {
    use crate::services::serial_ports::map_serial_port_info;

    #[test]
    fn maps_usb_metadata_without_accessing_hardware() {
        let item = map_serial_port_info(tokio_serial::SerialPortInfo {
            port_name: "/dev/cu.test".to_string(),
            port_type: tokio_serial::SerialPortType::UsbPort(tokio_serial::UsbPortInfo {
                vid: 0x1234,
                pid: 0xabcd,
                serial_number: Some("SN-1".to_string()),
                manufacturer: Some("Test Vendor".to_string()),
                product: Some("Test Adapter".to_string()),
            }),
        });

        assert_eq!(item.port_name, "/dev/cu.test");
        assert_eq!(item.port_type, "usb");
        assert_eq!(item.vendor_id, Some(0x1234));
        assert_eq!(item.product_id, Some(0xabcd));
        assert_eq!(item.manufacturer.as_deref(), Some("Test Vendor"));
        assert_eq!(item.product.as_deref(), Some("Test Adapter"));
        assert_eq!(item.serial_number.as_deref(), Some("SN-1"));

        let serialized = serde_json::to_value(item).expect("serial port item should serialize");
        assert_eq!(serialized["portName"], "/dev/cu.test");
        assert_eq!(serialized["vendorId"], 0x1234);
        assert_eq!(serialized["productId"], 0xabcd);
    }
}

#[cfg(test)]
mod external_url_tests {
    use super::validate_external_url;

    #[test]
    fn external_url_policy_accepts_only_web_links() {
        for allowed in [
            "https://github.com/St0ff3l/fileterm",
            "http://127.0.0.1/docs",
        ] {
            assert!(validate_external_url(allowed).is_ok());
        }
        for denied in [
            "file:///etc/passwd",
            "ssh://example.com",
            "javascript:alert(1)",
        ] {
            assert!(validate_external_url(denied).is_err());
        }
        assert!(validate_external_url("not a url").is_err());
    }
}
