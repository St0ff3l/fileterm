use super::*;

#[test]
fn high_dpi_work_area_limits_default_and_saved_sizes_and_native_minimum() {
    let area = PhysicalSize::new(1920_u32, 1040_u32).to_logical(1.75);
    for desired in [
        LogicalSize::new(MAIN_WINDOW_DEFAULT_WIDTH, MAIN_WINDOW_DEFAULT_HEIGHT),
        LogicalSize::new(3000.0, 2000.0),
    ] {
        let (size, minimum) = fit_window_size(
            desired,
            LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT),
            area,
        );
        assert!(size.width < area.width && size.height < area.height);
        assert!(minimum.width <= size.width && minimum.height <= size.height);
        assert!(minimum.width < MAIN_WINDOW_MIN_WIDTH);
        assert!(minimum.height < MAIN_WINDOW_MIN_HEIGHT);
    }
}

#[test]
fn normal_screen_keeps_default_and_saved_sizes() {
    let minimum = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
    for desired in [
        LogicalSize::new(1280.0, 820.0),
        LogicalSize::new(1600.0, 950.0),
    ] {
        assert_eq!(
            fit_window_size(desired, minimum, LogicalSize::new(1920.0, 1040.0)),
            (desired, minimum)
        );
    }
}

#[test]
fn four_k_keeps_default_logical_size_at_common_scales() {
    let desired = LogicalSize::new(MAIN_WINDOW_DEFAULT_WIDTH, MAIN_WINDOW_DEFAULT_HEIGHT);
    let minimum = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
    let physical_area = PhysicalSize::new(3840_u32, 2080_u32);
    for scale in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let (size, fitted_minimum) =
            fit_window_size(desired, minimum, physical_area.to_logical(scale));
        assert_eq!(size, desired, "4K scale={scale}");
        assert_eq!(fitted_minimum, minimum, "4K scale={scale}");
        let physical_size = size.to_physical::<u32>(scale);
        assert_eq!(physical_size.width, (1280.0 * scale) as u32);
        assert_eq!(physical_size.height, (820.0 * scale) as u32);
    }
}

#[test]
fn four_k_extreme_scaling_and_saved_large_windows_fit_work_area() {
    let physical_area = PhysicalSize::new(3840_u32, 2080_u32);
    let minimum = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
    for scale in [1.0, 1.5, 2.0, 2.5, 3.0, 4.0] {
        let area = physical_area.to_logical::<f64>(scale);
        for desired in [
            LogicalSize::new(1280.0, 820.0),
            LogicalSize::new(3000.0, 1800.0),
        ] {
            let (size, fitted_minimum) = fit_window_size(desired, minimum, area);
            assert!(size.width <= area.width - 24.0, "4K scale={scale}");
            assert!(size.height <= area.height - 48.0, "4K scale={scale}");
            assert!(fitted_minimum.width <= size.width && fitted_minimum.height <= size.height);
            let physical_size = size.to_physical::<u32>(scale);
            assert!(physical_size.width <= physical_area.width);
            assert!(physical_size.height <= physical_area.height);
        }
    }
}

#[test]
fn saved_four_k_window_fits_smaller_screen_and_negative_origin_display() {
    let desired = LogicalSize::new(3000.0, 1800.0);
    let minimum = LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT);
    for (area, scale) in [
        (PhysicalSize::new(3840_u32, 2080_u32), 2.0),
        (PhysicalSize::new(1920_u32, 1040_u32), 1.75),
    ] {
        let (size, _) = fit_window_size(desired, minimum, area.to_logical(scale));
        let physical_size = size.to_physical(scale);
        for origin in [
            PhysicalPosition::new(-3840, 80),
            PhysicalPosition::new(1920, 0),
        ] {
            let position =
                center_child_window_position(origin, area, physical_size, Some((origin, area)));
            assert!(position.x >= origin.x && position.y >= origin.y);
            assert!(
                i64::from(position.x) + i64::from(physical_size.width)
                    <= i64::from(origin.x) + i64::from(area.width)
            );
            assert!(
                i64::from(position.y) + i64::from(physical_size.height)
                    <= i64::from(origin.y) + i64::from(area.height)
            );
        }
    }
}

#[test]
fn small_work_area_also_fits_editor_and_connection_windows() {
    for (desired, minimum) in [
        (
            LogicalSize::new(1220.0, 780.0),
            LogicalSize::new(1040.0, 620.0),
        ),
        (
            LogicalSize::new(860.0, 680.0),
            LogicalSize::new(760.0, 620.0),
        ),
    ] {
        let (size, minimum) = fit_window_size(desired, minimum, LogicalSize::new(800.0, 550.0));
        assert!(size.width <= 776.0 && size.height <= 502.0);
        assert!(minimum.width <= size.width && minimum.height <= size.height);
    }
}

#[test]
fn resolution_scale_and_desktop_work_area_matrix() {
    let resolutions = [
        (1280_u32, 720_u32),
        (1366, 768),
        (1920, 1080),
        (2048, 1080),
        (2560, 1440),
        (2560, 1600),
        (2880, 1800),
        (3024, 1964),
        (3456, 2234),
        (3840, 2160),
        (5120, 2880),
        (3440, 1440),
        (1080, 1920),
    ];
    let windows = [
        ("main", (1280.0, 820.0), (1150.0, 790.0)),
        ("editor", (1220.0, 780.0), (1040.0, 620.0)),
        ("connection", (860.0, 680.0), (760.0, 620.0)),
        ("saved-large", (3000.0, 1800.0), (1150.0, 790.0)),
    ];
    // Insets are fixture inputs, not claims about native desktop reporting.
    let desktop_insets = [
        ("unreserved", (0.0, 0.0, 0.0)),
        ("bottom-taskbar", (0.0, 0.0, 48.0)),
        ("left-taskbar", (64.0, 0.0, 0.0)),
        ("menu-and-dock", (0.0, 32.0, 80.0)),
        ("top-bottom-panels", (0.0, 32.0, 48.0)),
    ];
    let mut checked = 0;
    for (width, height) in resolutions {
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0, 4.0] {
            for (desktop, (left, top, bottom)) in desktop_insets {
                let reserved = LogicalSize::new(left, top + bottom).to_physical::<u32>(scale);
                let area = PhysicalSize::new(width - reserved.width, height - reserved.height);
                for (kind, desired, minimum) in windows {
                    let context = format!("{width}x{height} scale={scale} {desktop} {kind}");
                    let (size, minimum) = fit_window_size(
                        LogicalSize::new(desired.0, desired.1),
                        LogicalSize::new(minimum.0, minimum.1),
                        area.to_logical(scale),
                    );
                    assert!(size.width > 0.0 && size.height > 0.0, "{context}");
                    assert!(
                        minimum.width <= size.width && minimum.height <= size.height,
                        "{context}"
                    );
                    let outer_size = LogicalSize::new(size.width + 24.0, size.height + 48.0)
                        .to_physical::<u32>(scale);
                    assert!(
                        outer_size.width <= area.width && outer_size.height <= area.height,
                        "{context}"
                    );
                    for (x, y) in [
                        (0, 0),
                        (-(width as i32), 120),
                        (width as i32, -(height as i32)),
                    ] {
                        let origin = PhysicalPosition::new(
                            x + (left * scale) as i32,
                            y + (top * scale) as i32,
                        );
                        let position = center_child_window_position(
                            origin,
                            area,
                            outer_size,
                            Some((origin, area)),
                        );
                        assert!(
                            position.x >= origin.x && position.y >= origin.y,
                            "{context}"
                        );
                        assert!(
                            i64::from(position.x) + i64::from(outer_size.width)
                                <= i64::from(origin.x) + i64::from(area.width),
                            "{context}"
                        );
                        assert!(
                            i64::from(position.y) + i64::from(outer_size.height)
                                <= i64::from(origin.y) + i64::from(area.height),
                            "{context}"
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 7020);
}
