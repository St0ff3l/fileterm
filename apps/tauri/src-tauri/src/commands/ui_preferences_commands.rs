// UI preferences, security, and terminal zoom commands.
#[tauri::command]
pub fn app_get_ui_preferences(app: AppHandle) -> Result<UiPreferences, AppError> {
    let path = crate::storage::state_path(&app)?;
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let preferences: UiPreferences = serde_json::from_str(&content)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        let mut preferences = normalize_ui_preferences(preferences);
        let persist_reset = reset_active_theme_for_app_version(
            &mut preferences,
            &app.package_info().version.to_string(),
        );
        if persist_reset {
            let content = serde_json::to_string_pretty(&preferences)
                .map_err(|error| AppError::Serialization(error.to_string()))?;
            if let Err(error) = std::fs::write(&path, content) {
                crate::services::logging::warn(
                    &app,
                    "ui-preferences",
                    format!("unable to persist default theme reset: {error}"),
                );
            }
        }
        Ok(preferences)
    } else {
        Ok(UiPreferences {
            theme: DEFAULT_UI_THEME.to_string(),
            locale: DEFAULT_UI_LOCALE.to_string(),
            theme_config: default_theme_config(),
            fileterm_theme_reset_app_version: Some(app.package_info().version.to_string()),
            custom_themes: Vec::new(),
            auto_check_updates: default_auto_check_updates(),
            update_channel: default_update_channel(),
            terminal_zoom_locked: false,
            local_terminal_shells: default_local_terminal_shells(),
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
pub fn app_list_local_terminal_shells(
) -> Vec<crate::sessions::local_terminal::LocalTerminalShellOption> {
    crate::sessions::local_terminal::available_shells()
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
    if let Some(local_terminal_shells) = input.local_terminal_shells {
        if let Some(value) = local_terminal_shells.win32 {
            preferences.local_terminal_shells.win32 = value;
        }
        if let Some(value) = local_terminal_shells.darwin {
            preferences.local_terminal_shells.darwin = value;
        }
        if let Some(value) = local_terminal_shells.linux {
            preferences.local_terminal_shells.linux = value;
        }
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
        if let Some(allowed_profile_ids) = mcp_agent.allowed_profile_ids {
            preferences.mcp_agent.allowed_profile_ids = allowed_profile_ids;
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

#[tauri::command]
pub fn app_get_security_settings(
    app: AppHandle,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    crate::services::security::get_settings(&app)
}

#[tauri::command]
pub fn app_set_security_settings(
    app: AppHandle,
    input: crate::services::security::SecuritySettingsInput,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    crate::services::security::save_settings(&app, input)
}

#[tauri::command]
pub fn app_reset_security_backup_password(
    app: AppHandle,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    crate::services::security::reset_backup_password(&app)
}

#[tauri::command]
pub fn app_verify_security_password(
    app: AppHandle,
    mut password: String,
) -> Result<bool, AppError> {
    let result = crate::services::security::verify_lock_password(&app, &password);
    password.zeroize();
    result
}

fn current_local_terminal_shell(preferences: &UiPreferences) -> String {
    #[cfg(target_os = "windows")]
    {
        preferences.local_terminal_shells.win32.clone()
    }

    #[cfg(target_os = "macos")]
    {
        preferences.local_terminal_shells.darwin.clone()
    }

    #[cfg(target_os = "linux")]
    {
        preferences.local_terminal_shells.linux.clone()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        crate::sessions::local_terminal::default_launch().shell
    }
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
            local_terminal_shells: None,
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
