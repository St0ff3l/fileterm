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
}
