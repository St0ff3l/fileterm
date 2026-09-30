//! Local diagnostics facade. Business code keeps its existing logging entry points.
//! Formatting/redaction, classification and file IO live in separate modules.

mod category;
mod format;
mod layer;
mod level;
mod panic;
mod writer;

pub use category::LogCategory;
pub use level::LogLevel;
pub use panic::install_panic_hook;

use crate::storage::state_path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::AppHandle;
use tracing_subscriber::prelude::*;

static LOG_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static TRACING_LAYER_ACTIVE: AtomicBool = AtomicBool::new(false);

fn log_directory(app: &AppHandle) -> Option<PathBuf> {
    state_path(app).ok().map(|path| path.with_file_name("logs"))
}

pub fn init(app: &AppHandle) {
    if let Some(directory) = log_directory(app) {
        let _ = LOG_DIRECTORY.set(directory.clone());
        writer::init(directory.clone());
        let filter = tracing_subscriber::EnvFilter::builder()
            .with_env_var("FILETERM_LOG_LEVEL")
            .with_default_directive(tracing_subscriber::filter::LevelFilter::DEBUG.into())
            .from_env_lossy();
        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(layer::FileTermLogLayer::new(directory.clone()));
        match tracing::subscriber::set_global_default(subscriber) {
            Ok(()) => {
                TRACING_LAYER_ACTIVE.store(true, Ordering::Release);
                // Some host integrations may already own the `log` facade.
                // The tracing subscriber remains usable even when that bridge
                // cannot be installed.
                let _ = tracing_log::LogTracer::init();
            }
            Err(error) if !TRACING_LAYER_ACTIVE.load(Ordering::Acquire) => {
                let message = format!(
                    "unable to install global tracing subscriber; using compatibility writer: {error}"
                );
                let line =
                    format::build_line(LogLevel::Warn, LogCategory::Logging, "logging", &message);
                writer::dispatch(directory, LogLevel::Warn, line);
            }
            Err(_) => {}
        }
    }
}

pub fn write(app: &AppHandle, level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(level) = LogLevel::parse(level) else {
        return;
    };
    write_level(app, level, scope, message);
}

pub fn write_level(app: &AppHandle, level: LogLevel, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = log_directory(app) else {
        return;
    };
    dispatch_event(directory, level, scope, message.as_ref());
}

pub fn write_global(level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(level) = LogLevel::parse(level) else {
        return;
    };
    let Some(directory) = LOG_DIRECTORY.get() else {
        return;
    };
    dispatch_event(directory.clone(), level, scope, message.as_ref());
}

fn dispatch_event(directory: PathBuf, level: LogLevel, scope: &str, message: &str) {
    if TRACING_LAYER_ACTIVE.load(Ordering::Acquire) {
        let category = LogCategory::from_scope(scope);
        match level {
            LogLevel::Trace => tracing::event!(
                target: "fileterm::diagnostics",
                tracing::Level::TRACE,
                category = category.as_str(),
                scope = scope,
                message = %message,
            ),
            LogLevel::Debug => tracing::event!(
                target: "fileterm::diagnostics",
                tracing::Level::DEBUG,
                category = category.as_str(),
                scope = scope,
                message = %message,
            ),
            LogLevel::Info => tracing::event!(
                target: "fileterm::diagnostics",
                tracing::Level::INFO,
                category = category.as_str(),
                scope = scope,
                message = %message,
            ),
            LogLevel::Warn => tracing::event!(
                target: "fileterm::diagnostics",
                tracing::Level::WARN,
                category = category.as_str(),
                scope = scope,
                message = %message,
            ),
            LogLevel::Error => tracing::event!(
                target: "fileterm::diagnostics",
                tracing::Level::ERROR,
                category = category.as_str(),
                scope = scope,
                message = %message,
            ),
        }
        return;
    }

    // Keep diagnostics available if another library installed a global
    // subscriber before FileTerm could register its own layer.
    if !level.enabled() {
        return;
    }
    let line = format::build_line(level, LogCategory::from_scope(scope), scope, message);
    writer::dispatch(directory, level, line);
}

/// Best-effort bounded drain on normal desktop exit; forced process termination cannot flush.
pub fn flush() {
    writer::flush(std::time::Duration::from_millis(500));
}

pub fn debug(app: &AppHandle, scope: &str, message: impl AsRef<str>) {
    write(app, "DEBUG", scope, message);
}

pub fn info(app: &AppHandle, scope: &str, message: impl AsRef<str>) {
    write(app, "INFO", scope, message);
}

pub fn warn(app: &AppHandle, scope: &str, message: impl AsRef<str>) {
    write(app, "WARN", scope, message);
}

pub fn error(app: &AppHandle, scope: &str, message: impl AsRef<str>) {
    write(app, "ERROR", scope, message);
}

pub fn debug_global(scope: &str, message: impl AsRef<str>) {
    write_global("DEBUG", scope, message);
}

pub fn info_global(scope: &str, message: impl AsRef<str>) {
    write_global("INFO", scope, message);
}

pub fn warn_global(scope: &str, message: impl AsRef<str>) {
    write_global("WARN", scope, message);
}

pub fn error_global(scope: &str, message: impl AsRef<str>) {
    write_global("ERROR", scope, message);
}

pub fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        messages.push(cause.to_string());
        source = cause.source();
    }
    messages.join(" <- ")
}

pub fn session(
    app: &AppHandle,
    level: &str,
    protocol: &str,
    tab_id: &str,
    message: impl AsRef<str>,
) {
    write(app, level, &format!("{protocol}:{tab_id}"), message);
}

pub fn ssh_debug(app: &AppHandle, tab_id: &str, message: impl AsRef<str>) {
    write(app, "DEBUG", &format!("ssh:{tab_id}"), message);
}
