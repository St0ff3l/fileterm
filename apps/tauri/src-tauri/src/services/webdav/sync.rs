pub fn get_config(app: &AppHandle) -> Result<Value, AppError> {
    Ok(public_config(&read_config(app)?))
}

pub fn save_config(app: &AppHandle, input: Value) -> Result<Value, AppError> {
    let previous = read_config(app)?;
    let enabled = input
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(previous.enabled);
    let allow_insecure_tls = input
        .get("allowInsecureTls")
        .and_then(Value::as_bool)
        .unwrap_or(previous.allow_insecure_tls == Some(true));
    let url = input
        .get("url")
        .and_then(Value::as_str)
        .map(|value| normalize_base_url(value, allow_insecure_tls))
        .transpose()?
        .unwrap_or(previous.url);
    let remote_path = input
        .get("remotePath")
        .and_then(Value::as_str)
        .map(normalize_remote_path)
        .transpose()?
        .unwrap_or(previous.remote_path);
    let username = match input.get("username") {
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.trim().to_string()),
        Some(Value::Null) => None,
        _ => previous.username,
    };
    let password = match input.get("password") {
        Some(Value::String(value)) if value.is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => previous.password,
    };
    let next = StoredConfig {
        enabled,
        url,
        username,
        remote_path,
        allow_insecure_tls: allow_insecure_tls.then_some(true),
        password,
        last_synced_at: previous.last_synced_at,
        last_etag: previous.last_etag,
        content_hash: previous.content_hash,
    };
    write_config(app, &next)?;
    crate::services::logging::info(
        app,
        "webdav",
        format!(
            "configuration saved enabled={} insecure_tls={}",
            next.enabled,
            next.allow_insecure_tls == Some(true)
        ),
    );
    Ok(public_config(&next))
}

/// Checks the saved endpoint and credentials without enabling synchronization
/// or changing the remote backup object.
pub async fn test_connection(app: &AppHandle) -> Result<Value, AppError> {
    crate::services::logging::info(app, "webdav", "connection test started");
    let result = async {
        let config = configured(app, false)?;
        let response = authenticated(client()?.head(remote_url(&config)?), &config)
            .send()
            .await
            .map_err(|error| command_error(format!("WebDAV 预检失败: {error}")))?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            return Err(response_error("预检", response.status()));
        }
        Ok(serde_json::json!({ "action": "test", "message": "WebDAV 连接成功。" }))
    }
    .await;
    match &result {
        Ok(_) => crate::services::logging::info(app, "webdav", "connection test completed"),
        Err(error) => crate::services::logging::error(
            app,
            "webdav",
            format!("connection test failed: {error}"),
        ),
    }
    result
}

pub async fn upload(app: &AppHandle, mode: Option<&str>) -> Result<Value, AppError> {
    crate::services::logging::info(app, "webdav", "upload started");
    let result = upload_inner(app, parse_upload_mode(mode)?).await;
    match &result {
        Ok(_) => crate::services::logging::info(app, "webdav", "upload completed"),
        Err(error) => {
            crate::services::logging::error(app, "webdav", format!("upload failed: {error}"))
        }
    }
    result
}

async fn upload_inner(app: &AppHandle, mode: UploadMode) -> Result<Value, AppError> {
    let mut config = configured(app, true)?;
    let client = client()?;
    let (remote_exists, _) = head_payload(&client, &config).await?;
    let password = backup_prompt::request(app, "upload", "WebDAV").await?;
    let (payload, content_hash) = if mode == UploadMode::MergeCloud && remote_exists {
        let (remote_bytes, _) = download_payload(&client, &config).await?;
        merge_bundle_with_local(app, &remote_bytes, &password)?
    } else {
        export_bundle(app, &password)?
    };
    let next_etag = upload_payload(&client, &config, payload).await?;
    config.last_etag = next_etag;
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(content_hash);
    write_config(app, &config)?;
    let message = match mode {
        UploadMode::OverwriteCloud => "已用本地连接配置覆盖 WebDAV 云端备份。",
        UploadMode::MergeCloud => "已将本地连接配置合并到 WebDAV 云端备份。",
    };
    Ok(serde_json::json!({
        "action": "upload",
        "mode": match mode {
            UploadMode::OverwriteCloud => "overwrite-cloud",
            UploadMode::MergeCloud => "merge-cloud",
        },
        "message": message,
    }))
}

async fn head_payload(
    client: &Client,
    config: &StoredConfig,
) -> Result<(bool, Option<String>), AppError> {
    let response = authenticated(client.head(remote_url(config)?), config)
        .send()
        .await
        .map_err(|error| command_error(format!("WebDAV 预检失败: {error}")))?;
    let remote_exists = response.status().is_success();
    if !remote_exists && response.status() != StatusCode::NOT_FOUND {
        return Err(response_error("预检", response.status()));
    }
    Ok((remote_exists, etag(response.headers())))
}

/// Upload a prepared profile bundle with optimistic-concurrency protection.
///
/// This boundary intentionally takes the HTTP client and serialized payload as
/// arguments so the protocol exchange can be exercised against a real WebDAV
/// endpoint without a Tauri application data directory.  The caller remains
/// responsible for persisting the returned ETag only after the PUT succeeds.
async fn upload_payload(
    client: &Client,
    config: &StoredConfig,
    payload: Vec<u8>,
) -> Result<Option<String>, AppError> {
    let remote = remote_url(config)?;
    let (_, remote_etag) = head_payload(client, config).await?;
    if let Some(last_etag) = config.last_etag.as_deref() {
        if remote_etag.as_deref() != Some(last_etag) {
            return Err(command_error(
                "远端配置自上次同步后已变更。请先下载并确认冲突，再上传。",
            ));
        }
    }
    let mut request = authenticated(
        client
            .put(remote)
            .header("content-type", "application/json; charset=utf-8")
            .body(payload),
        config,
    );
    request = match remote_etag.as_deref() {
        Some(value) => request.header(IF_MATCH, value),
        None => request.header(IF_NONE_MATCH, "*"),
    };
    let response = request
        .send()
        .await
        .map_err(|error| command_error(format!("WebDAV 上传失败: {error}")))?;
    if response.status() == StatusCode::PRECONDITION_FAILED {
        return Err(command_error("WebDAV ETag 冲突：远端文件已被其他设备修改"));
    }
    if !response.status().is_success() {
        return Err(response_error("上传", response.status()));
    }
    Ok(etag(response.headers()).or(remote_etag))
}

pub async fn download(app: &AppHandle, mode: Option<&str>) -> Result<Value, AppError> {
    crate::services::logging::info(app, "webdav", "download started");
    let result = download_inner(app, parse_download_mode(mode)?).await;
    match &result {
        Ok(value) => crate::services::logging::info(
            app,
            "webdav",
            format!(
                "download completed imported={} updated={} skipped={}",
                value.get("imported").and_then(Value::as_u64).unwrap_or(0),
                value.get("updated").and_then(Value::as_u64).unwrap_or(0),
                value.get("skipped").and_then(Value::as_u64).unwrap_or(0)
            ),
        ),
        Err(error) => {
            crate::services::logging::error(app, "webdav", format!("download failed: {error}"))
        }
    }
    result
}

async fn download_inner(app: &AppHandle, mode: DownloadMode) -> Result<Value, AppError> {
    let mut config = configured(app, true)?;
    let client = client()?;
    let (bytes, remote_etag) = download_payload(&client, &config).await?;
    let password = if backup_crypto::requires_password(&bytes)
        .map_err(|error| command_error(error.to_string()))?
    {
        Some(backup_prompt::request(app, "download", "WebDAV").await?)
    } else {
        None
    };
    let summary = import_bundle(
        app,
        &bytes,
        password.as_ref().map(|value| value.as_str()),
        mode,
    )?;
    config.last_etag = remote_etag;
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(sha256_hex(&bytes));
    write_config(app, &config)?;
    let action = match mode {
        DownloadMode::OverwriteLocal => format!(
            "已用 WebDAV 云端备份覆盖本地连接：导入 {} 个，替换 {} 个，跳过 {} 个无效项。",
            summary.imported, summary.replaced, summary.skipped
        ),
        DownloadMode::MergeLocal => format!(
            "已将 WebDAV 云端备份合并到本地：新增 {} 个，更新 {} 个，跳过 {} 个无效项。",
            summary.imported, summary.updated, summary.skipped
        ),
    };
    let message = if summary.legacy_plaintext {
        format!("{action} 该备份未加密，建议重新上传以生成加密备份。")
    } else {
        action
    };
    Ok(serde_json::json!({
        "action": "download",
        "mode": match mode {
            DownloadMode::OverwriteLocal => "overwrite-local",
            DownloadMode::MergeLocal => "merge-local",
        },
        "message": message,
        "imported": summary.imported,
        "updated": summary.updated,
        "replaced": summary.replaced,
        "skipped": summary.skipped,
        "legacyPlaintext": summary.legacy_plaintext,
    }))
}

/// Fetches a remote bundle without mutating local profiles. Keeping the HTTP
/// exchange at this boundary makes real WebDAV GET + ETag + integrity tests
/// possible without a Tauri application data directory.
async fn download_payload(
    client: &Client,
    config: &StoredConfig,
) -> Result<(Vec<u8>, Option<String>), AppError> {
    let response = authenticated(client.get(remote_url(config)?), config)
        .send()
        .await
        .map_err(|error| command_error(format!("WebDAV 下载失败: {error}")))?;
    if !response.status().is_success() {
        return Err(response_error("下载", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size as usize > MAX_BUNDLE_BYTES)
    {
        return Err(command_error("WebDAV 配置包超过 5 MB 限制"));
    }
    let remote_etag = etag(response.headers());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| command_error(format!("WebDAV 下载内容失败: {error}")))?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(command_error("WebDAV 配置包超过 5 MB 限制"));
    }
    Ok((bytes.to_vec(), remote_etag))
}
