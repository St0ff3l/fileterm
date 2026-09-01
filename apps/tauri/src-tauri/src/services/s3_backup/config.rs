fn command_error(message: impl Into<String>) -> AppError {
    AppError::Command(message.into())
}

fn config_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    workspace_file(app, "s3-backup.json")
}

fn normalize_provider(value: Option<&str>, endpoint: &str) -> String {
    let value = value.unwrap_or_default().trim().to_ascii_lowercase();
    if matches!(
        value.as_str(),
        PROVIDER_CUSTOM | PROVIDER_CLOUDFLARE_R2 | PROVIDER_BITIFUL_S4
    ) {
        return value;
    }
    if endpoint
        .to_ascii_lowercase()
        .contains(".r2.cloudflarestorage.com")
    {
        PROVIDER_CLOUDFLARE_R2.to_string()
    } else if endpoint.to_ascii_lowercase().contains("s3.bitiful.net") {
        PROVIDER_BITIFUL_S4.to_string()
    } else {
        PROVIDER_CUSTOM.to_string()
    }
}

fn apply_provider_defaults(config: &mut StoredConfig) {
    match config.provider.as_str() {
        PROVIDER_CLOUDFLARE_R2 => {
            config.region = "auto".to_string();
            config.path_style_access_enabled = true;
        }
        PROVIDER_BITIFUL_S4 => {
            config.endpoint = BITIFUL_S4_ENDPOINT.to_string();
            config.region = BITIFUL_S4_REGION.to_string();
            config.path_style_access_enabled = false;
        }
        _ => {}
    }
}

fn normalize_endpoint(value: &str) -> Result<String, AppError> {
    let url = Url::parse(value.trim()).map_err(|_| command_error("S3 Endpoint 无效"))?;
    if url.scheme() != "https" {
        return Err(command_error("S3 Endpoint 必须使用 HTTPS"));
    }
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(command_error("S3 Endpoint 只能包含 HTTPS 主机地址"));
    }
    if !matches!(url.path(), "" | "/") {
        return Err(command_error(
            "S3 Endpoint 不得包含路径；请单独填写 Bucket 和对象路径",
        ));
    }
    Ok(url[..Position::BeforePath]
        .trim_end_matches('/')
        .to_string())
}

fn normalize_bucket(value: &str) -> Result<String, AppError> {
    let bucket = value.trim();
    let valid = (3..=63).contains(&bucket.len())
        && bucket.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && !bucket.starts_with(['.', '-'])
        && !bucket.ends_with(['.', '-'])
        && !bucket.contains("..")
        && !bucket.parse::<std::net::Ipv4Addr>().is_ok();
    if !valid {
        return Err(command_error("S3 Bucket 名称无效"));
    }
    Ok(bucket.to_string())
}

fn normalize_remote_path(value: &str) -> Result<String, AppError> {
    let path = value.trim().trim_start_matches('/');
    if path.is_empty()
        || path
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(command_error("S3 对象路径无效"));
    }
    Ok(path.to_string())
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
        .ok_or_else(|| AppError::Storage("无法解析 S3 凭据存储目录".to_string()))?;
    let mut migrated = false;
    if let Some(access_key_id) = config.access_key_id.as_mut() {
        let (value, should_migrate) = crate::services::secret_crypto::decrypt_or_migrate(
            storage_root,
            "s3/access-key-id",
            access_key_id,
        )?;
        *access_key_id = value;
        migrated |= should_migrate;
    }
    if let Some(secret_access_key) = config.secret_access_key.as_mut() {
        let (value, should_migrate) = crate::services::secret_crypto::decrypt_or_migrate(
            storage_root,
            "s3/secret-access-key",
            secret_access_key,
        )?;
        *secret_access_key = value;
        migrated |= should_migrate;
    }
    if config.remote_path.trim().is_empty() {
        config.remote_path = DEFAULT_REMOTE_PATH.to_string();
    }
    config.provider = normalize_provider(Some(&config.provider), &config.endpoint);
    apply_provider_defaults(&mut config);
    Ok((config, migrated))
}

fn write_config(app: &AppHandle, config: &StoredConfig) -> Result<(), AppError> {
    let path = config_path(app)?;
    write_config_at(&path, config)
}

fn write_config_at(path: &Path, config: &StoredConfig) -> Result<(), AppError> {
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析 S3 凭据存储目录".to_string()))?;
    let temporary = path.with_file_name(format!(".s3-backup.json.{}.tmp", uuid::Uuid::new_v4()));
    let mut encrypted = config.clone();
    if let Some(access_key_id) = encrypted.access_key_id.as_mut() {
        *access_key_id = crate::services::secret_crypto::encrypt(
            storage_root,
            "s3/access-key-id",
            access_key_id,
        )?;
    }
    if let Some(secret_access_key) = encrypted.secret_access_key.as_mut() {
        *secret_access_key = crate::services::secret_crypto::encrypt(
            storage_root,
            "s3/secret-access-key",
            secret_access_key,
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
fn lock_down_config_file(path: &std::path::Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_config_file(_path: &std::path::Path) -> Result<(), AppError> {
    Ok(())
}

fn public_config(config: &StoredConfig) -> Value {
    serde_json::json!({
        "enabled": config.enabled,
        "provider": config.provider,
        "endpoint": config.endpoint,
        "region": config.region,
        "bucket": config.bucket,
        "remotePath": config.remote_path,
        "pathStyleAccessEnabled": config.path_style_access_enabled,
        "accessKeyId": config.access_key_id,
        "hasSavedSecret": config.secret_access_key.is_some(),
        "lastSyncedAt": config.last_synced_at,
        "lastEtag": config.last_etag,
    })
}

fn validate_config(config: &StoredConfig, require_enabled: bool) -> Result<(), AppError> {
    if require_enabled && !config.enabled {
        return Err(command_error("请先启用 S3 配置备份"));
    }
    normalize_endpoint(&config.endpoint)?;
    normalize_bucket(&config.bucket)?;
    normalize_remote_path(&config.remote_path)?;
    if config.access_key_id.as_deref().is_none_or(str::is_empty)
        || config
            .secret_access_key
            .as_deref()
            .is_none_or(str::is_empty)
    {
        return Err(command_error(
            "请填写 S3 Access Key ID 和 Secret Access Key",
        ));
    }
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
        .map_err(|error| command_error(format!("无法初始化 S3 客户端: {error}")))
}

fn percent_encode(value: &str, preserve_slashes: bool) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (preserve_slashes && byte == b'/')
        {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn target(config: &StoredConfig, object_key: Option<&str>) -> Result<ObjectTarget, AppError> {
    let endpoint = Url::parse(&normalize_endpoint(&config.endpoint)?)
        .map_err(|error| command_error(format!("S3 Endpoint 无效: {error}")))?;
    let bucket = normalize_bucket(&config.bucket)?;
    let key = object_key.map(normalize_remote_path).transpose()?;
    let mut origin = endpoint[..Position::BeforePath].to_string();
    let canonical_uri = if config.path_style_access_enabled {
        match key {
            Some(key) => format!(
                "/{}/{}",
                percent_encode(&bucket, false),
                percent_encode(&key, true)
            ),
            None => format!("/{}", percent_encode(&bucket, false)),
        }
    } else {
        let host = endpoint
            .host_str()
            .ok_or_else(|| command_error("S3 Endpoint 缺少主机"))?;
        let authority = format!("{bucket}.{host}");
        origin = match endpoint.port() {
            Some(port) => format!("{}://{authority}:{port}", endpoint.scheme()),
            None => format!("{}://{authority}", endpoint.scheme()),
        };
        key.map(|key| format!("/{}", percent_encode(&key, true)))
            .unwrap_or_else(|| "/".to_string())
    };
    let url = Url::parse(&format!("{origin}{canonical_uri}"))
        .map_err(|error| command_error(format!("S3 对象地址无效: {error}")))?;
    let host = match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().unwrap_or_default()),
        None => url.host_str().unwrap_or_default().to_string(),
    };
    Ok(ObjectTarget {
        url,
        canonical_uri,
        host,
    })
}

fn bucket_target(config: &StoredConfig) -> Result<ObjectTarget, AppError> {
    target(config, None)
}

fn object_target(config: &StoredConfig) -> Result<ObjectTarget, AppError> {
    target(config, Some(&config.remote_path))
}
