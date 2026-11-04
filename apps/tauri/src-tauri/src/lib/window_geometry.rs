use std::{fs, time::Duration};

use serde::{Deserialize, Serialize};
use tauri::LogicalSize;

const MAIN_WINDOW_STATE_FILE: &str = "window-state.json";
const MAIN_WINDOW_DEFAULT_WIDTH: f64 = 1280.0;
const MAIN_WINDOW_DEFAULT_HEIGHT: f64 = 820.0;
const MAIN_WINDOW_MIN_WIDTH: f64 = 1150.0;
const MAIN_WINDOW_MIN_HEIGHT: f64 = 790.0;
const MAIN_WINDOW_RESIZE_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
struct MainWindowGeometry {
    width: f64,
    height: f64,
    maximized: bool,
}

impl Default for MainWindowGeometry {
    fn default() -> Self {
        Self {
            width: MAIN_WINDOW_DEFAULT_WIDTH,
            height: MAIN_WINDOW_DEFAULT_HEIGHT,
            maximized: false,
        }
    }
}

#[derive(Default)]
struct MainWindowGeometryPersistence {
    revision: std::sync::atomic::AtomicU64,
    write_lock: std::sync::Mutex<()>,
}

fn read_main_window_geometry(
    app: &AppHandle<Wry>,
) -> Result<Option<MainWindowGeometry>, crate::AppError> {
    let path = crate::storage::workspace_file(app, MAIN_WINDOW_STATE_FILE)?;
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(crate::AppError::Storage(error.to_string())),
    };
    let geometry: MainWindowGeometry = serde_json::from_str(&content)
        .map_err(|error| crate::AppError::Serialization(error.to_string()))?;
    if !geometry.width.is_finite()
        || !geometry.height.is_finite()
        || geometry.width <= 0.0
        || geometry.height <= 0.0
    {
        return Err(crate::AppError::Serialization(
            "主窗口尺寸必须为有限正数".to_string(),
        ));
    }
    Ok(Some(geometry))
}

fn fit_window_size(
    desired: LogicalSize<f64>,
    minimum: LogicalSize<f64>,
    work_area: LogicalSize<f64>,
) -> (LogicalSize<f64>, LogicalSize<f64>) {
    // Leave room for native frames without letting a minimum exceed the screen.
    let max_width = (work_area.width - 24.0).max(1.0);
    let max_height = (work_area.height - 48.0).max(1.0);
    let minimum = LogicalSize::new(minimum.width.min(max_width), minimum.height.min(max_height));
    (
        LogicalSize::new(
            desired.width.clamp(minimum.width, max_width),
            desired.height.clamp(minimum.height, max_height),
        ),
        minimum,
    )
}

fn monitor_work_area(monitor: &tauri::Monitor) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let area = monitor.work_area();
    if area.size.width > 0 && area.size.height > 0 {
        (area.position, area.size)
    } else {
        (*monitor.position(), *monitor.size())
    }
}

fn watch_window_display_changes(window: &WebviewWindow<Wry>, minimum: LogicalSize<f64>) {
    let display_geometry = |window: &WebviewWindow<Wry>| {
        window.current_monitor().ok().flatten().map(|monitor| {
            let (position, area) = monitor_work_area(&monitor);
            (position, area, monitor.scale_factor())
        })
    };
    let previous = std::sync::Mutex::new(display_geometry(window));
    let tracked = window.clone();
    window.on_window_event(move |event| {
        if !matches!(
            event,
            WindowEvent::Resized(_)
                | WindowEvent::Moved(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Focused(true)
        ) {
            return;
        }
        let Some(display) = display_geometry(&tracked) else {
            return;
        };
        {
            let Ok(mut previous) = previous.lock() else {
                return;
            };
            if *previous == Some(display) {
                return;
            }
            // macOS scaled modes can change the work area while backingScaleFactor stays 2.
            *previous = Some(display);
        }
        if let Err(error) = fit_existing_window_to_display(&tracked, minimum, display) {
            crate::services::logging::warn(
                tracked.app_handle(),
                "window",
                format!(
                    "unable to adapt display change label={}: {error}",
                    tracked.label()
                ),
            );
        }
    });
}

fn fit_existing_window_to_display(
    window: &WebviewWindow<Wry>,
    minimum: LogicalSize<f64>,
    (position, area, scale): (PhysicalPosition<i32>, PhysicalSize<u32>, f64),
) -> tauri::Result<()> {
    let current = window.inner_size()?.to_logical(window.scale_factor()?);
    let (size, minimum) = fit_window_size(current, minimum, area.to_logical(scale));
    window.set_min_size(Some(minimum))?;
    if window.is_maximized()? || window.is_fullscreen()? {
        return Ok(());
    }
    if size != current {
        window.set_size(size)?;
    }
    let outer_size = window.outer_size()?;
    let current_position = window.outer_position()?;
    let fitted_position = center_child_window_position(
        current_position,
        outer_size,
        outer_size,
        Some((position, area)),
    );
    if fitted_position != current_position {
        window.set_position(fitted_position)?;
    }
    Ok(())
}

fn restore_main_window_geometry(
    app: &AppHandle<Wry>,
    window: &WebviewWindow<Wry>,
    remember_window_size: bool,
) {
    let geometry = if remember_window_size {
        match read_main_window_geometry(app) {
            Ok(Some(geometry)) => geometry,
            Ok(None) => MainWindowGeometry::default(),
            Err(error) => {
                crate::services::logging::warn(
                    app,
                    "window",
                    format!("unable to read saved main window size: {error}"),
                );
                MainWindowGeometry::default()
            }
        }
    } else {
        MainWindowGeometry::default()
    };
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let mut size = LogicalSize::new(geometry.width, geometry.height);
    if let Some(monitor) = &monitor {
        let (_, area) = monitor_work_area(monitor);
        let (fitted, minimum) = fit_window_size(
            size,
            LogicalSize::new(MAIN_WINDOW_MIN_WIDTH, MAIN_WINDOW_MIN_HEIGHT),
            area.to_logical(monitor.scale_factor()),
        );
        if let Err(error) = window.set_min_size(Some(minimum)) {
            crate::services::logging::warn(
                app,
                "window",
                format!("unable to adapt minimum window size: {error}"),
            );
        }
        size = fitted;
    }
    if !geometry.maximized && window.is_maximized().unwrap_or(false) {
        if let Err(error) = window.unmaximize() {
            crate::services::logging::warn(
                app,
                "window",
                format!("unable to unmaximize fixed-size main window: {error}"),
            );
        }
    }
    if let Err(error) = window.set_size(size) {
        crate::services::logging::warn(
            app,
            "window",
            format!("unable to restore main window size: {error}"),
        );
        return;
    }
    if geometry.maximized {
        if let Err(error) = window.maximize() {
            crate::services::logging::warn(
                app,
                "window",
                format!("unable to restore maximized main window: {error}"),
            );
        }
    } else if let Some(monitor) = monitor {
        let (position, area) = monitor_work_area(&monitor);
        if let Ok(outer_size) = window.outer_size() {
            let centered =
                center_child_window_position(position, area, outer_size, Some((position, area)));
            if let Err(error) = window.set_position(centered) {
                crate::services::logging::warn(
                    app,
                    "window",
                    format!("unable to position main window in work area: {error}"),
                );
            }
        }
    }
}

fn schedule_main_window_geometry_save(app: &AppHandle<Wry>) {
    let revision = app
        .state::<MainWindowGeometryPersistence>()
        .revision
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MAIN_WINDOW_RESIZE_DEBOUNCE).await;
        if app
            .state::<MainWindowGeometryPersistence>()
            .revision
            .load(std::sync::atomic::Ordering::Relaxed)
            != revision
        {
            return;
        }
        persist_main_window_geometry(&app);
    });
}

fn persist_main_window_geometry(app: &AppHandle<Wry>) {
    let persistence = app.state::<MainWindowGeometryPersistence>();
    let Ok(_guard) = persistence.write_lock.lock() else {
        crate::services::logging::warn(app, "window", "main window size write lock is poisoned");
        return;
    };
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let mut geometry = read_main_window_geometry(app)
        .ok()
        .flatten()
        .unwrap_or_default();
    let maximized = window.is_maximized().unwrap_or(false);
    if !maximized {
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let Ok(size) = window.inner_size() else {
            return;
        };
        let logical_size = size.to_logical::<f64>(scale_factor);
        geometry.width = logical_size.width;
        geometry.height = logical_size.height;
    }
    geometry.maximized = maximized;

    if let Err(error) = write_main_window_geometry(app, &geometry) {
        crate::services::logging::warn(
            app,
            "window",
            format!("unable to save main window size: {error}"),
        );
    }
}

fn flush_main_window_geometry(app: &AppHandle<Wry>) {
    app.state::<MainWindowGeometryPersistence>()
        .revision
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    persist_main_window_geometry(app);
}

fn write_main_window_geometry(
    app: &AppHandle<Wry>,
    geometry: &MainWindowGeometry,
) -> Result<(), crate::AppError> {
    let path = crate::storage::workspace_file(app, MAIN_WINDOW_STATE_FILE)?;
    let temporary = path.with_file_name(format!(
        ".{MAIN_WINDOW_STATE_FILE}.{}.tmp",
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_vec_pretty(geometry)
        .map_err(|error| crate::AppError::Serialization(error.to_string()))?;
    fs::write(&temporary, content).map_err(|error| crate::AppError::Storage(error.to_string()))?;
    if let Err(error) = crate::storage::replace_file_atomically(&temporary, &path) {
        let _ = fs::remove_file(temporary);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/window_geometry_tests.rs"]
mod window_geometry_tests;
