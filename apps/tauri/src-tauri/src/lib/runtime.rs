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
            let tray_icon = Image::from_bytes(include_bytes!("../../../build/trayTemplate@2x.png"))
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
            crate::commands::app_reset_security_backup_password,
            crate::commands::app_verify_security_password,
            crate::commands::app_list_local_terminal_shells,
            crate::commands::app_list_ai_providers,
            crate::commands::app_list_ai_models,
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
            crate::commands::app_attach_background_session,
            crate::commands::app_detach_session_to_background,
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
            crate::commands::app_clear_trusted_host_fingerprint,
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
            crate::commands::app_execute_ai_terminal_handoff,
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
