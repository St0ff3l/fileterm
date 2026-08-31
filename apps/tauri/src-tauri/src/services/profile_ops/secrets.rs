/// Read a secret only for an internal Rust execution path. Callers must never
/// forward the returned value to a renderer, snapshot, log, or agent.
fn read_profile_secret(
    app: &AppHandle,
    profile_id: &str,
    field: &str,
) -> Result<Option<String>, AppError> {
    let path = workspace_file(app, "profile-secrets.json")?;
    if path.exists() {
        lock_down_secret_file(&path)?;
    }
    let profiles = read_json_array(app, "profiles.json")?;
    if !profiles
        .iter()
        .any(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
    {
        return Err(AppError::Storage("Profile not found".to_string()));
    }
    let Some(mut store) = read_profile_secret_store(&path)? else {
        return Ok(None);
    };
    let Some(stored_value) =
        current_profile_secret(Some(&store), profile_id, field).and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let storage_root = profile_secret_storage_root(&path)?;
    let scope = format!("profile/{profile_id}/{field}");
    let (value, should_migrate) =
        crate::services::secret_crypto::decrypt_or_migrate(storage_root, &scope, stored_value)?;
    if should_migrate {
        let encrypted = crate::services::secret_crypto::encrypt(storage_root, &scope, &value)?;
        if let Some(secret) = store
            .get_mut("profiles")
            .and_then(Value::as_object_mut)
            .and_then(|profiles| profiles.get_mut(profile_id))
            .and_then(Value::as_object_mut)
            .and_then(|profile| profile.get_mut(field))
            .and_then(Value::as_object_mut)
        {
            secret.insert(
                "storage".to_string(),
                Value::String(crate::services::secret_crypto::ENCRYPTED_STORAGE.to_string()),
            );
            secret.insert("value".to_string(), Value::String(encrypted));
            let content = serde_json::to_vec_pretty(&store)
                .map_err(|error| AppError::Serialization(error.to_string()))?;
            write_secure_secret_file(&path, &content)?;
        }
    }
    Ok(Some(value))
}

fn hydrate_profile_secrets(path: &std::path::Path, profiles: &mut [Value]) -> Result<(), AppError> {
    let Some(store) = read_profile_secret_store(path)? else {
        return Ok(());
    };
    let storage_root = profile_secret_storage_root(path)?;
    for profile in profiles {
        let Some(profile_id) = profile.get("id").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        let Some(profile_object) = profile.as_object_mut() else {
            continue;
        };
        for field in [
            "password",
            "passphrase",
            "privateKeyPath",
            "sudoPassword",
            "suPassword",
        ] {
            let Some(stored_value) =
                current_profile_secret(Some(&store), &profile_id, field).and_then(Value::as_str)
            else {
                continue;
            };
            let scope = format!("profile/{profile_id}/{field}");
            let (value, _) = crate::services::secret_crypto::decrypt_or_migrate(
                storage_root,
                &scope,
                stored_value,
            )?;
            profile_object.insert(field.to_string(), Value::String(value));
        }
        if let Some(stored_value) =
            current_profile_secret(Some(&store), &profile_id, "proxyPassword")
                .and_then(Value::as_str)
        {
            let scope = format!("profile/{profile_id}/proxyPassword");
            let (value, _) = crate::services::secret_crypto::decrypt_or_migrate(
                storage_root,
                &scope,
                stored_value,
            )?;
            if let Some(proxy) = profile_object
                .entry("proxy")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
            {
                proxy.insert("password".to_string(), Value::String(value));
            }
        }
    }
    Ok(())
}

pub fn read_sudo_password(app: &AppHandle, profile_id: &str) -> Result<Option<String>, AppError> {
    read_profile_secret(app, profile_id, "sudoPassword")
}

pub fn read_su_password(app: &AppHandle, profile_id: &str) -> Result<Option<String>, AppError> {
    read_profile_secret(app, profile_id, "suPassword")
}

fn set_profile_secret(
    app: &AppHandle,
    profile_id: &str,
    field: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    if !matches!(field, "sudoPassword" | "suPassword") {
        return Err(AppError::Command(
            "Unsupported privileged profile secret".to_string(),
        ));
    }
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
    let object = profile
        .as_object_mut()
        .ok_or_else(|| AppError::Storage("Profile shape is invalid".to_string()))?;
    match value.filter(|value| !value.is_empty()) {
        Some(value) => {
            object.insert(field.to_string(), Value::String(value.to_string()));
        }
        None => {
            object.remove(field);
        }
    }
    persist_profiles(app, &profiles)
}

pub fn set_sudo_password(
    app: &AppHandle,
    profile_id: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    set_profile_secret(app, profile_id, "sudoPassword", value)
}

pub fn set_su_password(
    app: &AppHandle,
    profile_id: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    set_profile_secret(app, profile_id, "suPassword", value)
}

/// Delete a profile by id.
pub fn delete_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    profiles.retain(|p| p.get("id").and_then(|v| v.as_str()) != Some(profile_id));
    persist_profiles(app, &profiles)
}

/// Record a successful user-initiated open so renderer "recent connections"
/// can use the same persisted `lastUsedAt` ordering as Electron.
pub fn touch_profile(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let mut found = false;
    for profile in &mut profiles {
        if profile.get("id").and_then(Value::as_str) == Some(profile_id) {
            if let Some(object) = profile.as_object_mut() {
                object.insert(
                    "lastUsedAt".to_string(),
                    Value::Number(chrono_now_ms().into()),
                );
                found = true;
            }
            break;
        }
    }
    if !found {
        return Err(AppError::Storage("Profile not found".to_string()));
    }
    let stripped: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    write_json_array(app, "profiles.json", &stripped)
}

/// Update only the `trustedHostFingerprint` field on a profile. Called from
/// the SSH worker's `check_server_key` when the user picks "accept-and-save".
/// This avoids clobbering other profile fields (which a full `update_profile`
/// would require) and is safe to call from the worker context.
pub async fn update_trusted_host_fingerprint(
    app: &AppHandle,
    profile_id: &str,
    fingerprint: &str,
) -> Result<(), AppError> {
    crate::services::logging::info(app, "profile", "saving trusted host fingerprint");
    let app = app.clone();
    let profile_id = profile_id.to_string();
    let fingerprint = fingerprint.to_string();
    tokio::task::spawn_blocking(move || {
        let (mut profiles, _) = read_and_heal_profiles(&app)?;
        let mut found = false;
        if let Some(profile) = profiles
            .iter_mut()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(profile_id.as_str()))
        {
            if let Some(obj) = profile.as_object_mut() {
                obj.insert(
                    "trustedHostFingerprint".to_string(),
                    Value::String(fingerprint.clone()),
                );
                found = true;
            }
        }
        crate::services::logging::debug(
            &app,
            "profile",
            format!("trusted host fingerprint profile_found={found}"),
        );
        persist_profiles(&app, &profiles)
    })
    .await
    .map_err(|e| AppError::Storage(format!("join error: {}", e)))?
}

/// Explicitly clear the SSH host key trust record for a profile.
pub fn clear_trusted_host_fingerprint(app: &AppHandle, profile_id: &str) -> Result<(), AppError> {
    let (mut profiles, _) = read_and_heal_profiles(app)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| profile.get("id").and_then(Value::as_str) == Some(profile_id))
        .ok_or_else(|| AppError::Storage("Profile not found".to_string()))?;
    let object = profile
        .as_object_mut()
        .ok_or_else(|| AppError::Storage("Profile is invalid".to_string()))?;
    object.insert(
        "trustedHostFingerprint".to_string(),
        Value::String(String::new()),
    );
    persist_profiles(app, &profiles)
}

fn profile_secret_storage_root(path: &std::path::Path) -> Result<&std::path::Path, AppError> {
    path.parent()
        .ok_or_else(|| AppError::Storage("无法解析连接凭据存储目录".to_string()))
}

fn current_profile_secret<'a>(
    current: Option<&'a Value>,
    profile_id: &str,
    field: &str,
) -> Option<&'a Value> {
    current?
        .get("profiles")?
        .get(profile_id)?
        .get(field)?
        .get("value")
}

fn encrypted_profile_secret(
    storage_root: &std::path::Path,
    current: Option<&Value>,
    profile_id: &str,
    field: &str,
    plaintext: &str,
) -> Result<Value, AppError> {
    let scope = format!("profile/{profile_id}/{field}");
    if let Some(existing) = current_profile_secret(current, profile_id, field)
        .and_then(Value::as_str)
        .filter(|existing| crate::services::secret_crypto::is_encrypted(existing))
    {
        if crate::services::secret_crypto::decrypt(storage_root, &scope, existing)? == plaintext {
            return Ok(serde_json::json!({
                "storage": crate::services::secret_crypto::ENCRYPTED_STORAGE,
                "value": existing,
            }));
        }
    }
    Ok(serde_json::json!({
        "storage": crate::services::secret_crypto::ENCRYPTED_STORAGE,
        "value": crate::services::secret_crypto::encrypt(storage_root, &scope, plaintext)?,
    }))
}

/// Build the complete secret store from the current profile set. Rebuilding
/// instead of incrementally merging guarantees that deleted profiles cannot
/// leave orphan credentials behind. Existing matching ciphertext is reused so
/// normal reads do not rotate values just because AES-GCM has a fresh nonce.
fn build_profile_secrets(
    path: &std::path::Path,
    profiles: &[Value],
    current: Option<&Value>,
) -> Result<Value, AppError> {
    let storage_root = profile_secret_storage_root(path)?;
    let mut secrets_profiles = Map::new();
    for profile in profiles {
        let id = match profile.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let mut entry = Map::new();
        for key in [
            "password",
            "passphrase",
            "privateKeyPath",
            "sudoPassword",
            "suPassword",
        ] {
            if let Some(value) = profile.get(key).and_then(Value::as_str) {
                entry.insert(
                    key.to_string(),
                    encrypted_profile_secret(storage_root, current, &id, key, value)?,
                );
            }
        }
        if let Some(proxy) = profile.get("proxy").and_then(|v| v.as_object()) {
            if let Some(password) = proxy.get("password").and_then(Value::as_str) {
                entry.insert(
                    "proxyPassword".to_string(),
                    encrypted_profile_secret(
                        storage_root,
                        current,
                        &id,
                        "proxyPassword",
                        password,
                    )?,
                );
            }
        }
        if !entry.is_empty() {
            secrets_profiles.insert(id, Value::Object(entry));
        }
    }

    Ok(serde_json::json!({
        "version": 1,
        "profiles": secrets_profiles,
    }))
}

#[cfg(unix)]
fn lock_down_secret_file(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_secret_file(_path: &std::path::Path) -> Result<(), AppError> {
    // Windows ACL semantics are inherited from the per-user app-data
    // directory. Keep this best-effort behavior aligned with Electron.
    Ok(())
}

fn remove_file_if_present(path: &std::path::Path) -> Result<(), AppError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn write_secure_secret_file(path: &std::path::Path, content: &[u8]) -> Result<(), AppError> {
    use std::io::Write;

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let nonce = uuid::Uuid::new_v4();
    let temp_path = path.with_file_name(format!(".{file_name}.{nonce}.tmp"));
    let backup_path = path.with_file_name(format!(".{file_name}.{nonce}.bak"));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut temp_file = options
        .open(&temp_path)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    if let Err(error) = temp_file
        .write_all(content)
        .and_then(|_| temp_file.sync_all())
    {
        drop(temp_file);
        let cleanup_error = remove_file_if_present(&temp_path).err();
        return Err(AppError::Storage(match cleanup_error {
            Some(cleanup_error) => format!("{error}; 清理凭据临时文件失败: {cleanup_error}"),
            None => error.to_string(),
        }));
    }
    drop(temp_file);
    if let Err(error) = lock_down_secret_file(&temp_path) {
        let cleanup_error = remove_file_if_present(&temp_path).err();
        return Err(AppError::Storage(match cleanup_error {
            Some(cleanup_error) => format!("{error}; 清理凭据临时文件失败: {cleanup_error}"),
            None => error.to_string(),
        }));
    }

    let had_previous = path.exists();
    if had_previous {
        if let Err(error) = std::fs::rename(path, &backup_path) {
            let cleanup_error = remove_file_if_present(&temp_path).err();
            return Err(AppError::Storage(match cleanup_error {
                Some(cleanup_error) => {
                    format!("{error}; 清理凭据临时文件失败: {cleanup_error}")
                }
                None => error.to_string(),
            }));
        }
    }

    if let Err(error) = std::fs::rename(&temp_path, path) {
        let restore_error = if had_previous {
            std::fs::rename(&backup_path, path).err()
        } else {
            None
        };
        let _ = remove_file_if_present(&temp_path);
        return Err(AppError::Storage(match restore_error {
            Some(restore_error) => format!("{error}; 恢复原凭据文件失败: {restore_error}"),
            None => error.to_string(),
        }));
    }

    if let Err(error) = lock_down_secret_file(path) {
        let remove_error = remove_file_if_present(path).err();
        let restore_error = if had_previous {
            std::fs::rename(&backup_path, path).err()
        } else {
            None
        };
        return match (remove_error, restore_error) {
            (None, None) => Err(error),
            (remove_error, restore_error) => Err(AppError::Storage(format!(
                "{error}; 清理失败: {}; 恢复失败: {}",
                remove_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "无".to_string()),
                restore_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "无".to_string())
            ))),
        };
    }

    remove_file_if_present(&backup_path)?;
    Ok(())
}

fn read_profile_secret_store(path: &std::path::Path) -> Result<Option<Value>, AppError> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map(Some)
            .map_err(|error| AppError::Serialization(error.to_string())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn persist_profile_secrets_at(path: &std::path::Path, profiles: &[Value]) -> Result<(), AppError> {
    let current = read_profile_secret_store(path)?;
    let content =
        serde_json::to_vec_pretty(&build_profile_secrets(path, profiles, current.as_ref())?)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
    write_secure_secret_file(path, &content)
}

fn persist_profile_secrets(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, "profile-secrets.json")?;
    persist_profile_secrets_at(&path, profiles)
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<Vec<u8>>, AppError> {
    match std::fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn restore_secret_file(path: &std::path::Path, content: Option<&[u8]>) -> Result<(), AppError> {
    match content {
        Some(content) => write_secure_secret_file(path, content),
        None => remove_file_if_present(path),
    }
}

/// Persist both halves of the profile store. Secrets are written first; if
/// the public profile write fails, the previous secret file is restored so a
/// failed operation cannot silently strand a profile without its credentials.
fn persist_profiles(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let secrets_path = workspace_file(app, "profile-secrets.json")?;
    let previous_secrets = read_optional_file(&secrets_path)?;
    persist_profile_secrets(app, profiles)?;

    let public_profiles: Vec<Value> = profiles.iter().map(strip_secret_fields).collect();
    if let Err(public_error) = write_json_array(app, "profiles.json", &public_profiles) {
        return match restore_secret_file(&secrets_path, previous_secrets.as_deref()) {
            Ok(()) => Err(public_error),
            Err(rollback_error) => Err(AppError::Storage(format!(
                "{public_error}; 恢复凭据文件失败: {rollback_error}"
            ))),
        };
    }
    Ok(())
}

/// Heal legacy modes and prune stale secret IDs on normal profile reads.
fn reconcile_profile_secrets(app: &AppHandle, profiles: &[Value]) -> Result<(), AppError> {
    let path = workspace_file(app, "profile-secrets.json")?;
    let current = read_profile_secret_store(&path)?;
    let expected = build_profile_secrets(&path, profiles, current.as_ref())?;

    let expected_has_secrets = expected["profiles"]
        .as_object()
        .is_some_and(|profiles| !profiles.is_empty());
    if current.as_ref() != Some(&expected) && (path.exists() || expected_has_secrets) {
        let content = serde_json::to_vec_pretty(&expected)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        write_secure_secret_file(&path, &content)?;
    }
    if path.exists() {
        lock_down_secret_file(&path)?;
    }
    Ok(())
}

fn chrono_now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
