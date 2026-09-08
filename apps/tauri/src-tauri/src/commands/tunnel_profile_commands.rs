#[tauri::command]
pub fn app_list_tunnel_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    crate::services::tunnels::list(&app)
}

#[tauri::command]
pub fn app_save_tunnel_profile(
    app: AppHandle,
    input: crate::services::tunnels::SaveTunnelInput,
) -> Result<serde_json::Value, AppError> {
    crate::services::tunnels::save(&app, input)
}

#[tauri::command]
pub fn app_delete_tunnel_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    crate::services::tunnels::delete(&app, &id)
}
