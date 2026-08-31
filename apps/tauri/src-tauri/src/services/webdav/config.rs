fn command_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
}

fn config_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    workspace_file(app, "webdav-sync.json")
}

fn normalize_base_url(value: &str, allow_insecure_tls: bool) -> Result<String, AppError> {
    let mut url = Url::parse(value.trim()).map_err(|_| command_error("WebDAV 地址无效"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(command_error("WebDAV 地址不得内嵌用户名或密码"));
    }
    if url.fragment().is_some() {
        return Err(command_error("WebDAV 地址不得包含片段"));
    }
    if url.scheme() != "https" && !(allow_insecure_tls && url.scheme() == "http") {
        return Err(command_error(
            "WebDAV 地址必须使用 HTTPS；HTTP 需要明确启用高风险选项。",
        ));
    }
    url.set_query(None);
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn normalize_remote_path(value: &str) -> Result<String, AppError> {
    let path = value.trim().trim_start_matches('/');
    if path.is_empty()
        || path
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(command_error("WebDAV 远端路径无效"));
    }
    Ok(path.to_string())
}

fn remote_url(config: &StoredConfig) -> Result<Url, AppError> {
    let base = normalize_base_url(&config.url, config.allow_insecure_tls == Some(true))?;
    Url::parse(&(base + "/"))
        .map_err(|error| command_error(error.to_string()))?
        .join(&normalize_remote_path(&config.remote_path)?)
        .map_err(|error| command_error(format!("WebDAV 远端路径无效: {error}")))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn public_config(config: &StoredConfig) -> Value {
    serde_json::json!({
        "enabled": config.enabled,
        "url": config.url,
        "username": config.username,
        "remotePath": config.remote_path,
        "allowInsecureTls": config.allow_insecure_tls == Some(true),
        "lastSyncedAt": config.last_synced_at,
        "lastEtag": config.last_etag,
    })
}

fn read_config(app: &AppHandle) -> Result<StoredConfig, AppError> {
    let path = config_path(app)?;
    let (config, migrated) = read_config_at(&path)?;
    if migrated {
        write_config_at(&path, &config)?;
    }
    Ok(config)
}

fn read_config_at(path: &Path) -> Result<(StoredConfig, bool), AppError> {
    if !path.exists() {
        return Ok((StoredConfig::default(), false));
    }
    lock_down_config_file(path)?;
    let content = fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut config: StoredConfig = serde_json::from_str(&content)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析 WebDAV 凭据存储目录".to_string()))?;
    let mut migrated = false;
    if let Some(password) = config.password.as_mut() {
        let (value, should_migrate) = crate::services::secret_crypto::decrypt_or_migrate(
            storage_root,
            "webdav/password",
            password,
        )?;
        *password = value;
        migrated = should_migrate;
    }
    if config.remote_path.trim().is_empty() {
        config.remote_path = DEFAULT_REMOTE_PATH.to_string();
    }
    Ok((config, migrated))
}

fn write_config(app: &AppHandle, config: &StoredConfig) -> Result<(), AppError> {
    let path = config_path(app)?;
    write_config_at(&path, config)
}

fn write_config_at(path: &Path, config: &StoredConfig) -> Result<(), AppError> {
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析 WebDAV 凭据存储目录".to_string()))?;
    let temporary = path.with_file_name(format!(".webdav-sync.json.{}.tmp", uuid::Uuid::new_v4()));
    let mut encrypted = config.clone();
    if let Some(password) = encrypted.password.as_mut() {
        *password =
            crate::services::secret_crypto::encrypt(storage_root, "webdav/password", password)?;
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
fn lock_down_config_file(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_config_file(_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

fn validate_config(config: &StoredConfig, require_enabled: bool) -> Result<(), AppError> {
    if require_enabled && !config.enabled {
        return Err(command_error("请先启用 WebDAV 配置同步"));
    }
    normalize_base_url(&config.url, config.allow_insecure_tls == Some(true))?;
    normalize_remote_path(&config.remote_path)?;
    Ok(())
}

fn configured(app: &AppHandle, require_enabled: bool) -> Result<StoredConfig, AppError> {
    let config = read_config(app)?;
    validate_config(&config, require_enabled)?;
    Ok(config)
}

fn client() -> Result<Client, AppError> {
    Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| command_error(format!("无法初始化 WebDAV 客户端: {error}")))
}

fn authenticated(
    request: reqwest::RequestBuilder,
    config: &StoredConfig,
) -> reqwest::RequestBuilder {
    match config.username.as_deref() {
        Some(username) => request.basic_auth(username, config.password.as_deref()),
        None => request,
    }
}

fn response_error(action: &str, status: StatusCode) -> AppError {
    command_error(format!("WebDAV {action}失败 ({status})"))
}

fn etag(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}
