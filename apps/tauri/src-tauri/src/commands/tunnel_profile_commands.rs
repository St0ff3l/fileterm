#[tauri::command]
pub async fn app_list_tunnel_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::tunnels::list(&app)
}

#[tauri::command]
pub async fn app_save_tunnel_profile(
    app: AppHandle,
    input: crate::services::tunnels::SaveTunnelInput,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::tunnels::save(&app, input)
}

#[tauri::command]
pub async fn app_delete_tunnel_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::tunnels::delete(&app, &id)
}
