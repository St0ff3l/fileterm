pub mod commands;
pub mod services;
pub mod sessions;
pub mod storage;

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent, Wry,
};
use thiserror::Error;
use url::form_urlencoded::Serializer;
use crate::commands::OpenWindowInput;
use std::{collections::{HashMap, HashSet}, sync::Mutex};

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

/// Tracks file-editor close requests that are waiting for a renderer answer.
///
/// A Tauri `CloseRequested` event has no Promise to resolve like Electron's
/// `close` handler does. Keeping this state in main makes cancellation a real
/// lifecycle transition instead of a renderer-only no-op, and prevents two
/// close dialogs from being emitted for the same editor window.
#[derive(Default)]
pub(crate) struct FileEditorCloseRegistry {
    pending_labels: Mutex<HashSet<String>>,
}

impl FileEditorCloseRegistry {
    fn request(&self, label: &str) -> bool {
        self.pending_labels
            .lock()
            .expect("file editor close registry lock poisoned")
            .insert(label.to_string())
    }

    fn resolve(&self, label: &str) {
        self.pending_labels
            .lock()
            .expect("file editor close registry lock poisoned")
            .remove(label);
    }
}

pub(crate) fn request_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) -> bool {
    app.state::<FileEditorCloseRegistry>().request(window.label())
}

pub(crate) fn resolve_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    app.state::<FileEditorCloseRegistry>().resolve(window.label());
}

/// Per-window zoom is not exposed by Wry as a getter. Store the scale we last
/// applied so the View menu can provide deterministic reset/in/out behavior.
#[derive(Default)]
struct WindowMenuState {
    zoom_scales: Mutex<HashMap<String, f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowMenuKind {
    App,
    File,
    View,
    Window,
}

impl TryFrom<&str> for WindowMenuKind {
    type Error = AppError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "app" => Ok(Self::App),
            "file" => Ok(Self::File),
            "view" => Ok(Self::View),
            "window" => Ok(Self::Window),
            _ => Err(AppError::Command(format!("Unsupported window menu: {value}"))),
        }
    }
}

fn localized<'a>(is_english: bool, english: &'a str, chinese: &'a str) -> &'a str {
    if is_english { english } else { chinese }
}

/// Match Electron's platform-native window shortcuts. macOS owns Cmd+Q/W;
/// Windows and Linux keep Alt+F4 for quitting and Ctrl+W for closing the
/// focused workspace item/window.
fn application_menu_accelerators(platform: &str) -> (&'static str, &'static str) {
    if platform == "macos" {
        ("Cmd+Q", "Cmd+W")
    } else {
        ("Alt+F4", "Ctrl+W")
    }
}

fn focused_webview_window(app: &AppHandle<Wry>) -> Option<WebviewWindow<Wry>> {
    app.webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
        .or_else(|| app.get_webview_window("main"))
}

fn update_focused_window_zoom(app: &AppHandle<Wry>, operation: ZoomOperation) {
    let Some(window) = focused_webview_window(app) else {
        return;
    };
    let label = window.label().to_string();
    let state = app.state::<WindowMenuState>();
    let mut zoom_scales = state
        .zoom_scales
        .lock()
        .expect("window menu zoom state lock poisoned");
    let current = zoom_scales.get(&label).copied().unwrap_or(1.0);
    let next = match operation {
        ZoomOperation::Reset => 1.0,
        ZoomOperation::In => (current * 1.1).min(3.0),
        ZoomOperation::Out => (current / 1.1).max(0.5),
    };
    if window.set_zoom(next).is_ok() {
        zoom_scales.insert(label, next);
    }
}

enum ZoomOperation {
    Reset,
    In,
    Out,
}

fn request_close_focused_window(app: &AppHandle<Wry>) {
    let Some(window) = focused_webview_window(app) else {
        return;
    };
    if window.label() == "main" {
        let _ = window.emit("app:close-active-workspace-item-request", ());
    } else {
        // `close()` intentionally goes through the child's CloseRequested
        // guard, so unsaved file editors show the discard confirmation.
        let _ = window.close();
    }
}

pub(crate) fn show_window_context_menu(
    app: &AppHandle<Wry>,
    window: &WebviewWindow<Wry>,
    kind: WindowMenuKind,
    x: f64,
    y: f64,
) -> Result<(), AppError> {
    if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
        return Err(AppError::Command("Window menu position is invalid".to_string()));
    }
    let is_english = crate::commands::app_get_ui_preferences(app.clone())
        .map(|preferences| preferences.locale == "enUS")
        .unwrap_or(false);
    let (quit_accelerator, close_accelerator) = application_menu_accelerators(std::env::consts::OS);

    let menu = match kind {
        WindowMenuKind::App => {
            let version = MenuItemBuilder::with_id(
                "app-version",
                format!("Version {}", app.package_info().version),
            )
            .enabled(false)
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            MenuBuilder::new(app)
                .item(&version)
                .build()
                .map_err(|error| AppError::Window(error.to_string()))?
        }
        WindowMenuKind::File => {
            let new_connection = MenuItemBuilder::with_id(
                "new-connection",
                localized(is_english, "New Connection", "新建连接"),
            )
            .accelerator("CmdOrCtrl+N")
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let connection_manager = MenuItemBuilder::with_id(
                "connection-manager",
                localized(is_english, "Connection Manager", "连接管理"),
            )
            .accelerator("CmdOrCtrl+Shift+C")
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let command_manager = MenuItemBuilder::with_id(
                "command-manager",
                localized(is_english, "Command Manager", "命令管理"),
            )
            .accelerator("CmdOrCtrl+Shift+M")
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let logs = MenuItemBuilder::with_id(
                "open-logs-directory",
                localized(is_english, "Open Logs Directory", "打开日志目录"),
            )
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let quit = MenuItemBuilder::with_id("quit", localized(is_english, "Exit", "退出"))
                .accelerator(quit_accelerator)
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;
            MenuBuilder::new(app)
                .item(&new_connection)
                .item(&connection_manager)
                .item(&command_manager)
                .separator()
                .item(&logs)
                .separator()
                .item(&quit)
                .build()
                .map_err(|error| AppError::Window(error.to_string()))?
        }
        WindowMenuKind::View => {
            let reload = MenuItemBuilder::with_id("view-reload", localized(is_english, "Reload", "重新加载"))
                .accelerator("F5")
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;
            let reset_zoom = MenuItemBuilder::with_id(
                "view-reset-zoom",
                localized(is_english, "Actual Size", "实际大小"),
            )
            .accelerator("CmdOrCtrl+0")
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let zoom_in = MenuItemBuilder::with_id("view-zoom-in", localized(is_english, "Zoom In", "放大"))
                .accelerator("CmdOrCtrl+Plus")
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;
            let zoom_out = MenuItemBuilder::with_id("view-zoom-out", localized(is_english, "Zoom Out", "缩小"))
                .accelerator("CmdOrCtrl+-")
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;

            let builder = MenuBuilder::new(app).item(&reload);
            #[cfg(debug_assertions)]
            let builder = {
                let devtools = MenuItemBuilder::with_id(
                    "view-toggle-devtools",
                    localized(is_english, "Toggle Developer Tools", "开发者工具"),
                )
                .accelerator("F12")
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;
                builder.item(&devtools)
            };
            builder
                .separator()
                .item(&reset_zoom)
                .item(&zoom_in)
                .item(&zoom_out)
                .build()
                .map_err(|error| AppError::Window(error.to_string()))?
        }
        WindowMenuKind::Window => {
            let minimize = MenuItemBuilder::with_id(
                "window-minimize",
                localized(is_english, "Minimize", "最小化"),
            )
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let maximize_label = if window.is_maximized().unwrap_or(false) {
                localized(is_english, "Restore", "还原")
            } else {
                localized(is_english, "Maximize", "最大化")
            };
            let maximize = MenuItemBuilder::with_id("window-toggle-maximize", maximize_label)
                .build(app)
                .map_err(|error| AppError::Window(error.to_string()))?;
            let close = MenuItemBuilder::with_id(
                "window-request-close",
                localized(is_english, "Close Window", "关闭窗口"),
            )
            .accelerator(close_accelerator)
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            MenuBuilder::new(app)
                .item(&minimize)
                .item(&maximize)
                .separator()
                .item(&close)
                .build()
                .map_err(|error| AppError::Window(error.to_string()))?
        }
    };
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|error| AppError::Window(error.to_string()))
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
        // Manager windows render their own title bar. Keep the native frame
        // disabled so macOS does not add a second traffic-light row above it.
        "connection-manager" => ("连接管理器", 860.0, 680.0, 760.0, 520.0, false),
        "command-manager" => ("命令管理器", 860.0, 680.0, 760.0, 620.0, false),
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
                if request_file_editor_close(editor_window.app_handle(), &editor_window) {
                    let _ = editor_window.emit("app:file-editor-close-request", ());
                }
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
            app.manage(FileEditorCloseRegistry::default());
            app.manage(WindowMenuState::default());

            let main_window = app.get_webview_window("main")
                .ok_or_else(|| "Failed to find main window".to_string())?;

            // ── Platform-specific window chrome ────────────────────────────
            // macOS: keep decorations + Overlay titleBarStyle so the traffic
            //        lights float over renderer content. Overlay's AppKit
            //        geometry is intentionally calibrated in renderer CSS;
            //        it is not identical to Electron's hiddenInset.
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

            // Native menu building. Keep the shortcuts on the same main-side
            // lifecycle paths as Electron: Cmd+Q / Alt+F4 asks the renderer
            // to confirm application exit, while Cmd/Ctrl+W closes the active
            // workspace item (or a focused child window).
            let (quit_accelerator, close_accelerator) = application_menu_accelerators(std::env::consts::OS);
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
                        .accelerator(quit_accelerator)
                        .build(app)
                        .map_err(|error| error.to_string())?,
                )
                .build()
                .map_err(|error| error.to_string())?;

            let window_minimize_menu = MenuItemBuilder::with_id("window-minimize", "最小化")
                .build(app)
                .map_err(|error| error.to_string())?;
            let window_close_menu = MenuItemBuilder::with_id("window-request-close", "关闭窗口")
                .accelerator(close_accelerator)
                .build(app)
                .map_err(|error| error.to_string())?;
            let window_submenu = SubmenuBuilder::new(app, "窗口")
                .item(&window_minimize_menu)
                .separator()
                .item(&window_close_menu)
                .build()
                .map_err(|error| error.to_string())?;

            let menu = MenuBuilder::new(app)
                .item(&file_submenu)
                .item(&window_submenu)
                .build()
                .map_err(|error| error.to_string())?;
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
            "open-logs-directory" => {
                let _ = crate::commands::app_open_logs_directory(app.clone());
            }
            "view-reload" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.reload();
                }
            }
            "view-reset-zoom" => update_focused_window_zoom(app, ZoomOperation::Reset),
            "view-zoom-in" => update_focused_window_zoom(app, ZoomOperation::In),
            "view-zoom-out" => update_focused_window_zoom(app, ZoomOperation::Out),
            "view-toggle-devtools" => {
                #[cfg(debug_assertions)]
                if let Some(window) = focused_webview_window(app) {
                    if window.is_devtools_open() {
                        window.close_devtools();
                    } else {
                        window.open_devtools();
                    }
                }
            }
            "window-minimize" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.minimize();
                }
            }
            "window-toggle-maximize" => {
                if let Some(window) = focused_webview_window(app) {
                    if window.is_maximized().unwrap_or(false) {
                        let _ = window.unmaximize();
                    } else {
                        let _ = window.maximize();
                    }
                    let _ = app.emit(
                        "app:window-maximized-change",
                        window.is_maximized().unwrap_or(false),
                    );
                }
            }
            "window-request-close" => request_close_focused_window(app),
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
            crate::commands::app_list_ssh_keys,
            crate::commands::app_select_ssh_key_file,
            crate::commands::app_import_ssh_key,
            crate::commands::app_update_ssh_key_note,
            crate::commands::app_delete_ssh_key,
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
            crate::commands::app_cancel_file_editor_close,
            crate::commands::app_show_window_menu,
            
            // Phase 3 commands
            crate::commands::app_open_profile,
            crate::commands::app_activate_tab,
            crate::commands::app_reconnect_tab,
            crate::commands::app_disconnect_tab,
            crate::commands::app_close_tab,
            crate::commands::app_write_terminal,
            crate::commands::app_subscribe_terminal_data,
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
            crate::commands::app_execute_command_template,

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
        .run(|_app_handle, _event| {
            // macOS: clicking the dock icon when the main window is hidden
            // should bring it back (mirrors Electron `activate`).
            // `Reopen` is a macOS-only Tauri event and must not be referenced
            // while compiling the Linux or Windows desktop targets.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                show_main_window(_app_handle);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{application_menu_accelerators, FileEditorCloseRegistry, WindowMenuKind};

    #[test]
    fn keeps_mac_and_non_mac_window_shortcuts_distinct() {
        assert_eq!(application_menu_accelerators("macos"), ("Cmd+Q", "Cmd+W"));
        assert_eq!(application_menu_accelerators("windows"), ("Alt+F4", "Ctrl+W"));
        assert_eq!(application_menu_accelerators("linux"), ("Alt+F4", "Ctrl+W"));
    }

    #[test]
    fn window_menu_kind_accepts_the_public_bridge_values_only() {
        assert_eq!(WindowMenuKind::try_from("app").unwrap(), WindowMenuKind::App);
        assert_eq!(WindowMenuKind::try_from("file").unwrap(), WindowMenuKind::File);
        assert_eq!(WindowMenuKind::try_from("view").unwrap(), WindowMenuKind::View);
        assert_eq!(WindowMenuKind::try_from("window").unwrap(), WindowMenuKind::Window);
        assert!(WindowMenuKind::try_from("developer").is_err());
    }

    #[test]
    fn file_editor_close_registry_deduplicates_and_clears_requests() {
        let registry = FileEditorCloseRegistry::default();
        assert!(registry.request("file-editor-a"));
        assert!(!registry.request("file-editor-a"));
        registry.resolve("file-editor-a");
        assert!(registry.request("file-editor-a"));
    }
}
