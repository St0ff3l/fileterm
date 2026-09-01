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
    Image::from_bytes(include_bytes!("../../../build/icon.ico"))
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
