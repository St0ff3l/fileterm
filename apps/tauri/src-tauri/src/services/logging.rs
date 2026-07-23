//! Small structured file logger for the Tauri runtime.
//!
//! It deliberately avoids putting credentials, bearer tokens, or private-key
//! passphrases in diagnostics. The logs are local-only and can be opened from
//! Settings through `app_open_logs_directory`.
//!
//! File IO runs on a dedicated blocking thread via `spawn_blocking` so the
//! Tokio reactor is never stalled by `fs::write`/`OpenOptions` while a worker
//! loop is waiting on the same Tokio thread. The synchronous `LOG_LOCK` is
//! still acquired, but only inside the blocking task — callers from async
//! contexts return immediately after handing the line off.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use crate::storage::state_path;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
static LOG_LOCK: Mutex<()> = Mutex::new(());
static LOG_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
static AUTHORIZATION_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)(authorization["']?\s*[:=]\s*["']?(?:bearer|basic)\s+)[^\s,;"'}]+"#)
        .expect("static authorization redaction regex")
});
static SECRET_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"(?i)(password|passphrase|authorization|proxy[_-]?password|token)["']?\s*([:=])\s*["']?([^\s,;"'}]+)"#,
    )
    .expect("static redaction regex")
});

fn log_directory(app: &AppHandle) -> Option<std::path::PathBuf> {
    state_path(app).ok().map(|path| path.with_file_name("logs"))
}

pub fn init(app: &AppHandle) {
    if let Some(directory) = log_directory(app) {
        let _ = LOG_DIRECTORY.set(directory);
    }
}

fn redact(message: &str) -> String {
    let message = AUTHORIZATION_PATTERN.replace_all(message, "$1[REDACTED]");
    SECRET_PATTERN
        .replace_all(&message, "$1$2[REDACTED]")
        .into_owned()
}

/// Build the formatted log line. Pure / allocation-only, safe to call from
/// async contexts without touching the filesystem.
fn build_line(level: &str, scope: &str, message: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{timestamp} [{level}] [{scope}] {}\n", redact(message))
}

/// Synchronous append — only called from `spawn_blocking`. Holds `LOG_LOCK`
/// and does all filesystem work off the reactor thread.
fn append_sync(directory: &Path, line: &str) {
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let path = directory.join("app.log");
    if fs::metadata(&path)
        .map(|metadata| metadata.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let backup = directory.join("app.log.1");
        let _ = fs::remove_file(&backup);
        let _ = fs::rename(&path, backup);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Dispatch a log line to a blocking thread when running inside a Tokio
/// runtime; fall back to inline writes outside the runtime (e.g. during early
/// startup or unit tests). Inline writes are acceptable there because no
/// worker loop is depending on the calling thread.
fn dispatch(directory: PathBuf, line: String) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        // Fire-and-forget: the caller never waits for the file write. This is
        // intentional — log latency must not propagate back into SSH worker
        // loops, where it would manifest as multi-hundred-ms `select!` stalls
        // and unresponsive Ctrl+C under high-throughput output.
        handle.spawn_blocking(move || append_sync(&directory, &line));
    } else {
        append_sync(&directory, &line);
    }
}

pub fn write(app: &AppHandle, level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = log_directory(app) else {
        return;
    };
    let line = build_line(level, scope, message.as_ref());
    dispatch(directory, line);
}

pub fn write_global(level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = LOG_DIRECTORY.get() else {
        return;
    };
    let line = build_line(level, scope, message.as_ref());
    dispatch(directory.clone(), line);
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

/// Install a panic hook that mirrors the default stderr output but also
/// appends the panic payload + location to the local session log via
/// `write_global`.
///
/// Spawned tokio tasks (SSH worker, output pump, transfer service) panic
/// silently otherwise: the JoinHandle is awaited by supervision code which
/// only sees a `JoinError` whose `Display` carries no source location. With
/// this hook installed, the file log captures `panic at FILE:LINE:COL:
/// PAYLOAD` on the panicking thread before the task unwinds — so the
/// per-tab ERROR line from the supervision layer can be cross-referenced
/// with the exact panic site.
///
/// `write_global` dispatches via `spawn_blocking` when a Tokio runtime is
/// available (the common case for worker-thread panics) and falls back to
/// an inline `fs::write` otherwise, so the hook is safe to call from any
/// thread including the main thread during early startup.
///
/// Must be called after `init` so `LOG_DIRECTORY` is populated.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<non-string panic payload>");
        write_global("ERROR", "panic", format!("panic at {location}: {payload}"));
        // Preserve default stderr behaviour so `RUST_BACKTRACE` output and
        // the dev console still see the panic.
        previous(info);
    }));
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

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn strips_common_secret_labels() {
        let line = redact(
            r##"password=hunter2 Authorization: Bearer very-secret Authorization=Basic encoded-secret proxyPassword:abc "passphrase":"private" token='opaque'"##,
        );
        assert!(!line.contains("hunter2"));
        assert!(!line.contains("BearerSecret"));
        assert!(!line.contains("very-secret"));
        assert!(!line.contains("encoded-secret"));
        assert!(!line.contains("abc"));
        assert!(!line.contains("private"));
        assert!(!line.contains("opaque"));
    }

    #[test]
    fn preserves_non_secret_diagnostics() {
        let line = redact("session=tab-1 platform=windows cpu=12%");
        assert_eq!(line, "session=tab-1 platform=windows cpu=12%");
    }
}
