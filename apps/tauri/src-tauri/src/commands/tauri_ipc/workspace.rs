use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::commands as shared;
use crate::runtime::StreamChannel;
use crate::AppError;

#[tauri::command]
pub async fn app_open_profile(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_open_profile(crate::runtime_app(&app), profile_id).await
}

#[tauri::command]
pub async fn app_open_profile_background(
    app: AppHandle,
    profile_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_open_profile_background(crate::runtime_app(&app), profile_id).await
}

#[tauri::command]
pub async fn app_activate_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_activate_tab(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_attach_background_session(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_attach_background_session(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_detach_session_to_background(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_detach_session_to_background(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_hide_session_to_background(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_hide_session_to_background(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_reconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_reconnect_tab(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_retry_monitoring(
    app: AppHandle,
    tab_id: String,
    generation: u64,
) -> Result<(), AppError> {
    shared::app_retry_monitoring(crate::runtime_app(&app), tab_id, generation).await
}

#[tauri::command]
pub async fn app_get_network_connections(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_get_network_connections(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_probe_topology_services(
    targets: Vec<shared::TopologyTcpProbeTarget>,
) -> Result<Vec<shared::TopologyTcpProbeResult>, AppError> {
    shared::app_probe_topology_services(targets).await
}

#[tauri::command]
pub async fn app_set_monitoring_enabled(
    app: AppHandle,
    tab_id: String,
    generation: u64,
    enabled: bool,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_monitoring_enabled(crate::runtime_app(&app), tab_id, generation, enabled).await
}

#[tauri::command]
pub async fn app_disconnect_tab(
    app: AppHandle,
    tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_disconnect_tab(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_close_tab(app: AppHandle, tab_id: String) -> Result<serde_json::Value, AppError> {
    shared::app_close_tab(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_split_tab(
    app: AppHandle,
    source_tab_id: String,
    direction: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_split_tab(crate::runtime_app(&app), source_tab_id, direction).await
}

#[tauri::command]
pub async fn app_close_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_close_pane(crate::runtime_app(&app), root_tab_id, pane_tab_id).await
}

#[tauri::command]
pub async fn app_set_active_pane(
    app: AppHandle,
    root_tab_id: String,
    pane_tab_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_active_pane(crate::runtime_app(&app), root_tab_id, pane_tab_id).await
}

#[tauri::command]
pub async fn app_set_pane_weights(
    app: AppHandle,
    root_tab_id: String,
    pane_path: Vec<usize>,
    weights: Vec<f32>,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_pane_weights(crate::runtime_app(&app), root_tab_id, pane_path, weights).await
}

#[tauri::command]
pub async fn app_open_local_terminal(
    app: AppHandle,
    options: Option<crate::sessions::local_terminal::LocalTerminalLaunchOptions>,
) -> Result<serde_json::Value, AppError> {
    shared::app_open_local_terminal(crate::runtime_app(&app), options).await
}

#[tauri::command]
pub async fn app_write_terminal(
    app: AppHandle,
    tab_id: String,
    data: String,
) -> Result<(), AppError> {
    shared::app_write_terminal(crate::runtime_app(&app), tab_id, data).await
}

#[tauri::command]
pub fn app_subscribe_terminal_data(
    app: AppHandle,
    window: WebviewWindow,
    channel: Channel<serde_json::Value>,
) {
    let runtime = crate::runtime_app(&app);
    let channel_id = shared::app_subscribe_terminal_data(
        runtime,
        window.label().to_string(),
        StreamChannel::from_tauri(channel),
    );
    let channels = Arc::clone(
        &app.state::<crate::services::workspace::WorkspaceState>()
            .terminal_output_channels,
    );
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            if let Ok(mut channels) = channels.lock() {
                if let Some(subscription) = channels.remove(&channel_id) {
                    subscription.close();
                }
            }
        }
    });
}

#[tauri::command]
pub fn app_ack_terminal_data(
    app: AppHandle,
    window: WebviewWindow,
    channel_id: u32,
    delivery_id: u64,
) {
    shared::app_ack_terminal_data(
        crate::runtime_app(&app),
        window.label().to_string(),
        channel_id,
        delivery_id,
    );
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
    shared::app_resize_terminal(crate::runtime_app(&app), tab_id, cols, rows, width, height).await
}

#[tauri::command]
pub async fn app_open_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_open_remote_path(crate::runtime_app(&app), tab_id, target_path).await
}

#[tauri::command]
pub async fn app_list_remote_directory(
    app: AppHandle,
    tab_id: String,
    target_path: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_remote_directory(crate::runtime_app(&app), tab_id, target_path).await
}

#[tauri::command]
pub async fn app_set_follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    enabled: bool,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_follow_shell_cwd(crate::runtime_app(&app), tab_id, enabled).await
}

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
    shared::app_execute_remote_command(
        crate::runtime_app(&app),
        tab_id,
        command,
        cwd,
        timeout_ms,
        sudo_password,
        su_password,
        save_sudo_password,
        save_su_password,
    )
    .await
}

#[tauri::command]
pub async fn app_read_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    shared::app_read_remote_file(crate::runtime_app(&app), tab_id, target_path, encoding).await
}

#[tauri::command]
pub async fn app_write_remote_file(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_write_remote_file(
        crate::runtime_app(&app),
        tab_id,
        target_path,
        content,
        encoding,
    )
    .await
}

#[tauri::command]
pub async fn app_create_remote_directory(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_create_remote_directory(crate::runtime_app(&app), tab_id, parent_path, name).await
}

#[tauri::command]
pub async fn app_create_remote_file(
    app: AppHandle,
    tab_id: String,
    parent_path: String,
    name: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_create_remote_file(crate::runtime_app(&app), tab_id, parent_path, name).await
}

#[tauri::command]
pub async fn app_copy_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
    target_type: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_copy_remote_path(
        crate::runtime_app(&app),
        tab_id,
        target_path,
        destination_path,
        target_type,
    )
    .await
}

#[tauri::command]
pub async fn app_move_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    destination_path: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_move_remote_path(
        crate::runtime_app(&app),
        tab_id,
        target_path,
        destination_path,
    )
    .await
}

#[tauri::command]
pub async fn app_rename_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    new_name: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_rename_remote_path(crate::runtime_app(&app), tab_id, target_path, new_name).await
}

#[tauri::command]
pub async fn app_delete_remote_path(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    target_type: String,
    target_is_symlink: bool,
) -> Result<serde_json::Value, AppError> {
    shared::app_delete_remote_path(
        crate::runtime_app(&app),
        tab_id,
        target_path,
        target_type,
        target_is_symlink,
    )
    .await
}

#[tauri::command]
pub async fn app_change_remote_permissions(
    app: AppHandle,
    tab_id: String,
    target_path: String,
    options: shared::RemotePermissionChangeOptions,
) -> Result<serde_json::Value, AppError> {
    shared::app_change_remote_permissions(crate::runtime_app(&app), tab_id, target_path, options)
        .await
}

#[tauri::command]
pub async fn app_set_remote_file_access_mode(
    app: AppHandle,
    tab_id: String,
    mode: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_remote_file_access_mode(crate::runtime_app(&app), tab_id, mode, options).await
}

#[tauri::command]
pub async fn app_browse_transfer_directory(
    app: AppHandle,
    tab_id: String,
    path: String,
) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_browse_transfer_directory(crate::runtime_app(&app), tab_id, path).await
}

#[tauri::command]
pub async fn app_copy_between_servers(
    app: AppHandle,
    source_tab_id: String,
    target_tab_id: String,
    source_path: String,
    target_directory: String,
    replace_existing: bool,
) -> Result<String, AppError> {
    shared::app_copy_between_servers(
        crate::runtime_app(&app),
        source_tab_id,
        target_tab_id,
        source_path,
        target_directory,
        replace_existing,
    )
    .await
}

#[tauri::command]
pub async fn app_queue_upload(
    app: AppHandle,
    file_names: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_queue_upload(crate::runtime_app(&app), file_names).await
}

#[tauri::command]
pub async fn app_upload_file(
    app: AppHandle,
    tab_id: String,
    local_path: String,
    remote_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    shared::app_upload_file(
        crate::runtime_app(&app),
        tab_id,
        local_path,
        remote_directory,
        options,
    )
    .await
}

#[tauri::command]
pub async fn app_download_file(
    app: AppHandle,
    tab_id: String,
    remote_path: String,
    local_directory: String,
    options: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    shared::app_download_file(
        crate::runtime_app(&app),
        tab_id,
        remote_path,
        local_directory,
        options,
    )
    .await
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
    shared::app_download_remote_path(
        crate::runtime_app(&app),
        tab_id,
        remote_path,
        target_type,
        local_directory,
        options,
    )
    .await
}

#[tauri::command]
pub async fn app_cancel_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_cancel_transfer(crate::runtime_app(&app), transfer_id).await
}

#[tauri::command]
pub async fn app_pause_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_pause_transfer(crate::runtime_app(&app), transfer_id).await
}

#[tauri::command]
pub async fn app_resume_transfer(
    app: AppHandle,
    transfer_id: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_resume_transfer(crate::runtime_app(&app), transfer_id).await
}

#[tauri::command]
pub async fn app_discard_transfer(
    app: AppHandle,
    transfer_id: String,
    force: Option<bool>,
) -> Result<serde_json::Value, AppError> {
    shared::app_discard_transfer(crate::runtime_app(&app), transfer_id, force).await
}

#[tauri::command]
pub async fn app_clear_transfers(
    app: AppHandle,
    transfer_ids: Vec<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_clear_transfers(crate::runtime_app(&app), transfer_ids).await
}
