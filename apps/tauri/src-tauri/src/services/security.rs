//! Local session security and the shared remote-backup password.
//!
//! The renderer only receives status flags. Passwords are kept in this
//! device-bound store and are never returned through the Tauri API. The
//! portable storage root is selected by `storage::workspace_file`, so this
//! file follows the portable `config` directory on Windows.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use zeroize::{Zeroize, Zeroizing};

use crate::AppError;

pub(crate) const SECURITY_FILE: &str = "security.json";
pub(crate) const BACKUP_PASSWORD_REQUIRED_ERROR: &str = "SECURITY_BACKUP_PASSWORD_REQUIRED";
const MAX_IDLE_LOCK_MINUTES: u32 = 24 * 60;
const MIN_LOCK_PASSWORD_CHARS: usize = 4;
const MAX_PASSWORD_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSecurityConfig {
    #[serde(default)]
    lock_enabled: bool,
    #[serde(default)]
    idle_lock_minutes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lock_password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_password: Option<String>,
}

impl StoredSecurityConfig {
    fn clear_secrets(&mut self) {
        if let Some(password) = self.lock_password.as_mut() {
            password.zeroize();
        }
        if let Some(password) = self.backup_password.as_mut() {
            password.zeroize();
        }
        self.lock_password = None;
        self.backup_password = None;
    }
}

impl Drop for StoredSecurityConfig {
    fn drop(&mut self) {
        self.clear_secrets();
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySettings {
    pub lock_enabled: bool,
    pub idle_lock_minutes: u32,
    pub has_lock_password: bool,
    pub has_backup_password: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySettingsInput {
    pub lock_enabled: Option<bool>,
    pub idle_lock_minutes: Option<u32>,
    pub lock_password: Option<String>,
    pub backup_password: Option<String>,
    pub clear_lock_password: Option<bool>,
    pub clear_backup_password: Option<bool>,
}

fn config_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    crate::storage::workspace_file(app, SECURITY_FILE)
}

fn normalize_config(config: &mut StoredSecurityConfig) -> bool {
    let mut changed = false;
    if config.idle_lock_minutes > MAX_IDLE_LOCK_MINUTES {
        config.idle_lock_minutes = MAX_IDLE_LOCK_MINUTES;
        changed = true;
    }
    if config.lock_enabled && config.lock_password.is_none() {
        config.lock_enabled = false;
        changed = true;
    }
    changed
}

fn decrypt_optional(
    storage_root: &Path,
    scope: &str,
    value: &mut Option<String>,
) -> Result<bool, AppError> {
    let Some(stored) = value.as_ref() else {
        return Ok(false);
    };
    let (cleartext, should_migrate) =
        crate::services::secret_crypto::decrypt_or_migrate(storage_root, scope, stored)?;
    if cleartext.is_empty() {
        *value = None;
        return Ok(true);
    }
    *value = Some(cleartext);
    Ok(should_migrate)
}

fn read_config_at(path: &Path) -> Result<(StoredSecurityConfig, bool), AppError> {
    if !path.exists() {
        return Ok((StoredSecurityConfig::default(), false));
    }
    lock_down_config_file(path)?;
    let content = fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut config: StoredSecurityConfig = serde_json::from_str(&content)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析安全设置存储目录".to_string()))?;
    let mut changed = false;
    changed |= decrypt_optional(
        storage_root,
        "security/lock-password",
        &mut config.lock_password,
    )?;
    changed |= decrypt_optional(
        storage_root,
        "security/backup-password",
        &mut config.backup_password,
    )?;
    changed |= normalize_config(&mut config);
    Ok((config, changed))
}

fn read_config(app: &AppHandle) -> Result<StoredSecurityConfig, AppError> {
    let path = config_path(app)?;
    let (config, migrated) = read_config_at(&path)?;
    if migrated {
        write_config_at(&path, &config)?;
    }
    Ok(config)
}

fn write_config_at(path: &Path, config: &StoredSecurityConfig) -> Result<(), AppError> {
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析安全设置存储目录".to_string()))?;
    fs::create_dir_all(storage_root).map_err(|error| AppError::Storage(error.to_string()))?;
    let temporary = path.with_file_name(format!(".security.json.{}.tmp", uuid::Uuid::new_v4()));
    let mut encrypted = config.clone();
    if let Some(password) = encrypted.lock_password.as_mut() {
        *password = crate::services::secret_crypto::encrypt(
            storage_root,
            "security/lock-password",
            password,
        )?;
    }
    if let Some(password) = encrypted.backup_password.as_mut() {
        *password = crate::services::secret_crypto::encrypt(
            storage_root,
            "security/backup-password",
            password,
        )?;
    }
    let content = serde_json::to_vec_pretty(&encrypted)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&temporary, &content)?;
    if let Err(error) = lock_down_config_file(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    crate::storage::replace_file_atomically(&temporary, path)?;
    lock_down_config_file(path)
}

#[cfg(unix)]
fn lock_down_config_file(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_config_file(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn snapshot(config: &StoredSecurityConfig) -> SecuritySettings {
    SecuritySettings {
        lock_enabled: config.lock_enabled,
        idle_lock_minutes: config.idle_lock_minutes,
        has_lock_password: config.lock_password.is_some(),
        has_backup_password: config.backup_password.is_some(),
    }
}

fn validate_lock_password(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_LOCK_PASSWORD_CHARS {
        return Err(AppError::Command(format!(
            "锁屏密码至少需要 {MIN_LOCK_PASSWORD_CHARS} 个字符。"
        )));
    }
    if password.is_empty()
        || password.len() > MAX_PASSWORD_BYTES
        || password
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n' | '\u{1b}'))
    {
        return Err(AppError::Command("锁屏密码格式无效。".to_string()));
    }
    Ok(())
}

pub(crate) fn get_settings(app: &AppHandle) -> Result<SecuritySettings, AppError> {
    let mut config = read_config(app)?;
    let settings = snapshot(&config);
    config.clear_secrets();
    Ok(settings)
}

pub(crate) fn save_settings(
    app: &AppHandle,
    mut input: SecuritySettingsInput,
) -> Result<SecuritySettings, AppError> {
    let path = config_path(app)?;
    let (mut config, _) = read_config_at(&path)?;

    if input.clear_lock_password == Some(true) {
        config.lock_password = None;
    }
    if input.clear_backup_password == Some(true) {
        config.backup_password = None;
    }
    if let Some(password) = input.lock_password.take() {
        let password = Zeroizing::new(password);
        validate_lock_password(&password)?;
        config.lock_password = Some(password.to_string());
    }
    if let Some(password) = input.backup_password.take() {
        let password = Zeroizing::new(password);
        crate::services::backup_crypto::validate_password(&password)
            .map_err(|error| AppError::Command(error.to_string()))?;
        config.backup_password = Some(password.to_string());
    }
    if let Some(lock_enabled) = input.lock_enabled {
        config.lock_enabled = lock_enabled;
    }
    if let Some(idle_lock_minutes) = input.idle_lock_minutes {
        config.idle_lock_minutes = idle_lock_minutes.min(MAX_IDLE_LOCK_MINUTES);
    }
    if config.lock_enabled && config.lock_password.is_none() {
        return Err(AppError::Command(
            "SECURITY_LOCK_PASSWORD_REQUIRED".to_string(),
        ));
    }

    normalize_config(&mut config);
    write_config_at(&path, &config)?;
    let settings = snapshot(&config);
    crate::services::logging::info(
        app,
        "security",
        format!(
            "settings saved lock_enabled={} idle_lock_minutes={} has_lock_password={} has_backup_password={}",
            settings.lock_enabled,
            settings.idle_lock_minutes,
            settings.has_lock_password,
            settings.has_backup_password
        ),
    );
    let _ = app.emit("app:security-settings-changed", &settings);
    config.clear_secrets();
    Ok(settings)
}

pub(crate) fn verify_lock_password(app: &AppHandle, password: &str) -> Result<bool, AppError> {
    let mut config = read_config(app)?;
    let matches = config
        .lock_password
        .as_deref()
        .is_some_and(|saved| !password.is_empty() && saved == password);
    config.clear_secrets();
    Ok(matches)
}

pub(crate) fn backup_password(app: &AppHandle) -> Result<Zeroizing<String>, AppError> {
    let mut config = read_config(app)?;
    let password = config.backup_password.take();
    config.clear_secrets();
    password
        .map(Zeroizing::new)
        .ok_or_else(|| AppError::Command(BACKUP_PASSWORD_REQUIRED_ERROR.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("fileterm-security-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        directory.join(SECURITY_FILE)
    }

    #[test]
    fn lock_password_requires_four_characters() {
        assert!(validate_lock_password("123").is_err());
        assert!(validate_lock_password("1234").is_ok());
        assert!(validate_lock_password("line\nbreak").is_err());
    }

    #[test]
    fn security_secrets_are_encrypted_at_rest_and_round_trip() {
        let path = fixture_path();
        let config = StoredSecurityConfig {
            lock_enabled: true,
            idle_lock_minutes: 10,
            lock_password: Some("lock-password".to_string()),
            backup_password: Some("Backup password 8".to_string()),
        };
        write_config_at(&path, &config).expect("security config should be written");
        let raw = fs::read_to_string(&path).expect("security config should be readable");
        assert!(!raw.contains("lock-password"));
        assert!(!raw.contains("Backup password 8"));

        let (decoded, migrated) = read_config_at(&path).expect("security config should decrypt");
        assert!(!migrated);
        assert_eq!(decoded.lock_password.as_deref(), Some("lock-password"));
        assert_eq!(
            decoded.backup_password.as_deref(),
            Some("Backup password 8")
        );
        assert_eq!(snapshot(&decoded).idle_lock_minutes, 10);
    }

    #[test]
    fn plaintext_security_secrets_are_migrated() {
        let path = fixture_path();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "lockEnabled": true,
                "idleLockMinutes": 5,
                "lockPassword": "legacy-lock",
                "backupPassword": "Backup password 8"
            }))
            .expect("legacy config should serialize"),
        )
        .expect("legacy config should be written");

        let (decoded, migrated) = read_config_at(&path).expect("legacy config should load");
        assert!(migrated);
        assert_eq!(decoded.lock_password.as_deref(), Some("legacy-lock"));
        write_config_at(&path, &decoded).expect("legacy config should be re-encrypted");
        let raw = fs::read_to_string(&path).expect("migrated config should be readable");
        assert!(!raw.contains("legacy-lock"));
    }
}
