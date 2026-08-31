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
