pub mod commands;
pub mod services;
pub mod sessions;
pub mod storage;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry,
};
use thiserror::Error;
use url::form_urlencoded::Serializer;
use crate::commands::OpenWindowInput;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("clipboard error: {0}")]
    Clipboard(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("window error: {0}")]
    Window(String),
    #[error("command error: {0}")]
    Command(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

fn window_query(input: &OpenWindowInput) -> String {
    let mut serializer = Serializer::new(String::new());
    serializer.append_pair("window", &input.kind);
    if let Some(value) = &input.mode {
        serializer.append_pair("mode", value);
    }
    if let Some(value) = &input.profile_id {
        serializer.append_pair("profileId", value);
    }
    if let Some(value) = &input.command_id {
        serializer.append_pair("commandId", value);
    }
    if let Some(value) = &input.folder_id {
        serializer.append_pair("folderId", value);
    }
    if let Some(value) = &input.source {
        serializer.append_pair("source", value);
    }
    if let Some(value) = &input.path {
        serializer.append_pair("path", value);
    }
    if let Some(value) = &input.name {
        serializer.append_pair("name", value);
    }
    if let Some(value) = &input.tab_id {
        serializer.append_pair("tabId", value);
    }
    if let Some(value) = &input.encoding {
        serializer.append_pair("encoding", value);
    }
    serializer.finish()
}

fn window_label(input: &OpenWindowInput) -> String {
    match input.kind.as_str() {
        "connection-manager" => "connection-manager".to_string(),
        "command-manager" => "command-manager".to_string(),
        "connection-form" => "connection-form".to_string(),
        "command-form" => "command-form".to_string(),
        "file-editor" => {
            let key = format!(
                "{}:{}:{}",
                input.source.as_deref().unwrap_or(""),
                input.tab_id.as_deref().unwrap_or(""),
                input.path.as_deref().unwrap_or("")
            );
            let hash = key.bytes().fold(0_u64, |value, byte| {
                value.wrapping_mul(31).wrapping_add(byte as u64)
            });
            format!("file-editor-{hash:x}")
        }
        _ => "main".to_string(),
    }
}

fn window_url(input: &OpenWindowInput) -> WebviewUrl {
    WebviewUrl::App(format!("index.html?{}", window_query(input)).into())
}

pub fn open_child_window(app: &AppHandle, input: OpenWindowInput) -> Result<(), AppError> {
    let label = window_label(&input);
    if let Some(window) = app.get_webview_window(&label) {
        window
            .show()
            .map_err(|error| AppError::Window(error.to_string()))?;
        window
            .set_focus()
            .map_err(|error| AppError::Window(error.to_string()))?;
        return Ok(());
    }

    let (title, width, height, min_width, min_height, decorations) = match input.kind.as_str() {
        "connection-manager" => ("连接管理器", 860.0, 680.0, 760.0, 520.0, true),
        "command-manager" => ("命令管理器", 860.0, 680.0, 760.0, 620.0, true),
        "connection-form" => ("连接", 860.0, 680.0, 760.0, 620.0, false),
        "command-form" => ("命令", 860.0, 680.0, 760.0, 620.0, false),
        "file-editor" => ("编辑文件", 1220.0, 780.0, 1040.0, 620.0, false),
        _ => return Ok(()),
    };

    let window = WebviewWindowBuilder::new(app, &label, window_url(&input))
        .title(title)
        .inner_size(width, height)
        .min_inner_size(min_width, min_height)
        .center()
        .decorations(decorations)
        .transparent(!decorations)
        .build()
        .map_err(|error| AppError::Window(error.to_string()))?;
    if input.kind == "file-editor" {
        let editor_window = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = editor_window.emit("app:file-editor-close-request", ());
            }
        });
    }
    Ok(())
}

fn show_main_window(app: &AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn request_main_close(app: &AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit(
            "app:window-close-request",
            serde_json::json!({ "isQuit": true }),
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(crate::services::WorkspaceState::default());

            let main_window = app.get_webview_window("main")
                .ok_or_else(|| "Failed to find main window".to_string())?;

            // ── Platform-specific window chrome ────────────────────────────
            // macOS: keep decorations + Overlay titleBarStyle (traffic light
            //        floats over content), aligned with Electron hiddenInset.
            // Windows: drop the OS frame so the renderer owns the title bar.
            // Linux: keep native decorations.
            #[cfg(target_os = "windows")]
            {
                let _ = main_window.set_decorations(false);
            }

            let app_handle = app.handle().clone();
            main_window.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        request_main_close(&app_handle);
                    }
                    WindowEvent::Resized(_) => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = app_handle.emit(
                                "app:window-maximized-change",
                                window.is_maximized().unwrap_or(false),
                            );
                        }
                    }
                    _ => {}
                }
            });

            // Native menu building
            let new_connection_menu = MenuItemBuilder::with_id("new-connection", "新建连接")
                .accelerator("CmdOrCtrl+N")
                .build(app)
                .map_err(|error| error.to_string())?;
            let connection_manager_menu = MenuItemBuilder::with_id("connection-manager", "连接管理器")
                .accelerator("CmdOrCtrl+Shift+C")
                .build(app)
                .map_err(|error| error.to_string())?;
            let command_manager_menu = MenuItemBuilder::with_id("command-manager", "命令管理器")
                .accelerator("CmdOrCtrl+Shift+P")
                .build(app)
                .map_err(|error| error.to_string())?;

            let file_submenu = SubmenuBuilder::new(app, "文件")
                .item(&new_connection_menu)
                .item(&connection_manager_menu)
                .item(&command_manager_menu)
                .separator()
                .item(
                    &MenuItemBuilder::with_id("quit", "退出 FileTerm")
                        .accelerator("CmdOrCtrl+Q")
                        .build(app)
                        .map_err(|error| error.to_string())?,
                )
                .build()
                .map_err(|error| error.to_string())?;

            let menu = MenuBuilder::new(app).item(&file_submenu).build().map_err(|error| error.to_string())?;
            app.set_menu(menu).map_err(|error| error.to_string())?;

            // Tray configuration
            let tray_connection_manager = MenuItemBuilder::with_id("tray-connection-manager", "连接管理器")
                .build(app)
                .map_err(|error| error.to_string())?;
            let tray_command_manager = MenuItemBuilder::with_id("tray-command-manager", "命令管理器")
                .build(app)
                .map_err(|error| error.to_string())?;
            let tray_show_main = MenuItemBuilder::with_id("tray-show-main", "显示主窗口")
                .build(app)
                .map_err(|error| error.to_string())?;
            let tray_quit = MenuItemBuilder::with_id("tray-quit", "退出 FileTerm")
                .build(app)
                .map_err(|error| error.to_string())?;

            let tray_menu = MenuBuilder::new(app)
                .item(&tray_show_main)
                .separator()
                .item(&tray_connection_manager)
                .item(&tray_command_manager)
                .separator()
                .item(&tray_quit)
                .build()
                .map_err(|error| error.to_string())?;

            if let Some(tray) = app.tray_by_id("main") {
                let _ = tray.set_menu(Some(tray_menu));
                let _ = tray.on_menu_event(|app, event| match event.id().as_ref() {
                    "tray-connection-manager" => {
                        let _ = open_child_window(
                            app,
                            OpenWindowInput {
                                kind: "connection-manager".to_string(),
                                mode: None,
                                profile_id: None,
                                command_id: None,
                                folder_id: None,
                                source: None,
                                path: None,
                                name: None,
                                tab_id: None,
                                encoding: None,
                            },
                        );
                    }
                    "tray-command-manager" => {
                        let _ = open_child_window(
                            app,
                            OpenWindowInput {
                                kind: "command-manager".to_string(),
                                mode: None,
                                profile_id: None,
                                command_id: None,
                                folder_id: None,
                                source: None,
                                path: None,
                                name: None,
                                tab_id: None,
                                encoding: None,
                            },
                        );
                    }
                    "tray-show-main" => show_main_window(app),
                    "tray-quit" => request_main_close(app),
                    _ => {}
                });
            }

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-connection" => {
                let _ = open_child_window(
                    app,
                    OpenWindowInput {
                        kind: "connection-form".to_string(),
                        mode: Some("create".to_string()),
                        profile_id: None,
                        command_id: None,
                        folder_id: None,
                        source: None,
                        path: None,
                        name: None,
                        tab_id: None,
                        encoding: None,
                    },
                );
            }
            "connection-manager" => {
                let _ = open_child_window(
                    app,
                    OpenWindowInput {
                        kind: "connection-manager".to_string(),
                        mode: None,
                        profile_id: None,
                        command_id: None,
                        folder_id: None,
                        source: None,
                        path: None,
                        name: None,
                        tab_id: None,
                        encoding: None,
                    },
                );
            }
            "command-manager" => {
                let _ = open_child_window(
                    app,
                    OpenWindowInput {
                        kind: "command-manager".to_string(),
                        mode: None,
                        profile_id: None,
                        command_id: None,
                        folder_id: None,
                        source: None,
                        path: None,
                        name: None,
                        tab_id: None,
                        encoding: None,
                    },
                );
            }
            "show-main" => show_main_window(app),
            "quit" => request_main_close(app),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::app_get_platform,
            crate::commands::app_get_arch,
            crate::commands::app_read_clipboard_text,
            crate::commands::app_write_clipboard_text,
            crate::commands::app_open_external_url,
            crate::commands::app_get_update_status,
            crate::commands::app_check_for_updates,
            crate::commands::app_download_update,
            crate::commands::app_install_update,
            crate::commands::app_open_logs_directory,
            crate::commands::app_get_ui_preferences,
            crate::commands::app_set_ui_preferences,
            crate::commands::app_get_ui_state_item,
            crate::commands::app_set_ui_state_item,
            crate::commands::app_remove_ui_state_item,
            crate::commands::app_get_terminal_command_history,
            crate::commands::app_set_terminal_command_history,
            crate::commands::app_get_command_send_preferences,
            crate::commands::app_set_command_send_preferences,
            crate::commands::app_get_snapshot,
            crate::commands::app_get_connection_library,
            crate::commands::app_preview_connection_import,
            crate::commands::app_commit_connection_json_import,
            crate::commands::app_export_connections,
            crate::commands::app_export_connections_as_files,
            crate::commands::app_get_webdav_sync_config,
            crate::commands::app_set_webdav_sync_config,
            crate::commands::app_upload_webdav_sync,
            crate::commands::app_download_webdav_sync,
            crate::commands::app_workspace_mutation,
            crate::commands::app_open_window,
            crate::commands::app_window_action,
            crate::commands::app_is_window_maximized,
            
            // Phase 3 commands
            crate::commands::app_open_profile,
            crate::commands::app_activate_tab,
            crate::commands::app_reconnect_tab,
            crate::commands::app_disconnect_tab,
            crate::commands::app_close_tab,
            crate::commands::app_write_terminal,
            crate::commands::app_resize_terminal,
            crate::commands::app_open_remote_path,
            crate::commands::app_set_follow_shell_cwd,
            crate::commands::app_read_remote_file,
            crate::commands::app_write_remote_file,
            crate::commands::app_create_remote_directory,
            crate::commands::app_create_remote_file,
            crate::commands::app_copy_remote_path,
            crate::commands::app_move_remote_path,
            crate::commands::app_rename_remote_path,
            crate::commands::app_delete_remote_path,
            crate::commands::app_change_remote_permissions,
            crate::commands::app_set_remote_file_access_mode,
            crate::commands::app_queue_upload,
            crate::commands::app_upload_file,
            crate::commands::app_download_file,
            crate::commands::app_download_remote_path,
            crate::commands::app_cancel_transfer,
            crate::commands::app_pause_transfer,
            crate::commands::app_resume_transfer,
            crate::commands::app_discard_transfer,
            crate::commands::app_clear_transfers,
            crate::commands::app_resolve_ssh_interaction,
            crate::commands::app_list_ssh_tunnels,
            crate::commands::app_create_ssh_tunnel,
            crate::commands::app_start_ssh_tunnel,
            crate::commands::app_stop_ssh_tunnel,
            crate::commands::app_delete_ssh_tunnel,

            // Phase 2: profile / folder / command CRUD
            crate::commands::app_create_profile,
            crate::commands::app_update_profile,
            crate::commands::app_delete_profile,
            crate::commands::app_update_folder,
            crate::commands::app_delete_folder,
            crate::commands::app_update_entity_order,
            crate::commands::app_update_command_folder,
            crate::commands::app_delete_command_folder,
            crate::commands::app_update_command_order,
            crate::commands::app_update_command_template,
            crate::commands::app_delete_command_template,

            // Local files
            crate::sessions::local_files::app_list_local_directory,
            crate::sessions::local_files::app_read_local_file,
            crate::sessions::local_files::app_write_local_file,
            crate::sessions::local_files::app_create_local_directory,
            crate::sessions::local_files::app_create_local_file,
            crate::sessions::local_files::app_copy_local_path,
            crate::sessions::local_files::app_move_local_path,
            crate::sessions::local_files::app_rename_local_path,
            crate::sessions::local_files::app_delete_local_path,
            crate::sessions::local_files::app_change_local_permissions,
            crate::sessions::local_files::app_select_local_files,
            crate::sessions::local_files::app_select_local_directory
        ])
        .build(tauri::generate_context!())
        .expect("error while building FileTerm Tauri application")
        .run(|app_handle, event| {
            // macOS: clicking the dock icon when the main window is hidden
            // should bring it back (mirrors Electron `activate`).
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app_handle);
            }
        });
}
