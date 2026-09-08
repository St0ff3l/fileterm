#[tauri::command]
pub fn app_list_proxy_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    crate::services::proxies::list(&app)
}

#[tauri::command]
pub fn app_save_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::SaveProxyInput,
) -> Result<serde_json::Value, AppError> {
    crate::services::proxies::save(&app, input)
}

#[tauri::command]
pub fn app_delete_proxy_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    crate::services::proxies::delete(&app, &id)
}

#[tauri::command]
pub async fn app_test_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::TestProxyInput,
) -> Result<crate::services::proxies::ProxyTestResult, AppError> {
    crate::services::proxies::test_proxy(&app, input).await
}
