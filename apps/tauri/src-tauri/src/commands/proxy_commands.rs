pub async fn app_list_proxy_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::proxies::list(&app)
}

pub async fn app_save_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::SaveProxyInput,
) -> Result<serde_json::Value, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::proxies::save(&app, input)
}

pub async fn app_delete_proxy_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::services::proxies::delete(&app, &id)
}

pub async fn app_test_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::TestProxyInput,
) -> Result<crate::services::proxies::ProxyTestResult, AppError> {
    crate::services::proxies::test_proxy(&app, input).await
}

pub(crate) fn app_get_ssh_route_policy(
    app: AppHandle,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    crate::sessions::ssh::ssh_route_policy_snapshot(&app)
}

pub(crate) async fn app_save_ssh_route_policy(
    app: AppHandle,
    expected: crate::sessions::ssh::SshRoutePolicy,
    policy: crate::sessions::ssh::SshRoutePolicy,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    let _guard = lock_library_after_transfer_hydration(&app).await?;
    crate::sessions::ssh::save_ssh_route_policy(&app, expected, policy)
}

pub(crate) async fn app_probe_ssh_route_policy(
    app: AppHandle,
    profile_id: String,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    crate::sessions::ssh::probe_ssh_route_policy(&app, &profile_id).await
}
