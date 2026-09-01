fn commit_pending_files(pending: &[PendingFile]) -> Result<(), AppError> {
    let mut committed: Vec<(&Path, &Path)> = Vec::new();
    for file in pending {
        if let Some(parent) = file.target.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                rollback_committed(&committed);
                AppError::Storage(error.to_string())
            })?;
            if file.confidential {
                if let Err(error) = lock_down_directory(parent) {
                    rollback_committed(&committed);
                    return Err(error);
                }
            }
        }
        let had_current = file.target.exists();
        if had_current {
            if let Some(parent) = file.backup.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    rollback_committed(&committed);
                    AppError::Storage(error.to_string())
                })?;
                if let Err(error) = lock_down_directory(parent) {
                    rollback_committed(&committed);
                    return Err(error);
                }
            }
            if let Err(error) = fs::rename(&file.target, &file.backup) {
                rollback_committed(&committed);
                return Err(AppError::Storage(error.to_string()));
            }
        }
        if let Err(error) = fs::rename(&file.staged, &file.target) {
            if had_current {
                let _ = fs::rename(&file.backup, &file.target);
            }
            rollback_committed(&committed);
            return Err(AppError::Storage(error.to_string()));
        }
        if file.confidential {
            if let Err(error) = lock_down_file(&file.target) {
                let _ = fs::remove_file(&file.target);
                if had_current {
                    let _ = fs::rename(&file.backup, &file.target);
                }
                rollback_committed(&committed);
                return Err(error);
            }
        }
        committed.push((&file.target, &file.backup));
    }
    Ok(())
}

fn rollback_committed(committed: &[(&Path, &Path)]) {
    for (target, backup) in committed.iter().rev() {
        let _ = fs::remove_file(target);
        if backup.exists() {
            let _ = fs::rename(backup, target);
        }
    }
}

fn read_optional_json_file(path: &Path) -> Result<Option<Value>, AppError> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| AppError::Serialization(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let content = fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_str(&content).map_err(|error| AppError::Serialization(error.to_string()))
}

#[cfg(unix)]
pub(crate) fn write_restricted_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    result.map_err(|error| restricted_write_error(path, error))
}

#[cfg(not(unix))]
pub(crate) fn write_restricted_file(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    result.map_err(|error| restricted_write_error(path, error))
}

fn restricted_write_error(path: &Path, error: std::io::Error) -> AppError {
    match fs::remove_file(path) {
        Ok(()) => AppError::Storage(error.to_string()),
        Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
            AppError::Storage(error.to_string())
        }
        Err(cleanup_error) => {
            AppError::Storage(format!("{error}; 清理受限临时文件失败: {cleanup_error}"))
        }
    }
}

#[cfg(unix)]
fn lock_down_file(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_file(_path: &Path) -> Result<(), AppError> {
    // Windows relies on the per-user application-data directory ACL. A
    // platform-specific restricted ACL remains a release acceptance item.
    Ok(())
}

#[cfg(unix)]
fn lock_down_directory(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_directory(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn read_json_object(app: &AppHandle, name: &str) -> Result<Value, AppError> {
    let path = workspace_file(app, name)?;
    if !path.exists() {
        return Ok(serde_json::json!({}));
    }
    read_json_file(&path)
}

pub fn read_json_array(app: &AppHandle, name: &str) -> Result<Vec<Value>, AppError> {
    let path = workspace_file(app, name)?;
    let mut values: Vec<Value> = if path.exists() {
        read_json_file(&path)?
    } else {
        Vec::new()
    };

    if name == "profiles.json" {
        let secrets_path = workspace_file(app, "profile-secrets.json")?;
        let storage_root = secrets_path
            .parent()
            .ok_or_else(|| AppError::Storage("无法解析连接凭据存储目录".to_string()))?;
        let mut secrets = if secrets_path.exists() {
            read_json_file(&secrets_path)?
        } else {
            serde_json::json!({})
        };
        let mut migrated = false;
        if let Some(secrets_profiles) = secrets.get_mut("profiles").and_then(Value::as_object_mut) {
            for profile in &mut values {
                if let Some(profile_obj) = profile.as_object_mut() {
                    if let Some(profile_id) = profile_obj
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                    {
                        if let Some(profile_secrets) = secrets_profiles
                            .get_mut(&profile_id)
                            .and_then(Value::as_object_mut)
                        {
                            let (password, password_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "password",
                                profile_secrets,
                            )?;
                            migrated |= password_migrated;
                            if let Some(password) = password {
                                profile_obj.insert("password".to_string(), Value::String(password));
                            }
                            let (passphrase, passphrase_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "passphrase",
                                profile_secrets,
                            )?;
                            migrated |= passphrase_migrated;
                            if let Some(passphrase) = passphrase {
                                profile_obj
                                    .insert("passphrase".to_string(), Value::String(passphrase));
                            }
                            let (private_key_path, private_key_path_migrated) =
                                profile_secret_value(
                                    storage_root,
                                    &profile_id,
                                    "privateKeyPath",
                                    profile_secrets,
                                )?;
                            migrated |= private_key_path_migrated;
                            if let Some(private_key_path) = private_key_path {
                                profile_obj.insert(
                                    "privateKeyPath".to_string(),
                                    Value::String(private_key_path),
                                );
                            }
                            let (proxy_password, proxy_password_migrated) = profile_secret_value(
                                storage_root,
                                &profile_id,
                                "proxyPassword",
                                profile_secrets,
                            )?;
                            migrated |= proxy_password_migrated;
                            if let Some(proxy_password) = proxy_password {
                                if let Some(proxy_obj) =
                                    profile_obj.get_mut("proxy").and_then(Value::as_object_mut)
                                {
                                    proxy_obj.insert(
                                        "password".to_string(),
                                        Value::String(proxy_password),
                                    );
                                }
                            }
                            for field in ["sudoPassword", "suPassword"] {
                                let (password, password_migrated) = profile_secret_value(
                                    storage_root,
                                    &profile_id,
                                    field,
                                    profile_secrets,
                                )?;
                                migrated |= password_migrated;
                                if let Some(password) = password {
                                    profile_obj.insert(field.to_string(), Value::String(password));
                                }
                            }
                        }
                    }
                }
            }
        }
        if migrated {
            write_restricted_json(&secrets_path, &secrets)?;
        }
    }

    Ok(values)
}

fn profile_secret_value(
    storage_root: &Path,
    profile_id: &str,
    field: &str,
    profile_secrets: &mut Map<String, Value>,
) -> Result<(Option<String>, bool), AppError> {
    let Some(value) = profile_secrets
        .get(field)
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
    else {
        return Ok((None, false));
    };
    let scope = format!("profile/{profile_id}/{field}");
    let (plaintext, should_migrate) =
        crate::services::secret_crypto::decrypt_or_migrate(storage_root, &scope, &value)?;
    if should_migrate {
        let encrypted = crate::services::secret_crypto::encrypt(storage_root, &scope, &plaintext)?;
        if let Some(entry) = profile_secrets
            .get_mut(field)
            .and_then(Value::as_object_mut)
        {
            entry.insert(
                "storage".to_string(),
                Value::String(crate::services::secret_crypto::ENCRYPTED_STORAGE.to_string()),
            );
            entry.insert("value".to_string(), Value::String(encrypted));
        }
    }
    Ok((Some(plaintext), should_migrate))
}

fn write_restricted_json(path: &Path, value: &Value) -> Result<(), AppError> {
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&temporary, &content)?;
    if let Err(error) = replace_file_atomically(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

pub fn write_json_array(app: &AppHandle, name: &str, values: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, name)?;
    let temp_path = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let content = serde_json::to_string_pretty(values)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    fs::write(&temp_path, content).map_err(|error| AppError::Storage(error.to_string()))?;
    replace_file_atomically(&temp_path, &path)
}

/// Replace a file using same-directory staging and rollback. Moving the
/// current target aside first keeps this path compatible with Windows, where
/// `rename(staged, existing_target)` does not replace the target.
pub fn replace_file_atomically(staged: &Path, target: &Path) -> Result<(), AppError> {
    let backup = target.with_file_name(format!(
        ".{}.{}.bak",
        target.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let had_current = target.exists();
    if had_current {
        if let Err(error) = fs::rename(target, &backup) {
            let _ = fs::remove_file(staged);
            return Err(AppError::Storage(error.to_string()));
        }
    }

    if let Err(error) = fs::rename(staged, target) {
        let restore_error = if had_current {
            fs::rename(&backup, target).err()
        } else {
            None
        };
        let _ = fs::remove_file(staged);
        return Err(AppError::Storage(match restore_error {
            Some(restore_error) => {
                format!("{error}; 恢复原文件失败: {restore_error}")
            }
            None => error.to_string(),
        }));
    }

    if had_current {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}
