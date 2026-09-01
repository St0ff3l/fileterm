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
        // with the new mode/id URL. Do this by navigating the existing
        // WebviewWindow instead of destroying and immediately rebuilding the
        // same label. Native window destruction is asynchronous on macOS and
        // Windows; rebuilding synchronously used to race with label removal,
        // leaving an old form/listener alive and making SSH prompts appear
        // intermittently.
        if matches!(input.kind.as_str(), "connection-form" | "command-form") {
            let current_url = window
                .url()
                .map_err(|error| AppError::Window(error.to_string()))?;
            let target_url = current_url
                .join(&window_url(&input).to_string())
                .map_err(|error| AppError::Window(error.to_string()))?;
            window
                .navigate(target_url)
                .map_err(|error| AppError::Window(error.to_string()))?;
            center_child_window_on_main(app, &window);
            restore_window(app, &window, true);
            crate::services::logging::debug(
                app,
                "window",
                format!("navigated existing label={label} kind={}", input.kind),
            );
            return Ok(());
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
