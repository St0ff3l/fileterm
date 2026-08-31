// Preference types, defaults, normalization, and profile resolution.
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
pub struct LocalTerminalShellPreferences {
    #[serde(default = "default_windows_local_terminal_shell")]
    pub win32: String,
    #[serde(default = "default_macos_local_terminal_shell")]
    pub darwin: String,
    #[serde(default = "default_linux_local_terminal_shell")]
    pub linux: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalShellPreferencesInput {
    pub win32: Option<String>,
    pub darwin: Option<String>,
    pub linux: Option<String>,
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
    #[serde(default = "default_local_terminal_shells")]
    pub local_terminal_shells: LocalTerminalShellPreferences,
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
    pub local_terminal_shells: Option<LocalTerminalShellPreferencesInput>,
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

fn default_windows_local_terminal_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        crate::sessions::local_terminal::default_launch().shell
    }

    #[cfg(not(target_os = "windows"))]
    {
        "pwsh.exe".to_string()
    }
}

fn default_macos_local_terminal_shell() -> String {
    #[cfg(target_os = "macos")]
    {
        crate::sessions::local_terminal::default_launch().shell
    }

    #[cfg(not(target_os = "macos"))]
    {
        "/bin/zsh".to_string()
    }
}

fn default_linux_local_terminal_shell() -> String {
    #[cfg(target_os = "linux")]
    {
        crate::sessions::local_terminal::default_launch().shell
    }

    #[cfg(not(target_os = "linux"))]
    {
        "/bin/bash".to_string()
    }
}

fn default_local_terminal_shells() -> LocalTerminalShellPreferences {
    LocalTerminalShellPreferences {
        win32: default_windows_local_terminal_shell(),
        darwin: default_macos_local_terminal_shell(),
        linux: default_linux_local_terminal_shell(),
    }
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
    "selected-connections".to_string()
}

fn default_mcp_operation_policy() -> String {
    "basic-safe-operations".to_string()
}

fn normalize_mcp_operation_policy(operation_policy: &str) -> String {
    match operation_policy {
        "read-only" => "read-only".to_string(),
        // Keep the previous persisted value readable, but write the clearer
        // policy name back whenever preferences are saved.
        "approved-operations" | "basic-safe-operations" => "basic-safe-operations".to_string(),
        "full-access" => "full-access".to_string(),
        _ => default_mcp_operation_policy(),
    }
}

fn normalize_mcp_allowed_profile_ids(profile_ids: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for profile_id in profile_ids {
        let profile_id = profile_id.trim();
        if profile_id.is_empty() || profile_id.len() > 256 {
            continue;
        }
        if !normalized.iter().any(|existing| existing == profile_id) {
            normalized.push(profile_id.to_string());
        }
        if normalized.len() >= 256 {
            break;
        }
    }
    normalized
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

fn normalize_local_terminal_shell(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_local_terminal_shells(
    mut shells: LocalTerminalShellPreferences,
) -> LocalTerminalShellPreferences {
    let defaults = default_local_terminal_shells();
    shells.win32 = normalize_local_terminal_shell(shells.win32, &defaults.win32);
    shells.darwin = normalize_local_terminal_shell(shells.darwin, &defaults.darwin);
    shells.linux = normalize_local_terminal_shell(shells.linux, &defaults.linux);
    shells
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
    // DBX-style MCP policy has only two connection modes: all saved
    // connections or an explicit allowlist. Migrate the two older runtime
    // target modes when loading preferences. An active-session policy has no
    // stable saved profile to recover, so it fails closed with an empty
    // allowlist; a default-connection policy can be preserved exactly.
    let legacy_default_profile_id = preferences
        .mcp_agent
        .legacy_default_profile_id
        .take()
        .and_then(|profile_id| {
            let trimmed = profile_id.trim();
            (!trimmed.is_empty() && trimmed.len() <= 256).then(|| trimmed.to_string())
        });
    match preferences.mcp_agent.connection_scope.as_str() {
        "all-saved-connections" | "selected-connections" => {}
        "active-session" => {
            preferences.mcp_agent.connection_scope = "selected-connections".to_string();
            preferences.mcp_agent.allowed_profile_ids.clear();
        }
        "default-connection" => {
            preferences.mcp_agent.connection_scope = "selected-connections".to_string();
            if let Some(profile_id) = legacy_default_profile_id {
                preferences.mcp_agent.allowed_profile_ids.push(profile_id);
            }
        }
        _ => {
            preferences.mcp_agent.connection_scope = default_mcp_connection_scope();
        }
    }
    preferences.mcp_agent.operation_policy =
        normalize_mcp_operation_policy(&preferences.mcp_agent.operation_policy);
    preferences.mcp_agent.allowed_profile_ids =
        normalize_mcp_allowed_profile_ids(preferences.mcp_agent.allowed_profile_ids);
    preferences.overview_section_order =
        normalize_overview_section_order(preferences.overview_section_order);
    preferences.local_terminal_shells =
        normalize_local_terminal_shells(preferences.local_terminal_shells);
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
