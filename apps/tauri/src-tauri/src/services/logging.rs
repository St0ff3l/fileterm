//! Small structured file logger for the Tauri runtime.
//!
//! It deliberately avoids putting credentials, bearer tokens, or private-key
//! passphrases in diagnostics. The logs are local-only and can be opened from
//! Settings through `app_open_logs_directory`.

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
    regex::Regex::new(
        r#"(?i)(authorization["']?\s*[:=]\s*["']?(?:bearer|basic)\s+)[^\s,;"'}]+"#,
    )
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

fn append(directory: &Path, level: &str, scope: &str, message: &str) {
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
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let line = format!(
        "{timestamp} [{level}] [{scope}] {}\n",
        redact(message)
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn write(app: &AppHandle, level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = log_directory(app) else {
        return;
    };
    append(&directory, level, scope, message.as_ref());
}

pub fn write_global(level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = LOG_DIRECTORY.get() else {
        return;
    };
    append(directory, level, scope, message.as_ref());
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
