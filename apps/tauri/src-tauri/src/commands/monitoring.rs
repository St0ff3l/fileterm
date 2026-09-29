#[tauri::command]
pub async fn app_retry_monitoring(
    app: AppHandle,
    tab_id: String,
    generation: u64,
) -> Result<(), AppError> {
    app.state::<crate::services::workspace::WorkspaceState>()
        .retry_monitoring(&tab_id, generation)
        .await
        .map_err(AppError::Storage)
}

#[tauri::command]
pub async fn app_set_monitoring_enabled(
    app: AppHandle,
    tab_id: String,
    generation: u64,
    enabled: bool,
) -> Result<Value, AppError> {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let queued_before_startup = state
        .set_monitoring_enabled(&tab_id, generation, enabled)
        .await
        .map_err(AppError::Storage)?;
    if !queued_before_startup {
        state
            .wait_for_monitoring_enabled(&tab_id, generation, enabled)
            .await
            .map_err(AppError::Storage)?;
    }
    get_workspace_snapshot_and_emit(&app).await
}

fn monitoring_enabled_for_profile(profile: &Value) -> bool {
    profile.get("type").and_then(Value::as_str) == Some("ssh")
        && profile.get("deviceMode").and_then(Value::as_str) != Some("network-device")
        && profile.get("enableExecChannel").and_then(Value::as_bool) != Some(false)
        && profile
            .get("enableResourceMonitoring")
            .and_then(Value::as_bool)
            != Some(false)
}

async fn stop_disabled_monitoring(app: &AppHandle, profile: &Value) {
    if !monitoring_enabled_for_profile(profile) {
        if let Some(id) = profile.get("id").and_then(Value::as_str) {
            app.state::<crate::services::workspace::WorkspaceState>()
                .stop_monitoring_for_profile(id)
                .await;
        }
    }
}

fn schedule_disabled_monitoring_cleanup(app: &AppHandle, defaults: &SshConnectionDefaults) {
    let Ok(profiles) = read_json_array(app, "profiles.json") else {
        return;
    };
    let disabled: Vec<_> = profiles
        .iter()
        .map(|profile| resolve_profile_with_connection_defaults(profile, defaults))
        .filter(|profile| !monitoring_enabled_for_profile(profile))
        .collect();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if disabled.is_empty() {
            return;
        }
        for profile in disabled {
            stop_disabled_monitoring(&app, &profile).await;
        }
        let _ = get_workspace_snapshot_and_emit(&app).await;
    });
}
