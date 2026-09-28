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

fn clamp_main_window_geometry(
    window: &WebviewWindow<Wry>,
    mut geometry: MainWindowGeometry,
) -> MainWindowGeometry {
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    if let Some(monitor) = monitor {
        let work_area = monitor
            .work_area()
            .size
            .to_logical::<f64>(monitor.scale_factor());
        let max_width = (work_area.width - 24.0).max(MAIN_WINDOW_MIN_WIDTH);
        let max_height = (work_area.height - 48.0).max(MAIN_WINDOW_MIN_HEIGHT);
        geometry.width = geometry.width.clamp(MAIN_WINDOW_MIN_WIDTH, max_width);
        geometry.height = geometry.height.clamp(MAIN_WINDOW_MIN_HEIGHT, max_height);
    }
    geometry
}

fn restore_main_window_geometry(app: &AppHandle<Wry>, window: &WebviewWindow<Wry>) {
    let geometry = match read_main_window_geometry(app) {
        Ok(Some(geometry)) => geometry,
        Ok(None) => return,
        Err(error) => {
            crate::services::logging::warn(
                app,
                "window",
                format!("unable to read saved main window size: {error}"),
            );
            return;
        }
    };
    let geometry = clamp_main_window_geometry(window, geometry);
    if let Err(error) = window.set_size(LogicalSize::new(geometry.width, geometry.height)) {
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
