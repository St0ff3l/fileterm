pub mod commands;
pub mod services;
pub mod sessions;
pub mod storage;

use crate::commands::OpenWindowInput;
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicBool, atomic::Ordering, Mutex},
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri::image::Image;
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent, Wry,
};
use thiserror::Error;
use tokio::sync::oneshot;
use url::form_urlencoded::Serializer;

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
struct FileEditorCloseState {
    pending_labels: HashSet<String>,
    waiters: HashMap<String, Vec<oneshot::Sender<bool>>>,
}

#[derive(Default)]
pub(crate) struct FileEditorCloseRegistry {
    state: Mutex<FileEditorCloseState>,
}

#[derive(Default)]
pub(crate) struct QuitPreparationRegistry {
    in_progress: AtomicBool,
}

/// Windows hidden together with the main window must be restored together as
/// well. This mirrors Electron's `childWindowsHiddenWithMain` lifecycle and
/// avoids losing standalone managers/editors after a tray hide/show cycle.
#[derive(Default)]
struct HiddenWithMainRegistry {
    labels: Mutex<HashSet<String>>,
}

impl FileEditorCloseRegistry {
    fn request(&self, label: &str) -> bool {
        self.state
            .lock()
            .expect("file editor close registry lock poisoned")
            .pending_labels
            .insert(label.to_string())
    }

    fn request_and_wait(&self, label: &str) -> (bool, oneshot::Receiver<bool>) {
        let (sender, receiver) = oneshot::channel();
        let mut state = self
            .state
            .lock()
            .expect("file editor close registry lock poisoned");
        let should_emit = state.pending_labels.insert(label.to_string());
        state
            .waiters
            .entry(label.to_string())
            .or_default()
            .push(sender);
        (should_emit, receiver)
    }

    fn resolve(&self, label: &str, approved: bool) {
        let waiters = {
            let mut state = self
                .state
                .lock()
                .expect("file editor close registry lock poisoned");
            state.pending_labels.remove(label);
            state.waiters.remove(label).unwrap_or_default()
        };
        for waiter in waiters {
            let _ = waiter.send(approved);
        }
    }
}

impl QuitPreparationRegistry {
    pub(crate) fn try_begin(&self) -> bool {
        self.in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub(crate) fn cancel(&self) {
        self.in_progress.store(false, Ordering::Release);
    }
}

pub(crate) fn request_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) -> bool {
    app.state::<FileEditorCloseRegistry>()
        .request(window.label())
}

pub(crate) fn resolve_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    app.state::<FileEditorCloseRegistry>()
        .resolve(window.label(), true);
}

pub(crate) fn cancel_file_editor_close(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    app.state::<FileEditorCloseRegistry>()
        .resolve(window.label(), false);
}

/// Ask every standalone editor to resolve its dirty state before the app tears
/// down transfers or sessions. A cancel from any editor aborts the whole quit.
pub(crate) async fn request_file_editors_for_quit(app: &AppHandle<Wry>) -> Result<bool, AppError> {
    let mut labels = app
        .webview_windows()
        .into_keys()
        .filter(|label| label.starts_with("file-editor-"))
        .collect::<Vec<_>>();
    labels.sort();

    for label in labels {
        let Some(window) = app.get_webview_window(&label) else {
            continue;
        };
        let (should_emit, resolution) = app
            .state::<FileEditorCloseRegistry>()
            .request_and_wait(&label);
        if should_emit {
            if let Err(error) = window.emit("app:file-editor-close-request", ()) {
                // Do not leave a stale pending label/waiter behind. A later
                // quit request must be able to ask this editor again.
                app.state::<FileEditorCloseRegistry>()
                    .resolve(&label, false);
                return Err(AppError::Window(error.to_string()));
            }
        }
        match resolution.await {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(_) if app.get_webview_window(&label).is_none() => {}
            Err(_) => {
                return Err(AppError::Window(format!(
                    "File editor close request ended without a decision: {label}"
                )))
            }
        }
    }
    Ok(true)
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
            _ => Err(AppError::Command(format!(
                "Unsupported window menu: {value}"
            ))),
        }
    }
}

fn localized<'a>(is_english: bool, english: &'a str, chinese: &'a str) -> &'a str {
    if is_english {
        english
    } else {
        chinese
    }
}

fn tray_menu_labels(is_english: bool) -> [&'static str; 4] {
    [
        localized(is_english, "Show Main Window", "显示主窗口"),
        localized(is_english, "Connection Manager", "连接管理器"),
        localized(is_english, "Command Manager", "命令管理器"),
        localized(is_english, "Quit FileTerm", "退出 FileTerm"),
    ]
}

fn build_tray_menu(app: &AppHandle<Wry>, is_english: bool) -> Result<Menu<Wry>, AppError> {
    let [show_main_label, connection_manager_label, command_manager_label, quit_label] =
        tray_menu_labels(is_english);
    let show_main = MenuItemBuilder::with_id("tray-show-main", show_main_label)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
    let connection_manager =
        MenuItemBuilder::with_id("tray-connection-manager", connection_manager_label)
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
    let command_manager = MenuItemBuilder::with_id("tray-command-manager", command_manager_label)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
    let quit = MenuItemBuilder::with_id("tray-quit", quit_label)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;

    MenuBuilder::new(app)
        .item(&show_main)
        .separator()
        .item(&connection_manager)
        .item(&command_manager)
        .separator()
        .item(&quit)
        .build()
        .map_err(|error| AppError::Window(error.to_string()))
}

pub(crate) fn install_localized_tray_menu(
    app: &AppHandle<Wry>,
    is_english: bool,
) -> Result<(), AppError> {
    let Some(tray) = app.tray_by_id("main") else {
        return Ok(());
    };
    tray.set_menu(Some(build_tray_menu(app, is_english)?))
        .map_err(|error| AppError::Window(error.to_string()))
}

fn build_application_menu(app: &AppHandle<Wry>, is_english: bool) -> Result<Menu<Wry>, AppError> {
    let (quit_accelerator, close_accelerator) = application_menu_accelerators(std::env::consts::OS);
    let new_connection_menu = MenuItemBuilder::with_id(
        "new-connection",
        localized(is_english, "New Connection", "新建连接"),
    )
    .accelerator("CmdOrCtrl+N")
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let connection_manager_menu = MenuItemBuilder::with_id(
        "connection-manager",
        localized(is_english, "Connection Manager", "连接管理器"),
    )
    .accelerator("CmdOrCtrl+Shift+C")
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let command_manager_menu = MenuItemBuilder::with_id(
        "command-manager",
        localized(is_english, "Command Manager", "命令管理器"),
    )
    .accelerator("CmdOrCtrl+Shift+M")
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;

    let file_submenu_builder = SubmenuBuilder::new(app, localized(is_english, "File", "文件"))
        .item(&new_connection_menu)
        .item(&connection_manager_menu)
        .item(&command_manager_menu);
    #[cfg(not(target_os = "macos"))]
    let file_submenu_builder = file_submenu_builder.separator().item(
        &MenuItemBuilder::with_id(
            "quit",
            localized(is_english, "Exit FileTerm", "退出 FileTerm"),
        )
        .accelerator(quit_accelerator)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?,
    );
    let file_submenu = file_submenu_builder
        .build()
        .map_err(|error| AppError::Window(error.to_string()))?;

    // WebKit routes the standard Cmd/Ctrl editing accelerators through native
    // predefined items. Explicit labels make these items follow FileTerm's
    // locale instead of the host process locale.
    let edit_undo = PredefinedMenuItem::undo(app, Some(localized(is_english, "Undo", "撤销")))
        .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_redo = PredefinedMenuItem::redo(app, Some(localized(is_english, "Redo", "重做")))
        .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_cut = PredefinedMenuItem::cut(app, Some(localized(is_english, "Cut", "剪切")))
        .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_copy = PredefinedMenuItem::copy(app, Some(localized(is_english, "Copy", "复制")))
        .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_paste = PredefinedMenuItem::paste(app, Some(localized(is_english, "Paste", "粘贴")))
        .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_select_all =
        PredefinedMenuItem::select_all(app, Some(localized(is_english, "Select All", "全选")))
            .map_err(|error| AppError::Window(error.to_string()))?;
    let edit_submenu = SubmenuBuilder::new(app, localized(is_english, "Edit", "编辑"))
        .item(&edit_undo)
        .item(&edit_redo)
        .separator()
        .item(&edit_cut)
        .item(&edit_copy)
        .item(&edit_paste)
        .separator()
        .item(&edit_select_all)
        .build()
        .map_err(|error| AppError::Window(error.to_string()))?;

    let window_minimize_menu = MenuItemBuilder::with_id(
        "window-minimize",
        localized(is_english, "Minimize", "最小化"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let window_close_menu = MenuItemBuilder::with_id(
        "window-request-close",
        localized(is_english, "Close Window", "关闭窗口"),
    )
    .accelerator(close_accelerator)
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let window_submenu_builder = SubmenuBuilder::new(app, localized(is_english, "Window", "窗口"))
        .item(&window_minimize_menu)
        .separator()
        .item(&window_close_menu);
    #[cfg(target_os = "macos")]
    let window_submenu_builder = window_submenu_builder.separator().item(
        &PredefinedMenuItem::bring_all_to_front(
            app,
            Some(localized(is_english, "Bring All to Front", "全部置于顶层")),
        )
        .map_err(|error| AppError::Window(error.to_string()))?,
    );
    let window_submenu = window_submenu_builder
        .build()
        .map_err(|error| AppError::Window(error.to_string()))?;

    let menu_builder = MenuBuilder::new(app);
    #[cfg(target_os = "macos")]
    let menu_builder = {
        let about = PredefinedMenuItem::about(
            app,
            Some(localized(is_english, "About FileTerm", "关于 FileTerm")),
            None,
        )
        .map_err(|error| AppError::Window(error.to_string()))?;
        let services =
            PredefinedMenuItem::services(app, Some(localized(is_english, "Services", "服务")))
                .map_err(|error| AppError::Window(error.to_string()))?;
        let hide = PredefinedMenuItem::hide(
            app,
            Some(localized(is_english, "Hide FileTerm", "隐藏 FileTerm")),
        )
        .map_err(|error| AppError::Window(error.to_string()))?;
        let hide_others = PredefinedMenuItem::hide_others(
            app,
            Some(localized(is_english, "Hide Others", "隐藏其他")),
        )
        .map_err(|error| AppError::Window(error.to_string()))?;
        let show_all =
            PredefinedMenuItem::show_all(app, Some(localized(is_english, "Show All", "全部显示")))
                .map_err(|error| AppError::Window(error.to_string()))?;
        // Keep quit on FileTerm's confirmation/transfer-cleanup path instead
        // of using the predefined item, which would terminate immediately.
        let quit = MenuItemBuilder::with_id(
            "quit",
            localized(is_english, "Quit FileTerm", "退出 FileTerm"),
        )
        .accelerator(quit_accelerator)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let app_submenu = SubmenuBuilder::new(app, "FileTerm")
            .item(&about)
            .separator()
            .item(&services)
            .separator()
            .item(&hide)
            .item(&hide_others)
            .item(&show_all)
            .separator()
            .item(&quit)
            .build()
            .map_err(|error| AppError::Window(error.to_string()))?;
        menu_builder.item(&app_submenu)
    };
    menu_builder
        .item(&file_submenu)
        .item(&edit_submenu)
        .item(&window_submenu)
        .build()
        .map_err(|error| AppError::Window(error.to_string()))
}

pub(crate) fn install_localized_application_menu(
    app: &AppHandle<Wry>,
    is_english: bool,
) -> Result<(), AppError> {
    let menu = build_application_menu(app, is_english)?;
    app.set_menu(menu)
        .map_err(|error| AppError::Window(error.to_string()))?;
    Ok(())
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

fn tray_icon_should_be_template(platform: &str) -> bool {
    platform == "macos"
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
        return Err(AppError::Command(
            "Window menu position is invalid".to_string(),
        ));
    }
    let is_english = crate::commands::app_get_ui_preferences(app.clone())
        .map(|preferences| preferences.locale == "enUS")
        .unwrap_or(false);
    let (quit_accelerator, close_accelerator) = application_menu_accelerators(std::env::consts::OS);

    let menu = match kind {
        WindowMenuKind::App => {
            let version = MenuItemBuilder::with_id(
                "app-version",
                format!(
                    "{} {}",
                    localized(is_english, "Version", "版本"),
                    app.package_info().version
                ),
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
            let reload = MenuItemBuilder::with_id(
                "view-reload",
                localized(is_english, "Reload", "重新加载"),
            )
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
            let zoom_in =
                MenuItemBuilder::with_id("view-zoom-in", localized(is_english, "Zoom In", "放大"))
                    .accelerator("CmdOrCtrl+Plus")
                    .build(app)
                    .map_err(|error| AppError::Window(error.to_string()))?;
            let zoom_out = MenuItemBuilder::with_id(
                "view-zoom-out",
                localized(is_english, "Zoom Out", "缩小"),
            )
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

fn child_window_should_be_transparent(platform: &str, decorations: bool) -> bool {
    platform == "macos" && !decorations
}

#[cfg(target_os = "windows")]
fn prepare_windows_icon(size: u32, content_size: u32) -> Result<Image<'static>, AppError> {
    use image::{imageops, imageops::FilterType, RgbaImage};

    let source = image::load_from_memory(include_bytes!("../../build/icon.png"))
        .map_err(|error| AppError::Window(error.to_string()))?
        .into_rgba8();
    let (mut min_x, mut min_y) = (source.width(), source.height());
    let (mut max_x, mut max_y) = (0, 0);
    let mut has_visible_pixel = false;

    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel[3] > 8 {
            has_visible_pixel = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if !has_visible_pixel {
        return Err(AppError::Window(
            "Windows icon has no visible pixels".to_string(),
        ));
    }

    let cropped =
        imageops::crop_imm(&source, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1).to_image();
    let scale = content_size as f64 / cropped.width().max(cropped.height()) as f64;
    let resized_width = (cropped.width() as f64 * scale).round().max(1.0) as u32;
    let resized_height = (cropped.height() as f64 * scale).round().max(1.0) as u32;
    let resized = imageops::resize(
        &cropped,
        resized_width,
        resized_height,
        FilterType::Lanczos3,
    );
    let mut canvas = RgbaImage::new(size, size);
    imageops::overlay(
        &mut canvas,
        &resized,
        ((size - resized_width) / 2) as i64,
        ((size - resized_height) / 2) as i64,
    );

    Ok(Image::new_owned(canvas.into_raw(), size, size))
}

#[cfg(target_os = "windows")]
fn windows_app_icon(scale_factor: f64) -> Result<Image<'static>, AppError> {
    // Windows uses a 32px large icon at 100% scaling. Supplying the matching
    // physical size avoids a second taskbar resample at 125%/150%/200% DPI.
    let size = (32.0 * scale_factor).round().clamp(32.0, 128.0) as u32;
    let content_size = (size as f64 * 0.96).round() as u32;
    prepare_windows_icon(size, content_size)
}

#[cfg(target_os = "windows")]
fn windows_tray_icon(scale_factor: f64) -> Result<Image<'static>, AppError> {
    // Electron feeds Tray a 16x16 logical icon on Windows. Tauri accepts raw
    // physical pixels, so account for Windows DPI here to avoid the shell
    // upscaling a fixed 16px bitmap on 125%/150%/200% displays.
    let size = (16.0 * scale_factor).round().clamp(16.0, 64.0) as u32;
    let content_size = (size as f64 * 0.94).round() as u32;
    prepare_windows_icon(size, content_size)
}

#[cfg(target_os = "windows")]
fn prefer_windows_native_rounded_corners(window: &WebviewWindow<Wry>) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::{
        Foundation::HWND,
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
            DWM_WINDOW_CORNER_PREFERENCE,
        },
    };

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as HWND,
        _ => return,
    };

    // SetWindowRgn uses a 1-bit GDI mask, which makes a large custom radius
    // visibly jagged. Let DWM own the outline instead: it is anti-aliased,
    // adapts to DPI, and automatically becomes square while maximized.
    let preference: DWM_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const std::ffi::c_void,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

#[cfg(target_os = "macos")]
fn calibrate_macos_traffic_lights(window: &WebviewWindow<Wry>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSView, NSWindowButton};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const BUTTON_SIZE: f64 = 14.0;
    const BUTTON_SPACING: f64 = 23.0;
    // Keep the packaged Overlay window's vertical compensation native so
    // renderer CSS does not have to know which build flavor launched the app.
    const RELEASE_VERTICAL_OFFSET: f64 = 1.0;

    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };

    // Tauri exposes the AppKit view through raw-window-handle. AppKit objects
    // are main-thread-only, and this function is called from setup on macOS.
    let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
    let Some(ns_window) = view.window() else {
        return;
    };
    let Some(close) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
        return;
    };
    let Some(miniaturize) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
    else {
        return;
    };
    let Some(zoom) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
        return;
    };

    let buttons = [&close, &miniaturize, &zoom];
    let first_frame = buttons[0].frame();
    let vertical_offset = if cfg!(debug_assertions) {
        0.0
    } else {
        RELEASE_VERTICAL_OFFSET
    };
    let center_y = first_frame.origin.y + first_frame.size.height / 2.0 + vertical_offset;
    let center_x = first_frame.origin.x + first_frame.size.width / 2.0;

    for (index, button) in buttons.into_iter().enumerate() {
        let mut frame = button.frame();
        frame.origin.x = center_x + index as f64 * BUTTON_SPACING - BUTTON_SIZE / 2.0;
        frame.origin.y = center_y - BUTTON_SIZE / 2.0;
        frame.size.width = BUTTON_SIZE;
        frame.size.height = BUTTON_SIZE;
        button.setFrame(frame);
    }
}

pub fn open_child_window(app: &AppHandle, input: OpenWindowInput) -> Result<(), AppError> {
    if input.kind == "file-editor"
        && input.source.as_deref() == Some("remote")
        && input.tab_id.as_deref().is_none_or(str::is_empty)
    {
        return Err(AppError::Window(
            "远程文件编辑器缺少会话标识，已阻止打开".to_string(),
        ));
    }

    let label = window_label(&input);
    if let Some(window) = app.get_webview_window(&label) {
        // Match Electron's form lifecycle: opening a form always reloads it
        // with the new mode/id URL. Focusing the existing WebviewWindow keeps
        // its old query string, which made edit requests render the previous
        // create form (or a different profile) instead.
        if matches!(input.kind.as_str(), "connection-form" | "command-form") {
            window
                .destroy()
                .map_err(|error| AppError::Window(error.to_string()))?;
        } else {
            crate::services::logging::debug(
                app,
                "window",
                format!("focus existing label={label} kind={}", input.kind),
            );
            window
                .show()
                .map_err(|error| AppError::Window(error.to_string()))?;
            window
                .set_focus()
                .map_err(|error| AppError::Window(error.to_string()))?;
            return Ok(());
        }
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

    // Frameless macOS windows use a transparent native surface so the
    // renderer's rounded standalone frame can clip the four corners. Keep
    // Windows opaque: WebView2 otherwise exposes the desktop through those
    // corners when the renderer applies its rounded frame.
    let transparent = child_window_should_be_transparent(std::env::consts::OS, decorations);
    let background_color = if transparent {
        Color(0, 0, 0, 0)
    } else {
        Color(21, 21, 21, 255)
    };

    let window = WebviewWindowBuilder::new(app, &label, window_url(&input))
        .title(title)
        .inner_size(width, height)
        .min_inner_size(min_width, min_height)
        .center()
        .decorations(decorations)
        // Match Electron's `show: false` + `ready-to-show` lifecycle. Wry
        // otherwise shows a transparent native frame before React and the
        // theme bootstrap have painted, which flashes twice on Windows.
        .transparent(transparent)
        .background_color(background_color)
        .shadow(true)
        .visible(false)
        .build()
        .map_err(|error| {
            crate::services::logging::error(
                app,
                "window",
                format!(
                    "create failed label={label} kind={} error={error}",
                    input.kind
                ),
            );
            AppError::Window(error.to_string())
        })?;
    #[cfg(target_os = "windows")]
    window
        .set_icon(windows_app_icon(window.scale_factor().unwrap_or(1.0))?)
        .map_err(|error| AppError::Window(error.to_string()))?;
    #[cfg(target_os = "windows")]
    prefer_windows_native_rounded_corners(&window);
    crate::services::logging::info(
        app,
        "window",
        format!("created label={label} kind={}", input.kind),
    );
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

fn open_child_window_from_native_event(app: &AppHandle, input: OpenWindowInput) {
    // Tauri/WebView2 documents the same Windows deadlock for synchronous
    // event handlers as for synchronous commands. Tray and native menu
    // callbacks therefore hand the blocking builder work to a worker thread.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kind = input.kind.clone();
        if let Err(error) = open_child_window(&app, input) {
            crate::services::logging::error(
                &app,
                "window",
                format!("native request failed kind={kind} error={error}"),
            );
        }
    });
}

fn show_main_window(app: &AppHandle<Wry>) {
    let hidden_labels = {
        let state = app.state::<HiddenWithMainRegistry>();
        let mut labels = state
            .labels
            .lock()
            .expect("hidden window registry lock poisoned");
        labels.drain().collect::<Vec<_>>()
    };

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }

    for label in hidden_labels {
        if label != "main" {
            if let Some(window) = app.get_webview_window(&label) {
                let _ = window.show();
            }
        }
    }
}

pub(crate) fn hide_main_window_and_children(app: &AppHandle<Wry>) {
    let mut hidden_labels = HashSet::new();
    for (label, window) in app.webview_windows() {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            hidden_labels.insert(label);
        }
    }

    let state = app.state::<HiddenWithMainRegistry>();
    *state
        .labels
        .lock()
        .expect("hidden window registry lock poisoned") = hidden_labels;
}

fn toggle_main_window_visibility(app: &AppHandle<Wry>) {
    let should_hide = app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false)
    });
    if should_hide {
        hide_main_window_and_children(app);
    } else {
        show_main_window(app);
    }
}

pub(crate) fn request_main_window_close(app: &AppHandle<Wry>, is_quit: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit(
            "app:window-close-request",
            serde_json::json!({ "isQuit": is_quit }),
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Windows packages use Tauri's signed updater. macOS deliberately keeps
    // the Release-page flow so users choose the GitHub download themselves.
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|app| {
            crate::storage::migrate_legacy_data_once(app.handle())?;
            crate::services::logging::init(app.handle());
            crate::services::logging::info(
                app.handle(),
                "app",
                format!(
                    "startup version={} platform={} arch={}",
                    app.package_info().version,
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
            );
            app.manage(crate::services::WorkspaceState::default());
            app.manage(FileEditorCloseRegistry::default());
            app.manage(QuitPreparationRegistry::default());
            app.manage(HiddenWithMainRegistry::default());
            app.manage(WindowMenuState::default());

            let main_window = app
                .get_webview_window("main")
                .ok_or_else(|| "Failed to find main window".to_string())?;

            // ── Platform-specific window chrome ────────────────────────────
            // macOS: keep decorations + Overlay titleBarStyle so the traffic
            //        lights float over renderer content. AppKit button frames
            //        are calibrated below so dev and packaged builds match.
            // Windows: drop the OS frame so the renderer owns the title bar.
            // Linux: keep native decorations.
            #[cfg(target_os = "macos")]
            calibrate_macos_traffic_lights(&main_window);

            #[cfg(target_os = "windows")]
            {
                let _ = main_window.set_decorations(false);
                prefer_windows_native_rounded_corners(&main_window);
                main_window
                    .set_icon(
                        windows_app_icon(main_window.scale_factor().unwrap_or(1.0))
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
            }

            let app_handle = app.handle().clone();
            main_window.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    crate::services::logging::info(&app_handle, "window", "main close requested");
                    api.prevent_close();
                    request_main_window_close(&app_handle, false);
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
            });

            // Native menu building. Keep shortcuts on the same main-side
            // lifecycle paths as Electron and build labels from persisted UI
            // preferences so the native chrome matches the renderer locale.
            let is_english = crate::commands::app_get_ui_preferences(app.handle().clone())
                .map(|preferences| preferences.locale == "enUS")
                .unwrap_or(false);
            install_localized_application_menu(app.handle(), is_english)
                .map_err(|error| error.to_string())?;

            // Tray labels use the same persisted locale as the application
            // menu and are rebuilt when preferences change.
            let tray_menu =
                build_tray_menu(app.handle(), is_english).map_err(|error| error.to_string())?;

            #[cfg(target_os = "macos")]
            // tray-icon renders the source at 18 logical points on macOS.
            // Feed it the 36px Retina representation so the status item has
            // one physical source pixel per output pixel on @2x displays.
            let tray_icon = Image::from_bytes(include_bytes!("../../build/trayTemplate@2x.png"))
                .map_err(|error| error.to_string())?;
            #[cfg(target_os = "windows")]
            let tray_icon = windows_tray_icon(main_window.scale_factor().unwrap_or(1.0))
                .map_err(|error| error.to_string())?;
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| "Failed to load the default tray icon".to_string())?;

            TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(tray_icon_should_be_template(std::env::consts::OS))
                .tooltip("FileTerm")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "tray-connection-manager" => {
                        open_child_window_from_native_event(
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
                        open_child_window_from_native_event(
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
                    "tray-quit" => request_main_window_close(app, true),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        toggle_main_window_visibility(tray.app_handle());
                    }
                })
                .build(app)
                .map_err(|error| error.to_string())?;

            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            "new-connection" => {
                open_child_window_from_native_event(
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
                open_child_window_from_native_event(
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
                open_child_window_from_native_event(
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
            "view-toggle-devtools" =>
            {
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
            "quit" => request_main_window_close(app, true),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            crate::commands::app_get_platform,
            crate::commands::app_get_arch,
            crate::commands::app_get_runtime_version,
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
    use super::{
        application_menu_accelerators, child_window_should_be_transparent,
        tray_icon_should_be_template, tray_menu_labels, FileEditorCloseRegistry,
        QuitPreparationRegistry, WindowMenuKind,
    };

    #[cfg(target_os = "windows")]
    use super::{windows_app_icon, windows_tray_icon};

    #[test]
    fn keeps_mac_and_non_mac_window_shortcuts_distinct() {
        assert_eq!(application_menu_accelerators("macos"), ("Cmd+Q", "Cmd+W"));
        assert_eq!(
            application_menu_accelerators("windows"),
            ("Alt+F4", "Ctrl+W")
        );
        assert_eq!(application_menu_accelerators("linux"), ("Alt+F4", "Ctrl+W"));
    }

    #[test]
    fn uses_template_tray_icons_on_macos_only() {
        assert!(tray_icon_should_be_template("macos"));
        assert!(!tray_icon_should_be_template("windows"));
        assert!(!tray_icon_should_be_template("linux"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn prepares_windows_icons_like_the_electron_runtime() {
        let app_icon = windows_app_icon(1.5).unwrap();
        let tray_icon = windows_tray_icon(1.5).unwrap();

        assert_eq!((app_icon.width(), app_icon.height()), (48, 48));
        assert_eq!((tray_icon.width(), tray_icon.height()), (24, 24));
    }

    #[test]
    fn localizes_every_tray_menu_entry() {
        assert_eq!(
            tray_menu_labels(false),
            ["显示主窗口", "连接管理器", "命令管理器", "退出 FileTerm"]
        );
        assert_eq!(
            tray_menu_labels(true),
            [
                "Show Main Window",
                "Connection Manager",
                "Command Manager",
                "Quit FileTerm"
            ]
        );
    }

    #[test]
    fn only_frameless_macos_child_windows_use_transparency() {
        assert!(child_window_should_be_transparent("macos", false));
        assert!(!child_window_should_be_transparent("macos", true));
        assert!(!child_window_should_be_transparent("windows", false));
        assert!(!child_window_should_be_transparent("windows", true));
        assert!(!child_window_should_be_transparent("linux", false));
    }

    #[test]
    fn window_menu_kind_accepts_the_public_bridge_values_only() {
        assert_eq!(
            WindowMenuKind::try_from("app").unwrap(),
            WindowMenuKind::App
        );
        assert_eq!(
            WindowMenuKind::try_from("file").unwrap(),
            WindowMenuKind::File
        );
        assert_eq!(
            WindowMenuKind::try_from("view").unwrap(),
            WindowMenuKind::View
        );
        assert_eq!(
            WindowMenuKind::try_from("window").unwrap(),
            WindowMenuKind::Window
        );
        assert!(WindowMenuKind::try_from("developer").is_err());
    }

    #[test]
    fn file_editor_close_registry_deduplicates_and_clears_requests() {
        let registry = FileEditorCloseRegistry::default();
        assert!(registry.request("file-editor-a"));
        assert!(!registry.request("file-editor-a"));
        registry.resolve("file-editor-a", true);
        assert!(registry.request("file-editor-a"));
    }

    #[tokio::test]
    async fn file_editor_close_registry_notifies_all_quit_waiters() {
        let registry = FileEditorCloseRegistry::default();
        let (should_emit, first) = registry.request_and_wait("file-editor-a");
        let (should_emit_again, second) = registry.request_and_wait("file-editor-a");
        assert!(should_emit);
        assert!(!should_emit_again);

        registry.resolve("file-editor-a", false);
        assert!(!first.await.unwrap());
        assert!(!second.await.unwrap());
        assert!(registry.request("file-editor-a"));
    }

    #[test]
    fn quit_preparation_registry_prevents_duplicate_runs_and_can_reset() {
        let registry = QuitPreparationRegistry::default();
        assert!(registry.try_begin());
        assert!(!registry.try_begin());
        registry.cancel();
        assert!(registry.try_begin());
    }
}
