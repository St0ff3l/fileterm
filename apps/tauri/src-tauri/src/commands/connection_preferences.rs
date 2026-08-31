// Connection defaults, MCP preferences, and theme types.
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

/// Non-secret boundary shared by MCP clients, the FileTerm CLI and external
/// MCP and CLI bridges. It deliberately does not contain connection credentials or
/// any executable configuration path.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentPreferences {
    #[serde(default = "default_mcp_connection_scope")]
    pub connection_scope: String,
    #[serde(default = "default_mcp_operation_policy")]
    pub operation_policy: String,
    #[serde(default)]
    pub allowed_profile_ids: Vec<String>,
    /// Read old persisted `defaultProfileId` values only long enough to
    /// migrate legacy MCP scope settings. It is never returned to clients or
    /// written back to disk.
    #[serde(rename = "defaultProfileId", default, skip_serializing)]
    pub legacy_default_profile_id: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct McpAgentPreferencesInput {
    pub connection_scope: Option<String>,
    pub operation_policy: Option<String>,
    pub allowed_profile_ids: Option<Vec<String>>,
}

impl Default for McpAgentPreferences {
    fn default() -> Self {
        Self {
            connection_scope: default_mcp_connection_scope(),
            operation_policy: default_mcp_operation_policy(),
            allowed_profile_ids: Vec::new(),
            legacy_default_profile_id: None,
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
                diff_added: if is_light { "#168a53" } else { "#34d399" }.to_string(),
                diff_removed: if is_light { "#d94e4e" } else { "#ff5f57" }.to_string(),
                skill: if is_light { "#7c3aed" } else { "#b06dff" }.to_string(),
                keyword: if is_light { "#b45309" } else { "#fbbf24" }.to_string(),
                sftp: if is_light { "#0284c7" } else { "#38bdf8" }.to_string(),
                ftp: if is_light { "#9333ea" } else { "#c084fc" }.to_string(),
                secondary: if is_light { "#3b82f6" } else { "#8bbfff" }.to_string(),
                text_secondary: if is_light { "#5e5e61" } else { "#9b9b9b" }.to_string(),
                info: if is_light { "#3b82f6" } else { "#38bdf8" }.to_string(),
                warning: if is_light { "#d97706" } else { "#ffcc00" }.to_string(),
                error: if is_light { "#d94e4e" } else { "#ff5f57" }.to_string(),
                success: if is_light { "#168a53" } else { "#34d399" }.to_string(),
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
