pub mod commands;
pub mod services;
pub mod sessions;
pub mod storage;

pub fn run_mcp_stdio(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_stdio(arguments)
}

pub fn run_cli(arguments: &[String]) -> Result<(), String> {
    crate::services::mcp::run_cli(arguments)
}

/// Returns whether the first process argument belongs to the non-GUI CLI.
///
/// Keep this dispatch list in the library so the binary entrypoint and its
/// tests share the same contract. CLI commands must be recognized before
/// `run()` starts Tauri.
pub fn is_cli_command(argument: Option<&str>) -> bool {
    matches!(
        argument,
        Some(
            "cli"
                | "connections"
                | "sessions"
                | "directory"
                | "ls"
                | "read"
                | "cat"
                | "commands"
                | "command-templates"
                | "transfers"
                | "wait-transfer"
                | "tunnels"
                | "open"
                | "activate"
                | "reconnect"
                | "disconnect"
                | "close"
                | "exec"
                | "execute"
                | "command-template"
                | "write"
                | "mkdir"
                | "touch"
                | "copy"
                | "move"
                | "rename"
                | "delete"
                | "chmod"
                | "access"
                | "upload"
                | "download"
                | "download-directory"
                | "pause-transfer"
                | "resume-transfer"
                | "discard-transfer"
                | "cancel-transfer"
                | "clear-transfers"
                | "create-tunnel"
                | "start-tunnel"
                | "stop-tunnel"
                | "delete-tunnel"
                | "call"
                | "help"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
        )
    )
}

use crate::commands::OpenWindowInput;
#[cfg(target_os = "linux")]
use gtk::prelude::GtkWindowExt;
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU64;
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicBool, atomic::Ordering, Mutex},
};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tauri::image::Image;
#[cfg(not(target_os = "linux"))]
use tauri::menu::{CheckMenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    window::Color,
    AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent, Wry,
};
use thiserror::Error;
use tokio::sync::oneshot;
use url::form_urlencoded::Serializer;
#[cfg(target_os = "windows")]
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{ICoreWebView2Settings3, ICoreWebView2Settings5},
    ZoomFactorChangedEventHandler,
};
#[cfg(target_os = "windows")]
use windows::core::Interface;

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

#[cfg(target_os = "macos")]
static MACOS_TRAFFIC_LIGHTS_CALIBRATED: AtomicBool = AtomicBool::new(false);

/// A fullscreen transition emits several resize notifications while AppKit is
/// still rebuilding the title-bar hierarchy. Only the final notification may
/// position the traffic lights, otherwise they retain an obsolete y-coordinate
/// when the window returns from fullscreen.
#[cfg(target_os = "macos")]
static MACOS_TRAFFIC_LIGHT_RECALIBRATION_GENERATION: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
const MACOS_RENDERER_TITLEBAR_HEIGHT: f64 = 48.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_FRAME_SIZE: f64 = 14.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_DRAWN_SIZE: f64 = 12.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_LEFT_INSET: f64 = 20.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_CENTER_SPACING: f64 = 23.0;
#[cfg(target_os = "macos")]
const MACOS_TRAFFIC_LIGHT_RECALIBRATION_DELAY_MS: u64 = 140;

#[cfg(target_os = "macos")]
fn macos_traffic_light_target_center(window_height: f64, index: usize) -> (f64, f64) {
    (
        MACOS_TRAFFIC_LIGHT_LEFT_INSET
            + MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0
            + index as f64 * MACOS_TRAFFIC_LIGHT_CENTER_SPACING,
        window_height - MACOS_RENDERER_TITLEBAR_HEIGHT / 2.0,
    )
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayMenuAction {
    ShowMain,
    OpenConnectionManager,
    OpenCommandManager,
    RequestQuit,
}

fn tray_menu_action(id: &str) -> Option<TrayMenuAction> {
    match id {
        "tray-show-main" => Some(TrayMenuAction::ShowMain),
        "tray-connection-manager" => Some(TrayMenuAction::OpenConnectionManager),
        "tray-command-manager" => Some(TrayMenuAction::OpenCommandManager),
        "tray-quit" => Some(TrayMenuAction::RequestQuit),
        _ => None,
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

#[cfg(not(target_os = "linux"))]
fn build_application_menu(app: &AppHandle<Wry>, is_english: bool) -> Result<Menu<Wry>, AppError> {
    let platform = std::env::consts::OS;
    let quit_accelerator = application_quit_accelerator(platform);
    let terminal_zoom_locked = crate::commands::app_get_ui_preferences(app.clone())
        .map(|preferences| preferences.terminal_zoom_locked)
        .unwrap_or(false);
    let new_connection_menu = MenuItemBuilder::with_id(
        "new-connection",
        localized(is_english, "New Connection", "新建连接"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let new_tab_menu = MenuItemBuilder::with_id(
        "workspace-new-tab",
        localized(is_english, "New Tab", "新建标签页"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let connection_manager_menu = MenuItemBuilder::with_id(
        "connection-manager",
        localized(is_english, "Connection Manager", "连接管理器"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let command_manager_menu = MenuItemBuilder::with_id(
        "command-manager",
        localized(is_english, "Command Manager", "命令管理器"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;

    let file_submenu_builder = SubmenuBuilder::new(app, localized(is_english, "File", "文件"))
        .item(&new_tab_menu)
        .separator()
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
    );
    // Cmd+W is macOS's standard close-window affordance. Windows/Linux retain
    // only Alt+F4 for exit, with no competing menu accelerator here.
    #[cfg(target_os = "macos")]
    let window_close_menu = window_close_menu.accelerator("Cmd+W");
    let window_close_menu = window_close_menu
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

    let view_split_vertical = MenuItemBuilder::with_id(
        "view-split-vertical",
        localized(is_english, "Split Vertically", "垂直分屏"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let view_split_horizontal = MenuItemBuilder::with_id(
        "view-split-horizontal",
        localized(is_english, "Split Horizontally", "水平分屏"),
    )
    .build(app)
    .map_err(|error| AppError::Window(error.to_string()))?;
    let view_submenu_builder = SubmenuBuilder::new(app, localized(is_english, "View", "视图"))
        .item(&view_split_vertical)
        .item(&view_split_horizontal);
    // The macOS native menubar owns its displayed shortcuts. These dispatch
    // terminal-only zoom requests back through the Tauri bridge instead of
    // applying an application/WebView zoom level.
    #[cfg(target_os = "macos")]
    let view_submenu_builder = {
        let terminal_zoom_in = MenuItemBuilder::with_id(
            "view-terminal-zoom-in",
            localized(is_english, "Zoom Terminal In", "终端放大"),
        )
        // `+` is Shift+Equal on the physical keyboard. The native accelerator
        // uses the logical Equal key so macOS presents and accepts standard
        // Cmd+ without asking for an additional Shift modifier.
        .accelerator("Cmd+Equal")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_out = MenuItemBuilder::with_id(
            "view-terminal-zoom-out",
            localized(is_english, "Zoom Terminal Out", "终端缩小"),
        )
        .accelerator("Cmd+Minus")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_reset = MenuItemBuilder::with_id(
            "view-terminal-zoom-reset",
            localized(is_english, "Reset Terminal Zoom", "终端实际大小"),
        )
        .accelerator("Cmd+0")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_lock = CheckMenuItemBuilder::with_id(
            "view-terminal-zoom-lock",
            localized(is_english, "Lock Terminal Zoom", "锁定终端缩放"),
        )
        .checked(terminal_zoom_locked)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        view_submenu_builder
            .separator()
            .item(&terminal_zoom_in)
            .item(&terminal_zoom_out)
            .item(&terminal_zoom_reset)
            .item(&terminal_zoom_lock)
    };
    // Windows/Linux use a renderer-owned menubar, but native accelerators are
    // still the only path that reaches us before WebView2/WebKitGTK consumes a
    // browser-style zoom shortcut. They emit the same terminal-only request as
    // the macOS menu and never alter the WebView zoom level.
    #[cfg(not(target_os = "macos"))]
    let view_submenu_builder = {
        let terminal_zoom_in = MenuItemBuilder::with_id(
            "view-terminal-zoom-in",
            localized(is_english, "Zoom Terminal In", "终端放大"),
        )
        .accelerator("Ctrl+Shift+Equal")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_out = MenuItemBuilder::with_id(
            "view-terminal-zoom-out",
            localized(is_english, "Zoom Terminal Out", "终端缩小"),
        )
        .accelerator("Ctrl+Shift+Minus")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_reset = MenuItemBuilder::with_id(
            "view-terminal-zoom-reset",
            localized(is_english, "Reset Terminal Zoom", "终端实际大小"),
        )
        .accelerator("Ctrl+0")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        let terminal_zoom_lock = CheckMenuItemBuilder::with_id(
            "view-terminal-zoom-lock",
            localized(is_english, "Lock Terminal Zoom", "锁定终端缩放"),
        )
        .checked(terminal_zoom_locked)
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        view_submenu_builder
            .separator()
            .item(&terminal_zoom_in)
            .item(&terminal_zoom_out)
            .item(&terminal_zoom_reset)
            .item(&terminal_zoom_lock)
    };
    // macOS uses this native menu instead of the renderer-owned Windows/Linux
    // menu bar, so expose the requested debug-only F12 entry here.
    #[cfg(all(debug_assertions, target_os = "macos"))]
    let view_submenu_builder = {
        let devtools = MenuItemBuilder::with_id(
            "view-toggle-devtools",
            localized(is_english, "Toggle Developer Tools", "开发者工具"),
        )
        .accelerator("F12")
        .build(app)
        .map_err(|error| AppError::Window(error.to_string()))?;
        view_submenu_builder.separator().item(&devtools)
    };
    let view_submenu = view_submenu_builder
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
        .item(&view_submenu)
        .item(&window_submenu)
        .build()
        .map_err(|error| AppError::Window(error.to_string()))
}

pub(crate) fn install_localized_application_menu(
    app: &AppHandle<Wry>,
    is_english: bool,
) -> Result<(), AppError> {
    // Linux uses the renderer-owned menu bar to match the Windows shell and
    // keep it in sync with FileTerm themes. An app-wide GTK menu is attached
    // to every standalone window, which creates an unwanted second menu row
    // in connection, command and file-editor windows.
    #[cfg(target_os = "linux")]
    {
        let _ = (app, is_english);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let menu = build_application_menu(app, is_english)?;
        app.set_menu(menu)
            .map_err(|error| AppError::Window(error.to_string()))?;
        Ok(())
    }
}

/// Preserve the standard app-quit accelerator on macOS and Alt+F4 elsewhere.
/// All other window-menu accelerators are intentionally left unbound.
fn application_quit_accelerator(platform: &str) -> &'static str {
    if platform == "macos" {
        "Cmd+Q"
    } else {
        "Alt+F4"
    }
}

fn tray_icon_should_be_template(platform: &str) -> bool {
    platform == "macos"
}

pub(crate) fn focused_webview_window(app: &AppHandle<Wry>) -> Option<WebviewWindow<Wry>> {
    app.webview_windows()
        .into_values()
        .find(|window| window.is_focused().unwrap_or(false))
        .or_else(|| app.get_webview_window("main"))
}

pub(crate) fn request_close_focused_window(app: &AppHandle<Wry>) {
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
    let quit_accelerator = application_quit_accelerator(std::env::consts::OS);

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
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let connection_manager = MenuItemBuilder::with_id(
                "connection-manager",
                localized(is_english, "Connection Manager", "连接管理"),
            )
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;
            let command_manager = MenuItemBuilder::with_id(
                "command-manager",
                localized(is_english, "Command Manager", "命令管理"),
            )
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
            .build(app)
            .map_err(|error| AppError::Window(error.to_string()))?;

            let builder = MenuBuilder::new(app).item(&reload);
            #[cfg(all(debug_assertions, target_os = "macos"))]
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
            .accelerator(if cfg!(target_os = "macos") {
                "Cmd+W"
            } else {
                "Alt+F4"
            })
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
    if let Some(value) = &input.command {
        serializer.append_pair("command", value);
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
    matches!(platform, "macos" | "linux") && !decorations
}

/// Calculates a child window position centered over the main window's native
/// frame. Tauri's builder-level `center()` centers on the current monitor, so
/// it is not enough for the standalone windows that should follow the main
/// FileTerm window instead.
fn center_child_window_position(
    main_position: PhysicalPosition<i32>,
    main_size: PhysicalSize<u32>,
    child_size: PhysicalSize<u32>,
    work_area: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>,
) -> PhysicalPosition<i32> {
    let desired_x =
        i64::from(main_position.x) + (i64::from(main_size.width) - i64::from(child_size.width)) / 2;
    let desired_y = i64::from(main_position.y)
        + (i64::from(main_size.height) - i64::from(child_size.height)) / 2;

    let clamp_to_work_area = |desired: i64, start: i32, available: u32, child: u32| {
        let min = i64::from(start);
        let max = min + i64::from(available) - i64::from(child);
        if max < min {
            min
        } else {
            desired.clamp(min, max)
        }
    };

    let (x, y) = if let Some((work_area_position, work_area_size)) = work_area {
        (
            clamp_to_work_area(
                desired_x,
                work_area_position.x,
                work_area_size.width,
                child_size.width,
            ),
            clamp_to_work_area(
                desired_y,
                work_area_position.y,
                work_area_size.height,
                child_size.height,
            ),
        )
    } else {
        (desired_x, desired_y)
    };

    PhysicalPosition::new(
        x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
    )
}

/// Repositions an existing or newly-created standalone window over the main
/// window. Failures are deliberately non-fatal: the builder-level monitor
/// centering remains the safe fallback when a desktop backend cannot expose
/// native bounds (for example, some Wayland compositors).
fn center_child_window_on_main(app: &AppHandle<Wry>, child: &WebviewWindow<Wry>) {
    let label = child.label();
    let Some(main) = app.get_webview_window("main") else {
        crate::services::logging::warn(
            app,
            "window",
            format!("center skipped label={label}: main window unavailable"),
        );
        return;
    };

    let main_position = match main.outer_position() {
        Ok(position) => position,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "window",
                format!("center skipped label={label}: main position unavailable: {error}"),
            );
            return;
        }
    };
    let main_size = match main.outer_size() {
        Ok(size) => size,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "window",
                format!("center skipped label={label}: main size unavailable: {error}"),
            );
            return;
        }
    };
    let child_size = match child.outer_size() {
        Ok(size) => size,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "window",
                format!("center skipped label={label}: child size unavailable: {error}"),
            );
            return;
        }
    };
    let work_area = match main.current_monitor() {
        Ok(Some(monitor)) => {
            let area = monitor.work_area();
            Some((area.position, area.size))
        }
        Ok(None) => None,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "window",
                format!("work area unavailable label={label}: {error}"),
            );
            None
        }
    };

    let position = center_child_window_position(main_position, main_size, child_size, work_area);
    if let Err(error) = child.set_position(position) {
        crate::services::logging::warn(
            app,
            "window",
            format!("center failed label={label} position={position:?}: {error}"),
        );
    }
}

#[cfg(target_os = "windows")]
fn windows_icon_image() -> Result<Image<'static>, AppError> {
    // Keep the Windows runtime on the same source as Electron: the original
    // high-resolution FileTerm icon, packaged as a multi-size ICO. Tauri's
    // icon generator places the 32px frame first, which is the frame used by
    // its dev-time context loader. The previous ICO had a 16px first frame,
    // so npm dev loaded that bitmap and Windows enlarged it for the taskbar.
    Image::from_bytes(include_bytes!("../../build/icon.ico"))
        .map_err(|error| AppError::Window(error.to_string()))
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

/// WebView2 consumes precision-touchpad pinch gestures before they become DOM
/// wheel events. Let WebView2 recognize the native gesture, immediately reset
/// its page zoom, and relay only the direction back to the focused xterm pane.
/// This keeps application/WebView zoom fixed at 100% while giving touchpad
/// pinch the same terminal-only semantics as Ctrl+wheel.
#[cfg(target_os = "windows")]
fn install_windows_terminal_zoom_interceptor(window: &WebviewWindow<Wry>) {
    let event_window = window.clone();
    let install_result = window.with_webview(move |webview| {
        let controller = webview.controller();
        let callback_controller = controller.clone();
        let callback_window = event_window.clone();

        unsafe {
            let Ok(core_webview) = controller.CoreWebView2() else {
                return;
            };
            let Ok(settings) = core_webview.Settings() else {
                return;
            };

            // WebView2 otherwise swallows Ctrl+wheel and precision-touchpad
            // pinch before the renderer can observe either input. The
            // ZoomFactorChanged handler below converts them to terminal events
            // and restores the page to 100% immediately.
            let _ = settings.SetIsZoomControlEnabled(true);
            if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
                // Keep Ctrl+0 available to the renderer/native menu for
                // terminal reset instead of allowing WebView2 to consume it.
                let _ = settings3.SetAreBrowserAcceleratorKeysEnabled(false);
            }
            if let Ok(settings5) = settings.cast::<ICoreWebView2Settings5>() {
                let _ = settings5.SetIsPinchZoomEnabled(true);
            }

            let callback =
                ZoomFactorChangedEventHandler::create(Box::new(move |_sender, _args| {
                    let mut zoom_factor = 1.0;
                    callback_controller.ZoomFactor(&mut zoom_factor)?;
                    if (zoom_factor - 1.0).abs() < f64::EPSILON {
                        return Ok(());
                    }

                    // Reset before emitting: SetZoomFactor emits its own
                    // ZoomFactorChanged event, which is ignored at exactly 1.0.
                    callback_controller.SetZoomFactor(1.0)?;
                    let operation = if zoom_factor > 1.0 { "in" } else { "out" };
                    // The renderer only applies this gesture event while the pointer is over a
                    // terminal. Menu shortcuts continue to use app:terminal-zoom-request.
                    let _ = callback_window.emit("app:terminal-gesture-zoom-request", operation);
                    Ok(())
                }));
            let mut token = 0;
            let _ = controller.add_ZoomFactorChanged(&callback, &mut token);
        }
    });

    if let Err(error) = install_result {
        crate::services::logging::warn(
            window.app_handle(),
            "window",
            format!("failed to install WebView2 terminal zoom interceptor: {error}"),
        );
    }
}

#[cfg(target_os = "macos")]
fn apply_macos_main_window_vibrancy(window: &WebviewWindow<Wry>) -> Result<(), String> {
    window_vibrancy::apply_vibrancy(
        window,
        window_vibrancy::NSVisualEffectMaterial::UnderWindowBackground,
        // Keep AppKit's blur state deterministic. The renderer adds the
        // measured 32/60 color overlay; AppKit supplies only the stable blur
        // behind it.
        Some(window_vibrancy::NSVisualEffectState::Active),
        None,
    )
    .map_err(|error| error.to_string())?;

    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAppearance, NSAppearanceCustomization, NSView};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Ok(());
    };
    if MainThreadMarker::new().is_none() {
        return Ok(());
    }

    let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
    // Force a dark AppKit appearance for the native material. Without this,
    // the material can resolve against the system light appearance while the
    // renderer is using the Codex dark theme, which is the source of the
    // intermittent gray/over-transparent result during focus and movement.
    if let Some(native_window) = view.window() {
        let dark_appearance =
            unsafe { NSAppearance::appearanceNamed(objc2_app_kit::NSAppearanceNameDarkAqua) };
        if let Some(dark_appearance) = dark_appearance {
            native_window.setAppearance(Some(&dark_appearance));
        }
    }

    // window-vibrancy uses this stable tag for the view it inserts beneath
    // the WebView. Leave it visible so AppKit owns blur consistently; the
    // renderer's semi-transparent surface overlay keeps the resulting color
    // deterministic.
    if let Some(effect_view) = view.viewWithTag(91_376_254) {
        effect_view.setHidden(false);
        effect_view.setAlphaValue(1.0);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn calibrate_macos_traffic_lights(window: &WebviewWindow<Wry>) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSControlSize, NSView, NSWindowButton};
    use objc2_quartz_core::{CATransaction, CATransform3D};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    let Some(_main_thread) = MainThreadMarker::new() else {
        return false;
    };

    // Tauri exposes the AppKit view through raw-window-handle. AppKit objects
    // are main-thread-only, and this runs after the first page load on macOS.
    let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
    let Some(ns_window) = view.window() else {
        return false;
    };
    let Some(close) = ns_window.standardWindowButton(NSWindowButton::CloseButton) else {
        return false;
    };
    let Some(miniaturize) = ns_window.standardWindowButton(NSWindowButton::MiniaturizeButton)
    else {
        return false;
    };
    let Some(zoom) = ns_window.standardWindowButton(NSWindowButton::ZoomButton) else {
        return false;
    };
    // Standard window buttons are retained above and remain attached to the
    // NSWindow title-bar hierarchy for the duration of this main-thread call.
    let Some(close_superview) = (unsafe { close.superview() }) else {
        return false;
    };
    let Some(miniaturize_superview) = (unsafe { miniaturize.superview() }) else {
        return false;
    };
    let Some(zoom_superview) = (unsafe { zoom.superview() }) else {
        return false;
    };

    let buttons = [
        (&close, &close_superview),
        (&miniaturize, &miniaturize_superview),
        (&zoom, &zoom_superview),
    ];
    let window_height = ns_window.frame().size.height;
    let content_scale = MACOS_TRAFFIC_LIGHT_FRAME_SIZE / MACOS_TRAFFIC_LIGHT_DRAWN_SIZE;

    CATransaction::begin();
    CATransaction::setDisableActions(true);
    for (index, (button, button_superview)) in buttons.into_iter().enumerate() {
        // The design target is expressed in window coordinates, independent
        // of AppKit's Debug/Release title-bar container geometry. Convert that
        // absolute center into each native button's own superview before
        // assigning its frame.
        button.setControlSize(NSControlSize::Regular);
        button.sizeToFit();

        let (target_center_x, target_center_y) =
            macos_traffic_light_target_center(window_height, index);
        let mut target_center_in_window = button.frame().origin;
        target_center_in_window.x = target_center_x;
        target_center_in_window.y = target_center_y;
        let target_center = button_superview.convertPoint_fromView(target_center_in_window, None);

        let mut frame = button.frame();
        frame.origin.x = target_center.x - MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0;
        frame.origin.y = target_center.y - MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0;
        frame.size.width = MACOS_TRAFFIC_LIGHT_FRAME_SIZE;
        frame.size.height = MACOS_TRAFFIC_LIGHT_FRAME_SIZE;
        button.setFrame(frame);
        button.setWantsLayer(true);
        if let Some(layer) = button.layer() {
            let mut transform = CATransform3D::new_scale(content_scale, content_scale, 1.0);
            let centered_translation = MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0 * (1.0 - content_scale);
            transform.m41 = centered_translation;
            transform.m42 = centered_translation;
            layer.setTransform(transform);
        }
        NSView::setNeedsDisplay(button, true);
    }
    CATransaction::commit();

    true
}

#[cfg(target_os = "macos")]
fn schedule_macos_traffic_light_recalibration(app: &AppHandle<Wry>) {
    let generation =
        MACOS_TRAFFIC_LIGHT_RECALIBRATION_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(
            MACOS_TRAFFIC_LIGHT_RECALIBRATION_DELAY_MS,
        ))
        .await;
        if MACOS_TRAFFIC_LIGHT_RECALIBRATION_GENERATION.load(Ordering::Acquire) != generation {
            return;
        }

        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let calibration_window = window.clone();
        let _ = window.run_on_main_thread(move || {
            let _ = calibrate_macos_traffic_lights(&calibration_window);
        });
    });
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
            center_child_window_on_main(app, &window);
            restore_window(app, &window, true);
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

    // Frameless macOS and Linux windows use a transparent native surface so
    // the renderer's rounded standalone frame can clip all four corners.
    // Keep Windows opaque: WebView2 otherwise exposes the desktop through
    // those corners when the renderer applies its rounded frame.
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
    center_child_window_on_main(app, &window);
    #[cfg(target_os = "windows")]
    window
        .set_icon(windows_icon_image()?)
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

/// Restores a window from either the hidden or minimized state. `show()` alone
/// does not reliably deminiaturize a GTK window, which leaves renderer-owned
/// confirmation dialogs invisible after a tray action on Linux.
#[cfg(target_os = "linux")]
fn restore_linux_gtk_window(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>, focus: bool) {
    let label = window.label();
    match window.gtk_window() {
        Ok(gtk_window) => {
            // `present()` is only an activation request. Under GNOME/Wayland
            // it can be ignored for an iconified window when a tray menu does
            // not provide an activation token, so undo iconification first.
            // This is a native minimize restore, not a hide-to-tray path: the
            // application remains represented in the Dock throughout.
            gtk_window.deiconify();
            if focus {
                gtk_window.present_with_time(gtk::gdk::ffi::GDK_CURRENT_TIME as u32);
            }
        }
        Err(error) => crate::services::logging::warn(
            app,
            "window",
            format!("native GTK restore failed label={label}: {error}"),
        ),
    }
}

fn restore_window_on_main_thread(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>, focus: bool) {
    let label = window.label();
    #[cfg(target_os = "linux")]
    restore_linux_gtk_window(app, window, focus);

    if let Err(error) = window.unminimize() {
        crate::services::logging::warn(
            app,
            "window",
            format!("unminimize failed label={label}: {error}"),
        );
    }
    if let Err(error) = window.show() {
        crate::services::logging::warn(
            app,
            "window",
            format!("show failed label={label}: {error}"),
        );
        return;
    }
    if focus {
        if let Err(error) = window.set_focus() {
            crate::services::logging::warn(
                app,
                "window",
                format!("focus failed label={label}: {error}"),
            );
        }
    }
}

fn restore_window(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>, focus: bool) {
    let label = window.label().to_string();
    let restore_app = app.clone();
    let restore_window = window.clone();
    // GTK activation must happen on its main loop. Tray menu callbacks can
    // arrive on another thread; restoring there races with a minimized window
    // and leaves it hidden until the desktop shell activates it from the dock.
    if let Err(error) = window.run_on_main_thread(move || {
        restore_window_on_main_thread(&restore_app, &restore_window, focus);
    }) {
        crate::services::logging::warn(
            app,
            "window",
            format!("schedule restore failed label={label}: {error}"),
        );
    }
}

pub(crate) fn show_main_window(app: &AppHandle<Wry>) {
    let hidden_labels = {
        let state = app.state::<HiddenWithMainRegistry>();
        let mut labels = state
            .labels
            .lock()
            .expect("hidden window registry lock poisoned");
        labels.drain().collect::<Vec<_>>()
    };

    if let Some(window) = app.get_webview_window("main") {
        restore_window(app, &window, true);
    } else {
        crate::services::logging::warn(app, "tray", "show requested without a main window");
    }

    for label in hidden_labels {
        if label != "main" {
            if let Some(window) = app.get_webview_window(&label) {
                restore_window(app, &window, false);
            }
        }
    }
}

pub(crate) fn hide_main_window_and_children(app: &AppHandle<Wry>) {
    let mut hidden_labels = HashSet::new();
    for (label, window) in app.webview_windows() {
        if window.is_visible().unwrap_or(false) {
            match window.hide() {
                Ok(()) => {
                    hidden_labels.insert(label);
                }
                Err(error) => crate::services::logging::warn(
                    app,
                    "tray",
                    format!("hide failed label={label}: {error}"),
                ),
            }
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
    if is_quit {
        // A tray action can arrive while every FileTerm window is hidden. The
        // renderer owns the quit confirmation and dirty-editor prompts, so
        // make those surfaces visible before emitting the request instead of
        // leaving a modal active in an invisible WebView.
        show_main_window(app);
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.emit(
            "app:window-close-request",
            serde_json::json!({ "isQuit": is_quit }),
        ) {
            crate::services::logging::error(
                app,
                "tray",
                format!("close request delivery failed is_quit={is_quit}: {error}"),
            );
        }
    } else {
        crate::services::logging::error(app, "tray", "close requested without a main window");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    // Windows packages use Tauri's signed updater. macOS deliberately keeps
    // the Release-page flow so users choose the GitHub download themselves.
    #[cfg(target_os = "windows")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    #[cfg(target_os = "macos")]
    let builder = builder.on_page_load(|webview, payload| {
        if webview.label() == "main"
            && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
            && !MACOS_TRAFFIC_LIGHTS_CALIBRATED.load(Ordering::Acquire)
        {
            if let Some(window) = webview.get_webview_window("main") {
                let calibration_window = window.clone();
                let _ = window.run_on_main_thread(move || {
                    if calibrate_macos_traffic_lights(&calibration_window) {
                        MACOS_TRAFFIC_LIGHTS_CALIBRATED.store(true, Ordering::Release);
                    }
                });
            }
        }
    });

    builder
        .setup(|app| {
            // Initialize the logger before migration so portable-root and
            // legacy-source decisions remain diagnosable on first launch.
            crate::services::logging::init(app.handle());
            let migration_result = crate::storage::migrate_legacy_data_once(app.handle());
            // Install after `logging::init` so `LOG_DIRECTORY` is populated.
            // Captures panic location + payload for any spawned task that
            // panics (SSH worker, output pump, transfer service) — without
            // this, supervision code only sees a `JoinError` with no source
            // location and the panic site is lost.
            crate::services::logging::install_panic_hook();
            if let Err(error) = migration_result.as_ref() {
                crate::services::logging::error(
                    app.handle(),
                    "storage",
                    format!("startup migration failed: {error}"),
                );
            }
            migration_result?;

            match crate::storage::ensure_portable_marker() {
                Ok(Some(marker)) => crate::services::logging::info(
                    app.handle(),
                    "storage",
                    format!("portable marker ready path={}", marker.display()),
                ),
                Ok(None) => {}
                Err(error) => crate::services::logging::warn(
                    app.handle(),
                    "storage",
                    format!("unable to persist portable marker: {error}"),
                ),
            }

            let executable = std::env::current_exe()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("<unavailable:{error}>"));
            let portable_directory = crate::storage::portable_config_directory();
            let storage_mode = if portable_directory.is_some() {
                "portable"
            } else {
                "app-data"
            };
            let portable_directory = portable_directory
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            let app_data_directory = app
                .path()
                .app_data_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|error| format!("<unavailable:{error}>"));
            match crate::storage::storage_root(app.handle()) {
                Ok(root) => crate::services::logging::info(
                    app.handle(),
                    "storage",
                    format!(
                        "resolved mode={storage_mode} compiled_portable={} executable={executable} root={} portable_config={portable_directory} app_data={app_data_directory}",
                        crate::storage::is_compiled_portable_build(),
                        root.display()
                    ),
                ),
                Err(error) => crate::services::logging::error(
                    app.handle(),
                    "storage",
                    format!(
                        "unable to resolve storage root mode={storage_mode} compiled_portable={} executable={executable} portable_config={portable_directory} app_data={app_data_directory}: {error}",
                        crate::storage::is_compiled_portable_build()
                    ),
                ),
            }
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
            crate::services::serial_ports::start_watcher(app.handle());
            crate::services::mcp::start_runtime(app.handle())?;
            app.manage(FileEditorCloseRegistry::default());
            app.manage(QuitPreparationRegistry::default());
            app.manage(HiddenWithMainRegistry::default());

            let main_window = app
                .get_webview_window("main")
                .ok_or_else(|| "Failed to find main window".to_string())?;

            // ── Platform-specific window chrome ────────────────────────────
            // macOS: keep decorations + Overlay titleBarStyle so the traffic
            //        lights float over renderer content. AppKit control size
            //        and frames are calibrated after the first page load.
            // Windows/Linux: drop the OS frame so the renderer owns the
            // compact menu/title row. This also avoids a GTK titlebar above
            // the themed renderer menu on Linux.
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                let _ = main_window.set_decorations(false);
            }

            #[cfg(target_os = "windows")]
            {
                prefer_windows_native_rounded_corners(&main_window);
                install_windows_terminal_zoom_interceptor(&main_window);
                main_window
                    .set_icon(windows_icon_image().map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            }

            #[cfg(target_os = "macos")]
            if let Err(error) = apply_macos_main_window_vibrancy(&main_window) {
                crate::services::logging::warn(
                    app.handle(),
                    "window",
                    format!("failed to apply macOS main-window vibrancy: {error}"),
                );
            }

            let app_handle = app.handle().clone();
            main_window.on_window_event(move |event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    crate::services::logging::info(&app_handle, "window", "main close requested");
                    api.prevent_close();
                    request_main_window_close(&app_handle, false);
                }
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = app_handle.emit(
                            "app:window-maximized-change",
                            window.is_maximized().unwrap_or(false),
                        );
                    }
                    #[cfg(target_os = "macos")]
                    schedule_macos_traffic_light_recalibration(&app_handle);
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
            let tray_icon = windows_icon_image().map_err(|error| error.to_string())?;
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
                .on_menu_event(|app, event| {
                    let Some(action) = tray_menu_action(event.id().as_ref()) else {
                        return;
                    };
                    crate::services::logging::info(app, "tray", format!("menu action={action:?}"));
                    match action {
                        TrayMenuAction::OpenConnectionManager => {
                            open_child_window_from_native_event(
                                app,
                                OpenWindowInput {
                                    kind: "connection-manager".to_string(),
                                    mode: None,
                                    profile_id: None,
                                    command_id: None,
                                    folder_id: None,
                                    command: None,
                                    source: None,
                                    path: None,
                                    name: None,
                                    tab_id: None,
                                    encoding: None,
                                },
                            );
                        }
                        TrayMenuAction::OpenCommandManager => {
                            open_child_window_from_native_event(
                                app,
                                OpenWindowInput {
                                    kind: "command-manager".to_string(),
                                    mode: None,
                                    profile_id: None,
                                    command_id: None,
                                    folder_id: None,
                                    command: None,
                                    source: None,
                                    path: None,
                                    name: None,
                                    tab_id: None,
                                    encoding: None,
                                },
                            );
                        }
                        TrayMenuAction::ShowMain => show_main_window(app),
                        TrayMenuAction::RequestQuit => request_main_window_close(app, true),
                    }
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
                        crate::services::logging::info(
                            tray.app_handle(),
                            "tray",
                            "left click toggle main window",
                        );
                        toggle_main_window_visibility(tray.app_handle());
                    }
                })
                .build(app)
                .map_err(|error| error.to_string())?;

            // 启动后仅在用户允许时触发更新检查。延迟 1s 让前端先完成
            // onUpdateStatus 订阅；updates::check 内部已有 single-flight
            // 互斥，用户在此期间手动点击"检查更新"会复用同一次结果。
            // 无法读取旧偏好时维持既有行为，默认检查更新。
            let auto_check_updates = crate::commands::app_get_ui_preferences(app.handle().clone())
                .map(|preferences| preferences.auto_check_updates)
                .unwrap_or(true);
            if auto_check_updates {
                let startup_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let _ = crate::services::updates::check(&startup_handle).await;
                });
            }

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
                        command: None,
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
                        command: None,
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
                        command: None,
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
            "view-terminal-zoom-in" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:terminal-zoom-request", "in");
                }
            }
            "view-terminal-zoom-out" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:terminal-zoom-request", "out");
                }
            }
            "view-terminal-zoom-reset" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:terminal-zoom-request", "reset");
                }
            }
            "view-terminal-zoom-lock" => {
                if let Err(error) = crate::commands::app_toggle_terminal_zoom_lock(app.clone()) {
                    crate::services::logging::warn(
                        app,
                        "ui-preferences",
                        format!("failed to toggle terminal zoom lock: {error}"),
                    );
                }
            }
            "workspace-new-tab" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:new-tab-request", ());
                }
            }
            "view-split-vertical" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:split-pane-request", "row");
                }
            }
            "view-split-horizontal" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:split-pane-request", "column");
                }
            }
            "view-focus-pane-left" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:focus-pane-request", "left");
                }
            }
            "view-focus-pane-right" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:focus-pane-request", "right");
                }
            }
            "view-focus-pane-up" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:focus-pane-request", "up");
                }
            }
            "view-focus-pane-down" => {
                if let Some(window) = focused_webview_window(app) {
                    let _ = window.emit("app:focus-pane-request", "down");
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
            crate::commands::app_get_mcp_agent_setup,
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
            crate::commands::app_list_serial_ports,
            crate::commands::app_serial_control,
            crate::commands::app_serial_transfer,
            crate::commands::app_serial_cancel_transfer,
            crate::commands::app_save_session_log,
            crate::commands::app_get_ui_preferences,
            crate::commands::app_set_ui_preferences,
            crate::commands::app_get_security_settings,
            crate::commands::app_set_security_settings,
            crate::commands::app_verify_security_password,
            crate::commands::app_list_local_terminal_shells,
            crate::commands::app_list_ai_providers,
            crate::commands::app_save_ai_provider,
            crate::commands::app_delete_ai_provider,
            crate::commands::app_test_ai_provider,
            crate::commands::app_list_ai_conversations,
            crate::commands::app_get_ai_conversation,
            crate::commands::app_create_ai_conversation,
            crate::commands::app_rename_ai_conversation,
            crate::commands::app_summarize_ai_conversation_title,
            crate::commands::app_delete_ai_message,
            crate::commands::app_delete_ai_conversation,
            crate::commands::app_get_ai_copilot_mode_state,
            crate::commands::app_set_ai_copilot_mode,
            crate::commands::app_set_ai_context_attach,
            crate::commands::app_set_ai_dangerous_command_restrictions,
            crate::commands::app_create_ai_context_preview,
            crate::commands::app_start_ai_chat,
            crate::commands::app_retry_ai_chat,
            crate::commands::app_cancel_ai_chat,
            crate::commands::app_get_ui_state_item,
            crate::commands::app_set_ui_state_item,
            crate::commands::app_remove_ui_state_item,
            crate::commands::app_get_terminal_command_history,
            crate::commands::app_set_terminal_command_history,
            crate::commands::app_get_command_send_preferences,
            crate::commands::app_set_command_send_preferences,
            crate::commands::app_get_snapshot,
            crate::commands::app_get_connection_library,
            crate::commands::app_list_imported_fonts,
            crate::commands::app_import_font,
            crate::commands::app_get_imported_font_data,
            crate::commands::app_delete_imported_font,
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
            crate::commands::app_test_webdav_sync,
            crate::commands::app_upload_webdav_sync,
            crate::commands::app_download_webdav_sync,
            crate::commands::app_get_s3_backup_config,
            crate::commands::app_set_s3_backup_config,
            crate::commands::app_test_s3_backup,
            crate::commands::app_upload_s3_backup,
            crate::commands::app_download_s3_backup,
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
            crate::commands::app_split_tab,
            crate::commands::app_close_pane,
            crate::commands::app_set_active_pane,
            crate::commands::app_set_pane_weights,
            crate::commands::app_open_local_terminal,
            crate::commands::app_write_terminal,
            crate::commands::app_subscribe_terminal_data,
            crate::commands::app_resize_terminal,
            crate::commands::app_open_remote_path,
            crate::commands::app_set_follow_shell_cwd,
            crate::commands::app_execute_remote_command,
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
            crate::commands::app_resolve_sudo_password_prompt,
            crate::commands::app_set_sudo_password_renderer_ready,
            crate::commands::app_resolve_backup_password,
            crate::commands::app_set_backup_password_renderer_ready,
            crate::commands::app_list_ssh_tunnels,
            crate::commands::app_create_ssh_tunnel,
            crate::commands::app_start_ssh_tunnel,
            crate::commands::app_stop_ssh_tunnel,
            crate::commands::app_delete_ssh_tunnel,
            // Phase 2: profile / folder / command CRUD
            crate::commands::app_create_profile,
            crate::commands::app_update_profile,
            crate::commands::app_test_connection,
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
            crate::commands::app_resolve_mcp_approval,
            crate::commands::app_resolve_action_approval,
            crate::commands::app_resolve_ai_terminal_handoff,
            // Local files
            crate::sessions::local_files::app_list_local_directory,
            crate::sessions::local_files::app_connect_local_network_share,
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

            if matches!(_event, tauri::RunEvent::Exit) {
                crate::services::mcp::remove_runtime_descriptor(_app_handle);
            }

            #[cfg(target_os = "macos")]
            if matches!(_event, tauri::RunEvent::Exit) {
                crate::sessions::local_files::cleanup_network_mounts();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::{
        application_quit_accelerator, center_child_window_position,
        child_window_should_be_transparent, tray_icon_should_be_template, tray_menu_action,
        tray_menu_labels, FileEditorCloseRegistry, QuitPreparationRegistry, TrayMenuAction,
        WindowMenuKind,
    };
    use tauri::{PhysicalPosition, PhysicalSize};

    #[cfg(target_os = "windows")]
    use super::windows_icon_image;

    #[cfg(target_os = "macos")]
    use super::{macos_traffic_light_target_center, MACOS_TRAFFIC_LIGHT_FRAME_SIZE};

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_traffic_lights_use_absolute_renderer_titlebar_geometry() {
        let window_height = 820.0;
        let close = macos_traffic_light_target_center(window_height, 0);
        let miniaturize = macos_traffic_light_target_center(window_height, 1);
        let zoom = macos_traffic_light_target_center(window_height, 2);

        assert_eq!(close, (27.0, 796.0));
        assert_eq!(miniaturize, (50.0, 796.0));
        assert_eq!(zoom, (73.0, 796.0));
        assert_eq!(close.0 - MACOS_TRAFFIC_LIGHT_FRAME_SIZE / 2.0, 20.0);
    }

    #[test]
    fn retains_only_platform_quit_accelerators() {
        assert_eq!(application_quit_accelerator("macos"), "Cmd+Q");
        assert_eq!(application_quit_accelerator("windows"), "Alt+F4");
        assert_eq!(application_quit_accelerator("linux"), "Alt+F4");
    }

    #[test]
    fn uses_template_tray_icons_on_macos_only() {
        assert!(tray_icon_should_be_template("macos"));
        assert!(!tray_icon_should_be_template("windows"));
        assert!(!tray_icon_should_be_template("linux"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn loads_the_high_resolution_windows_icon_for_windows_surfaces() {
        let icon = windows_icon_image().unwrap();

        // `Image::from_bytes` decodes multi-size ICO containers through the
        // image crate, which selects the largest frame. The high-resolution
        // 256px bitmap is exactly what tray and window surfaces want.
        assert_eq!((icon.width(), icon.height()), (256, 256));
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
    fn tray_menu_ids_cover_every_visible_action() {
        assert_eq!(
            tray_menu_action("tray-show-main"),
            Some(TrayMenuAction::ShowMain)
        );
        assert_eq!(
            tray_menu_action("tray-connection-manager"),
            Some(TrayMenuAction::OpenConnectionManager)
        );
        assert_eq!(
            tray_menu_action("tray-command-manager"),
            Some(TrayMenuAction::OpenCommandManager)
        );
        assert_eq!(
            tray_menu_action("tray-quit"),
            Some(TrayMenuAction::RequestQuit)
        );
        assert_eq!(tray_menu_action("unknown"), None);
    }

    #[test]
    fn frameless_macos_and_linux_child_windows_use_transparency() {
        assert!(child_window_should_be_transparent("macos", false));
        assert!(!child_window_should_be_transparent("macos", true));
        assert!(!child_window_should_be_transparent("windows", false));
        assert!(!child_window_should_be_transparent("windows", true));
        assert!(child_window_should_be_transparent("linux", false));
        assert!(!child_window_should_be_transparent("linux", true));
    }

    #[test]
    fn centers_child_window_relative_to_main_window() {
        let position = center_child_window_position(
            PhysicalPosition::new(100, 200),
            PhysicalSize::new(1000, 800),
            PhysicalSize::new(400, 300),
            None,
        );

        assert_eq!((position.x, position.y), (400, 450));
    }

    #[test]
    fn keeps_centered_child_window_inside_monitor_work_area() {
        let position = center_child_window_position(
            PhysicalPosition::new(1800, 900),
            PhysicalSize::new(800, 600),
            PhysicalSize::new(1000, 700),
            Some((PhysicalPosition::new(0, 0), PhysicalSize::new(1920, 1080))),
        );

        assert_eq!((position.x, position.y), (920, 380));
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

    #[test]
    fn cli_dispatch_includes_exec_without_starting_tauri() {
        assert!(super::is_cli_command(Some("exec")));
        assert!(super::is_cli_command(Some("wait-transfer")));
        assert!(super::is_cli_command(Some("cli")));
        assert!(!super::is_cli_command(Some("mcp")));
        assert!(!super::is_cli_command(Some("unknown")));
    }
}
