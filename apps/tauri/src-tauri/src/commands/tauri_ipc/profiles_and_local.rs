use std::collections::BTreeMap;

use tauri::{AppHandle, WebviewWindow};

use crate::commands as shared;
use crate::sessions::local_files;
use crate::AppError;

#[tauri::command]
pub async fn app_resolve_ssh_interaction(
    app: AppHandle,
    request_id: String,
    response: serde_json::Value,
) -> Result<(), AppError> {
    shared::app_resolve_ssh_interaction(crate::runtime_app(&app), request_id, response).await
}

#[tauri::command]
pub async fn app_resolve_sudo_password_prompt(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
    save: Option<bool>,
) -> Result<(), AppError> {
    shared::app_resolve_sudo_password_prompt(
        crate::runtime_app(&app),
        request_id,
        cancelled,
        value,
        save,
    )
    .await
}

#[tauri::command]
pub async fn app_set_sudo_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    shared::app_set_sudo_password_renderer_ready(
        crate::runtime_app(&app),
        window.label().to_string(),
        registration_id,
        ready,
    )
    .await
}

#[tauri::command]
pub async fn app_resolve_backup_password(
    app: AppHandle,
    request_id: String,
    cancelled: bool,
    value: Option<String>,
) -> Result<(), AppError> {
    shared::app_resolve_backup_password(crate::runtime_app(&app), request_id, cancelled, value)
        .await
}

#[tauri::command]
pub async fn app_set_backup_password_renderer_ready(
    app: AppHandle,
    window: WebviewWindow,
    registration_id: String,
    ready: bool,
) -> Result<(), AppError> {
    shared::app_set_backup_password_renderer_ready(
        crate::runtime_app(&app),
        window.label().to_string(),
        registration_id,
        ready,
    )
    .await
}

#[tauri::command]
pub async fn app_list_ssh_tunnels(
    app: AppHandle,
    tab_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_ssh_tunnels(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_create_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule: serde_json::Value,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_create_ssh_tunnel(crate::runtime_app(&app), tab_id, rule).await
}

#[tauri::command]
pub async fn app_start_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_start_ssh_tunnel(crate::runtime_app(&app), tab_id, rule_id).await
}

#[tauri::command]
pub async fn app_stop_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_stop_ssh_tunnel(crate::runtime_app(&app), tab_id, rule_id).await
}

#[tauri::command]
pub async fn app_delete_ssh_tunnel(
    app: AppHandle,
    tab_id: String,
    rule_id: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_delete_ssh_tunnel(crate::runtime_app(&app), tab_id, rule_id).await
}

#[tauri::command]
pub async fn app_list_tunnel_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_tunnel_profiles(crate::runtime_app(&app)).await
}

#[tauri::command]
pub async fn app_save_tunnel_profile(
    app: AppHandle,
    input: crate::services::tunnels::SaveTunnelInput,
) -> Result<serde_json::Value, AppError> {
    shared::app_save_tunnel_profile(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_delete_tunnel_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    shared::app_delete_tunnel_profile(crate::runtime_app(&app), id).await
}

#[tauri::command]
pub async fn app_list_proxy_profiles(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_proxy_profiles(crate::runtime_app(&app)).await
}

#[tauri::command]
pub fn app_get_ssh_route_policy(
    app: AppHandle,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    shared::app_get_ssh_route_policy(crate::runtime_app(&app))
}

#[tauri::command]
pub async fn app_save_ssh_route_policy(
    app: AppHandle,
    expected: crate::sessions::ssh::SshRoutePolicy,
    policy: crate::sessions::ssh::SshRoutePolicy,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    shared::app_save_ssh_route_policy(crate::runtime_app(&app), expected, policy).await
}

#[tauri::command]
pub async fn app_probe_ssh_route_policy(
    app: AppHandle,
    profile_id: String,
) -> Result<crate::sessions::ssh::SshRoutePolicySnapshot, AppError> {
    shared::app_probe_ssh_route_policy(crate::runtime_app(&app), profile_id).await
}

#[tauri::command]
pub async fn app_save_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::SaveProxyInput,
) -> Result<serde_json::Value, AppError> {
    shared::app_save_proxy_profile(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_delete_proxy_profile(app: AppHandle, id: String) -> Result<(), AppError> {
    shared::app_delete_proxy_profile(crate::runtime_app(&app), id).await
}

#[tauri::command]
pub async fn app_test_proxy_profile(
    app: AppHandle,
    input: crate::services::proxies::TestProxyInput,
) -> Result<crate::services::proxies::ProxyTestResult, AppError> {
    shared::app_test_proxy_profile(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_create_profile(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_create_profile(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_update_profile(
    app: AppHandle,
    profile_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_profile(crate::runtime_app(&app), profile_id, input).await
}

#[tauri::command]
pub async fn app_assign_ssh_proxy(
    app: AppHandle,
    input: crate::services::profile_ops::SshProxyAssignmentInput,
) -> Result<bool, AppError> {
    shared::app_assign_ssh_proxy(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub fn app_list_ssh_network_interfaces(
) -> Result<Vec<crate::services::ssh_network_interface::AdapterInfo>, AppError> {
    shared::app_list_ssh_network_interfaces()
}

#[tauri::command]
pub async fn app_set_profile_file_pane_mode(
    app: AppHandle,
    profile_id: String,
    mode: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_profile_file_pane_mode(crate::runtime_app(&app), profile_id, mode).await
}

#[tauri::command]
pub async fn app_set_profile_default_directory(
    app: AppHandle,
    profile_id: String,
    pane: String,
    path: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_profile_default_directory(crate::runtime_app(&app), profile_id, pane, path)
        .await
}

#[tauri::command]
pub async fn app_clear_trusted_host_fingerprint(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_clear_trusted_host_fingerprint(crate::runtime_app(&app), profile_id).await
}

#[tauri::command]
pub async fn app_test_connection(
    app: AppHandle,
    window: WebviewWindow,
    profile_id: Option<String>,
    input: serde_json::Value,
) -> Result<(), AppError> {
    shared::app_test_connection(
        crate::runtime_app(&app),
        window.label().to_string(),
        profile_id,
        input,
    )
    .await
}

#[tauri::command]
pub async fn app_delete_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_delete_profile(crate::runtime_app(&app), profile_id).await
}

#[tauri::command]
pub async fn app_update_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_folder(crate::runtime_app(&app), folder_id, updates).await
}

#[tauri::command]
pub async fn app_delete_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_delete_folder(crate::runtime_app(&app), folder_id).await
}

#[tauri::command]
pub async fn app_update_entity_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_entity_order(crate::runtime_app(&app), id, new_parent_id, new_order).await
}

#[tauri::command]
pub async fn app_update_command_folder(
    app: AppHandle,
    folder_id: String,
    updates: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_command_folder(crate::runtime_app(&app), folder_id, updates).await
}

#[tauri::command]
pub async fn app_delete_command_folder(
    app: AppHandle,
    folder_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_delete_command_folder(crate::runtime_app(&app), folder_id).await
}

#[tauri::command]
pub async fn app_update_command_order(
    app: AppHandle,
    id: String,
    new_parent_id: Option<String>,
    new_order: f64,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_command_order(crate::runtime_app(&app), id, new_parent_id, new_order).await
}

#[tauri::command]
pub async fn app_update_command_template(
    app: AppHandle,
    command_id: String,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_command_template(crate::runtime_app(&app), command_id, input).await
}

#[tauri::command]
pub async fn app_delete_command_template(
    app: AppHandle,
    command_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_delete_command_template(crate::runtime_app(&app), command_id).await
}

#[tauri::command]
pub async fn app_execute_command_template(
    app: AppHandle,
    tab_id: String,
    command_id: String,
    args: Option<Vec<String>>,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    shared::app_execute_command_template(
        crate::runtime_app(&app),
        tab_id,
        command_id,
        args,
        options,
    )
    .await
}

#[tauri::command]
pub async fn app_run_command_batch(
    app: AppHandle,
    window: WebviewWindow,
    command_id: String,
    expected_template: String,
    args: BTreeMap<usize, String>,
    targets: Vec<shared::CommandBatchTarget>,
) -> Result<shared::CommandBatchRun, AppError> {
    shared::app_run_command_batch(
        crate::runtime_app(&app),
        window.label().to_string(),
        command_id,
        expected_template,
        args,
        targets,
    )
    .await
}

#[tauri::command]
pub fn app_list_command_batches(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_command_batches(crate::runtime_app(&app))
}

#[tauri::command]
pub async fn app_resolve_mcp_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    shared::app_resolve_mcp_approval(crate::runtime_app(&app), request_id, approved).await
}

#[tauri::command]
pub async fn app_resolve_action_approval(
    app: AppHandle,
    request_id: String,
    approved: bool,
) -> Result<(), AppError> {
    shared::app_resolve_action_approval(crate::runtime_app(&app), request_id, approved).await
}

#[tauri::command]
pub async fn app_execute_ai_terminal_handoff(
    app: AppHandle,
    request_id: String,
    tab_id: String,
    command: String,
) -> Result<(), AppError> {
    shared::app_execute_ai_terminal_handoff(crate::runtime_app(&app), request_id, tab_id, command)
        .await
}

#[tauri::command]
pub fn app_list_local_directory(
    dir_path: Option<String>,
) -> Result<local_files::DirectorySnapshot, AppError> {
    local_files::app_list_local_directory(dir_path)
}

#[tauri::command]
pub async fn app_connect_local_network_share(
    path: String,
    username: String,
    password: String,
    share: Option<String>,
) -> Result<local_files::LocalNetworkShareConnectionResult, AppError> {
    local_files::app_connect_local_network_share(path, username, password, share).await
}

#[tauri::command]
pub fn app_read_local_file(
    file_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    local_files::app_read_local_file(file_path, encoding)
}

#[tauri::command]
pub fn app_write_local_file(
    file_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    local_files::app_write_local_file(file_path, content, encoding)
}

#[tauri::command]
pub fn app_create_local_directory(dir_path: String, name: String) -> Result<(), AppError> {
    local_files::app_create_local_directory(dir_path, name)
}

#[tauri::command]
pub fn app_create_local_file(dir_path: String, name: String) -> Result<(), AppError> {
    local_files::app_create_local_file(dir_path, name)
}

#[tauri::command]
pub fn app_copy_local_path(source_path: String, destination_path: String) -> Result<(), AppError> {
    local_files::app_copy_local_path(source_path, destination_path)
}

#[tauri::command]
pub fn app_move_local_path(source_path: String, destination_path: String) -> Result<(), AppError> {
    local_files::app_move_local_path(source_path, destination_path)
}

#[tauri::command]
pub fn app_rename_local_path(target_path: String, new_name: String) -> Result<(), AppError> {
    local_files::app_rename_local_path(target_path, new_name)
}

#[tauri::command]
pub fn app_delete_local_path(target_path: String) -> Result<(), AppError> {
    local_files::app_delete_local_path(target_path)
}

#[tauri::command]
pub fn app_change_local_permissions(
    target_path: String,
    options: local_files::PermissionChangeOptions,
) -> Result<(), AppError> {
    local_files::app_change_local_permissions(target_path, options)
}

#[tauri::command]
pub async fn app_select_local_files(
    app: AppHandle,
    default_path: Option<String>,
) -> Result<Vec<String>, AppError> {
    local_files::app_select_local_files(crate::runtime_app(&app), default_path).await
}

#[tauri::command]
pub async fn app_select_local_directory(
    app: AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, AppError> {
    local_files::app_select_local_directory(crate::runtime_app(&app), default_path).await
}
