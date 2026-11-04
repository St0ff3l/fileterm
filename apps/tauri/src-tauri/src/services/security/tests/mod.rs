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
fn changing_an_existing_lock_password_requires_the_current_password() {
    assert!(authorize_lock_password_change(None, None).is_ok());
    assert!(matches!(
        authorize_lock_password_change(Some("old-password"), None),
        Err(AppError::Command(message)) if message == CURRENT_LOCK_PASSWORD_REQUIRED_ERROR
    ));
    assert!(matches!(
        authorize_lock_password_change(Some("old-password"), Some("wrong-password")),
        Err(AppError::Command(message)) if message == CURRENT_LOCK_PASSWORD_INVALID_ERROR
    ));
    assert!(authorize_lock_password_change(Some("old-password"), Some("old-password")).is_ok());
}

#[test]
fn changing_an_existing_backup_password_requires_the_current_password() {
    assert!(authorize_backup_password_change(None, None).is_ok());
    assert!(matches!(
        authorize_backup_password_change(Some("old-password"), None),
        Err(AppError::Command(message)) if message == CURRENT_BACKUP_PASSWORD_REQUIRED_ERROR
    ));
    assert!(matches!(
        authorize_backup_password_change(Some("old-password"), Some("wrong-password")),
        Err(AppError::Command(message)) if message == CURRENT_BACKUP_PASSWORD_INVALID_ERROR
    ));
    assert!(authorize_backup_password_change(Some("old-password"), Some("old-password")).is_ok());
}

#[test]
fn idle_lock_uses_the_default_and_only_allows_preset_values() {
    assert_eq!(
        StoredSecurityConfig::default().idle_lock_minutes,
        IDLE_LOCK_NEVER_MINUTES
    );
    assert!(is_supported_idle_lock_minutes(IDLE_LOCK_NEVER_MINUTES));
    assert!(is_supported_idle_lock_minutes(1));
    assert!(is_supported_idle_lock_minutes(180));
    assert!(!is_supported_idle_lock_minutes(4));
    assert!(!is_supported_idle_lock_minutes(1440));

    let mut config = StoredSecurityConfig {
        lock_enabled: false,
        idle_lock_minutes: 7,
        lock_password: None,
        backup_password: None,
    };
    assert!(normalize_config(&mut config));
    assert_eq!(config.idle_lock_minutes, DEFAULT_IDLE_LOCK_MINUTES);
}

#[test]
fn never_idle_lock_is_preserved_when_lock_protection_is_enabled() {
    let mut config = StoredSecurityConfig {
        lock_enabled: true,
        idle_lock_minutes: IDLE_LOCK_NEVER_MINUTES,
        lock_password: Some("lock-password".to_string()),
        backup_password: None,
    };

    assert!(!normalize_config(&mut config));
    assert_eq!(config.idle_lock_minutes, IDLE_LOCK_NEVER_MINUTES);
    assert!(snapshot(&config).lock_enabled);
    assert_eq!(snapshot(&config).idle_lock_minutes, IDLE_LOCK_NEVER_MINUTES);
}

#[test]
fn clearing_one_password_preserves_the_other() {
    let mut config = StoredSecurityConfig {
        lock_enabled: true,
        idle_lock_minutes: 15,
        lock_password: Some("lock-password".to_string()),
        backup_password: Some("Backup password 8".to_string()),
    };

    config.lock_enabled = false;
    config.clear_lock_password();
    normalize_config(&mut config);
    assert!(!config.lock_enabled);
    assert_eq!(config.idle_lock_minutes, DEFAULT_IDLE_LOCK_MINUTES);
    assert!(config.lock_password.is_none());
    assert_eq!(config.backup_password.as_deref(), Some("Backup password 8"));

    reset_backup_config(&mut config);
    assert!(config.backup_password.is_none());
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
