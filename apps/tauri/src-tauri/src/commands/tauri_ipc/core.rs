use tauri::{AppHandle, WebviewWindow};

use crate::commands as shared;
use crate::AppError;

#[tauri::command]
pub fn app_get_platform() -> String {
    shared::app_get_platform()
}

#[tauri::command]
pub fn app_get_linux_window_corner_style() -> &'static str {
    shared::app_get_linux_window_corner_style()
}

#[tauri::command]
pub fn app_get_mcp_agent_setup() -> Result<shared::McpAgentSetup, AppError> {
    shared::app_get_mcp_agent_setup()
}

#[tauri::command]
pub async fn app_read_mcp_call_log(
    app: AppHandle,
    limit: Option<usize>,
    offset: Option<usize>,
    before: Option<String>,
) -> Result<crate::services::mcp::McpCallLogPage, AppError> {
    shared::app_read_mcp_call_log(crate::runtime_app(&app), limit, offset, before).await
}

#[tauri::command]
pub fn app_get_arch() -> String {
    shared::app_get_arch()
}

#[tauri::command]
pub fn app_get_runtime_version() -> String {
    shared::app_get_runtime_version()
}

#[tauri::command]
pub fn app_read_clipboard_text() -> Result<String, AppError> {
    shared::app_read_clipboard_text()
}

#[tauri::command]
pub fn app_write_clipboard_text(text: String) -> Result<(), AppError> {
    shared::app_write_clipboard_text(text)
}

#[tauri::command]
pub async fn app_open_external_url(app: AppHandle, url: String) -> Result<(), AppError> {
    shared::app_open_external_url(crate::runtime_app(&app), url).await
}

#[tauri::command]
pub async fn app_get_update_status(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_get_update_status(crate::runtime_app(&app)).await
}

#[tauri::command]
pub async fn app_check_for_updates(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_check_for_updates(crate::runtime_app(&app)).await
}

#[tauri::command]
pub async fn app_download_update(app: AppHandle) -> Result<(), AppError> {
    shared::app_download_update(crate::runtime_app(&app)).await
}

#[tauri::command]
pub async fn app_install_update(app: AppHandle) -> Result<(), AppError> {
    shared::app_install_update(crate::runtime_app(&app)).await
}

#[tauri::command]
pub fn app_open_logs_directory(app: AppHandle) -> Result<(), AppError> {
    shared::app_open_logs_directory(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_write_diagnostic_log(
    app: AppHandle,
    level: String,
    scope: String,
    message: String,
) -> Result<(), AppError> {
    shared::app_write_diagnostic_log(crate::runtime_app(&app), level, scope, message)
}

#[tauri::command]
pub async fn app_list_serial_ports() -> Result<Vec<shared::SerialPortListItem>, AppError> {
    shared::app_list_serial_ports().await
}

#[tauri::command]
pub async fn app_serial_control(
    app: AppHandle,
    tab_id: String,
    action: String,
    value: Option<bool>,
    duration_ms: Option<u64>,
) -> Result<crate::sessions::SerialLineStatus, AppError> {
    shared::app_serial_control(crate::runtime_app(&app), tab_id, action, value, duration_ms).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn app_serial_transfer(
    app: AppHandle,
    tab_id: String,
    direction: String,
    mode: String,
    local_path: String,
    file_name: Option<String>,
    local_paths: Option<Vec<String>>,
    xmodem_preserve_padding: Option<bool>,
) -> Result<crate::sessions::SerialTransferResult, AppError> {
    shared::app_serial_transfer(
        crate::runtime_app(&app),
        tab_id,
        direction,
        mode,
        local_path,
        file_name,
        local_paths,
        xmodem_preserve_padding,
    )
    .await
}

#[tauri::command]
pub async fn app_serial_cancel_transfer(app: AppHandle, tab_id: String) -> Result<(), AppError> {
    shared::app_serial_cancel_transfer(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub async fn app_save_session_log(
    app: AppHandle,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    shared::app_save_session_log(crate::runtime_app(&app), tab_id).await
}

#[tauri::command]
pub fn app_get_ui_preferences(app: AppHandle) -> Result<shared::UiPreferences, AppError> {
    shared::app_get_ui_preferences(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_set_ui_preferences(
    app: AppHandle,
    input: shared::UiPreferencesInput,
) -> Result<shared::UiPreferences, AppError> {
    shared::app_set_ui_preferences(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_get_security_settings(
    app: AppHandle,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    shared::app_get_security_settings(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_set_security_settings(
    app: AppHandle,
    input: crate::services::security::SecuritySettingsInput,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    shared::app_set_security_settings(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_reset_security_backup_password(
    app: AppHandle,
) -> Result<crate::services::security::SecuritySettings, AppError> {
    shared::app_reset_security_backup_password(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_verify_security_password(app: AppHandle, password: String) -> Result<bool, AppError> {
    shared::app_verify_security_password(crate::runtime_app(&app), password)
}

#[tauri::command]
pub fn app_list_local_terminal_shells(
) -> Vec<crate::sessions::local_terminal::LocalTerminalShellOption> {
    shared::app_list_local_terminal_shells()
}

#[tauri::command]
pub fn app_get_ui_state_item(app: AppHandle, key: String) -> Result<Option<String>, AppError> {
    shared::app_get_ui_state_item(crate::runtime_app(&app), key)
}

#[tauri::command]
pub fn app_set_ui_state_item(app: AppHandle, key: String, value: String) -> Result<(), AppError> {
    shared::app_set_ui_state_item(crate::runtime_app(&app), key, value)
}

#[tauri::command]
pub fn app_remove_ui_state_item(app: AppHandle, key: String) -> Result<(), AppError> {
    shared::app_remove_ui_state_item(crate::runtime_app(&app), key)
}

#[tauri::command]
pub fn app_get_terminal_command_history(
    app: AppHandle,
    profile_id: String,
) -> Result<Vec<shared::TerminalCommandHistoryEntry>, AppError> {
    shared::app_get_terminal_command_history(crate::runtime_app(&app), profile_id)
}

#[tauri::command]
pub fn app_set_terminal_command_history(
    app: AppHandle,
    profile_id: String,
    entries: Vec<shared::TerminalCommandHistoryEntry>,
) -> Result<(), AppError> {
    shared::app_set_terminal_command_history(crate::runtime_app(&app), profile_id, entries)
}

#[tauri::command]
pub fn app_get_path_bookmarks(
    app: AppHandle,
    scope: String,
) -> Result<Vec<shared::PathBookmark>, AppError> {
    shared::app_get_path_bookmarks(crate::runtime_app(&app), scope)
}

#[tauri::command]
pub fn app_update_path_bookmark(
    app: AppHandle,
    scope: String,
    action: shared::BookmarkAction,
    bookmark: shared::PathBookmark,
) -> Result<Vec<shared::PathBookmark>, AppError> {
    shared::app_update_path_bookmark(crate::runtime_app(&app), scope, action, bookmark)
}

#[tauri::command]
pub fn app_get_command_send_preferences(
    app: AppHandle,
) -> Result<shared::CommandSendPreferences, AppError> {
    shared::app_get_command_send_preferences(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_set_command_send_preferences(
    app: AppHandle,
    preferences: shared::CommandSendPreferences,
) -> Result<(), AppError> {
    shared::app_set_command_send_preferences(crate::runtime_app(&app), preferences)
}

#[tauri::command]
pub async fn app_get_snapshot(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_get_snapshot(crate::runtime_app(&app)).await
}

#[tauri::command]
pub async fn app_get_connection_library(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_get_connection_library(crate::runtime_app(&app)).await
}

#[tauri::command]
pub fn app_list_imported_fonts(
    app: AppHandle,
) -> Result<Vec<crate::services::fonts::ImportedFont>, AppError> {
    shared::app_list_imported_fonts(crate::runtime_app(&app))
}

#[tauri::command]
pub async fn app_import_font(
    app: AppHandle,
) -> Result<Option<crate::services::fonts::ImportedFont>, AppError> {
    shared::app_import_font(crate::runtime_app(&app)).await
}

#[tauri::command]
pub fn app_get_imported_font_data(
    app: AppHandle,
    font_id: String,
) -> Result<Option<String>, AppError> {
    shared::app_get_imported_font_data(crate::runtime_app(&app), font_id)
}

#[tauri::command]
pub fn app_delete_imported_font(app: AppHandle, font_id: String) -> Result<bool, AppError> {
    shared::app_delete_imported_font(crate::runtime_app(&app), font_id)
}

#[tauri::command]
pub fn app_list_ssh_keys(app: AppHandle) -> Result<Vec<serde_json::Value>, AppError> {
    shared::app_list_ssh_keys(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_list_ssh_config_hosts(
    app: AppHandle,
) -> Result<Vec<crate::services::ssh_config::SshConfigHostSummary>, AppError> {
    shared::app_list_ssh_config_hosts(crate::runtime_app(&app))
}

#[tauri::command]
pub async fn app_select_ssh_key_file(
    app: AppHandle,
) -> Result<Option<serde_json::Value>, AppError> {
    shared::app_select_ssh_key_file(crate::runtime_app(&app)).await
}

#[tauri::command]
pub fn app_import_ssh_key(
    app: AppHandle,
    input: Option<shared::ImportSshKeyInput>,
) -> Result<Option<serde_json::Value>, AppError> {
    shared::app_import_ssh_key(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_update_ssh_key_note(
    app: AppHandle,
    key_id: String,
    note: String,
) -> Result<serde_json::Value, AppError> {
    shared::app_update_ssh_key_note(crate::runtime_app(&app), key_id, note)
}

#[tauri::command]
pub fn app_delete_ssh_key(app: AppHandle, key_id: String) -> Result<(), AppError> {
    shared::app_delete_ssh_key(crate::runtime_app(&app), key_id)
}

#[tauri::command]
pub async fn app_preview_connection_import(
    app: AppHandle,
    source: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    shared::app_preview_connection_import(crate::runtime_app(&app), source).await
}

#[tauri::command]
pub async fn app_commit_connection_json_import(
    app: AppHandle,
    plan_id: String,
    options: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_commit_connection_json_import(crate::runtime_app(&app), plan_id, options).await
}

#[tauri::command]
pub async fn app_export_connections(app: AppHandle, format: String) -> Result<bool, AppError> {
    shared::app_export_connections(crate::runtime_app(&app), format).await
}

#[tauri::command]
pub async fn app_export_connections_as_files(
    app: AppHandle,
    format: String,
) -> Result<bool, AppError> {
    shared::app_export_connections_as_files(crate::runtime_app(&app), format).await
}

#[tauri::command]
pub fn app_get_webdav_sync_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_get_webdav_sync_config(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_set_webdav_sync_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_webdav_sync_config(crate::runtime_app(&app), input)
}

#[tauri::command]
pub async fn app_test_webdav_sync(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_test_webdav_sync(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_upload_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_upload_webdav_sync(crate::runtime_app(&app), mode).await
}

#[tauri::command]
pub async fn app_download_webdav_sync(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_download_webdav_sync(crate::runtime_app(&app), mode).await
}

#[tauri::command]
pub fn app_get_s3_backup_config(app: AppHandle) -> Result<serde_json::Value, AppError> {
    shared::app_get_s3_backup_config(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_set_s3_backup_config(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_set_s3_backup_config(crate::runtime_app(&app), input)
}

#[tauri::command]
pub async fn app_test_s3_backup(
    app: AppHandle,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_test_s3_backup(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub async fn app_upload_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_upload_s3_backup(crate::runtime_app(&app), mode).await
}

#[tauri::command]
pub async fn app_download_s3_backup(
    app: AppHandle,
    mode: Option<String>,
) -> Result<serde_json::Value, AppError> {
    shared::app_download_s3_backup(crate::runtime_app(&app), mode).await
}

#[tauri::command]
pub async fn app_workspace_mutation(
    app: AppHandle,
    operation: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    shared::app_workspace_mutation(crate::runtime_app(&app), operation, payload).await
}

#[tauri::command]
pub async fn app_open_window(
    app: AppHandle,
    input: shared::OpenWindowInput,
) -> Result<(), AppError> {
    shared::app_open_window(app, input).await
}

#[tauri::command]
pub async fn app_window_action(
    app: AppHandle,
    window: WebviewWindow,
    action: String,
) -> Result<(), AppError> {
    shared::app_window_action(app, window, action).await
}

#[tauri::command]
pub fn app_is_window_maximized(window: WebviewWindow) -> bool {
    shared::app_is_window_maximized(window)
}

#[tauri::command]
pub fn app_cancel_file_editor_close(app: AppHandle, window: WebviewWindow) {
    shared::app_cancel_file_editor_close(app, window);
}

#[tauri::command]
pub fn app_show_window_menu(
    app: AppHandle,
    window: WebviewWindow,
    menu_type: String,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    shared::app_show_window_menu(app, window, menu_type, x, y)
}
