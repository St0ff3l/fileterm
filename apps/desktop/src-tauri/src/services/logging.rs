//! Small structured file logger for the Tauri runtime.
//!
//! It deliberately avoids putting credentials, bearer tokens, or private-key
//! passphrases in diagnostics. The logs are local-only and can be opened from
//! Settings through `app_open_logs_directory`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::AppHandle;

use crate::storage::state_path;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn log_directory(app: &AppHandle) -> Option<std::path::PathBuf> {
    state_path(app).ok().map(|path| path.with_file_name("logs"))
}

fn redact(message: &str) -> String {
    let pattern = regex::Regex::new(
        r"(?i)(password|passphrase|authorization|proxy[_-]?password|token)\s*([:=])\s*([^\s,;]+)",
    )
    .expect("static redaction regex");
    pattern.replace_all(message, "$1$2[REDACTED]").into_owned()
}

pub fn write(app: &AppHandle, level: &str, scope: &str, message: impl AsRef<str>) {
    let Some(directory) = log_directory(app) else {
        return;
    };
    if fs::create_dir_all(&directory).is_err() {
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
        redact(message.as_ref())
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub fn ssh_debug(app: &AppHandle, tab_id: &str, message: impl AsRef<str>) {
    write(app, "DEBUG", &format!("ssh:{tab_id}"), message);
}

#[cfg(test)]
mod tests {
    use super::redact;

    #[test]
    fn strips_common_secret_labels() {
        let line = redact("password=hunter2 Authorization: BearerSecret proxyPassword:abc");
        assert!(!line.contains("hunter2"));
        assert!(!line.contains("BearerSecret"));
        assert!(!line.contains("abc"));
    }
}
