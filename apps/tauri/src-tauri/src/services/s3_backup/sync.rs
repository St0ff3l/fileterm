pub fn get_config(app: &AppHandle) -> Result<Value, AppError> {
    Ok(public_config(&read_config(app)?))
}

pub fn save_config(app: &AppHandle, input: Value) -> Result<Value, AppError> {
    let previous = read_config(app)?;
    let enabled = input
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(previous.enabled);
    let endpoint = input
        .get("endpoint")
        .and_then(Value::as_str)
        .map(normalize_endpoint)
        .transpose()?
        .unwrap_or(previous.endpoint);
    let provider = normalize_provider(input.get("provider").and_then(Value::as_str), &endpoint);
    let region = input
        .get("region")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&previous.region)
        .to_string();
    let bucket = input
        .get("bucket")
        .and_then(Value::as_str)
        .map(normalize_bucket)
        .transpose()?
        .unwrap_or(previous.bucket);
    let remote_path = input
        .get("remotePath")
        .and_then(Value::as_str)
        .map(normalize_remote_path)
        .transpose()?
        .unwrap_or(previous.remote_path);
    let access_key_id = match input.get("accessKeyId") {
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.trim().to_string()),
        Some(Value::Null) => None,
        _ => previous.access_key_id,
    };
    let secret_access_key = match input.get("secretAccessKey") {
        Some(Value::String(value)) if value.is_empty() => previous.secret_access_key,
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Null) => None,
        _ => previous.secret_access_key,
    };
    let path_style_access_enabled = input
        .get("pathStyleAccessEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(previous.path_style_access_enabled);
    let mut next = StoredConfig {
        enabled,
        provider,
        endpoint,
        region,
        bucket,
        remote_path,
        path_style_access_enabled,
        access_key_id,
        secret_access_key,
        last_synced_at: previous.last_synced_at,
        last_etag: previous.last_etag,
        content_hash: previous.content_hash,
    };
    apply_provider_defaults(&mut next);
    if next.enabled {
        normalize_endpoint(&next.endpoint)?;
        normalize_bucket(&next.bucket)?;
        normalize_remote_path(&next.remote_path)?;
        if next.access_key_id.as_deref().is_none_or(str::is_empty)
            || next.secret_access_key.as_deref().is_none_or(str::is_empty)
        {
            return Err(command_error(
                "启用 S3 备份前请填写 Access Key ID 和 Secret Access Key",
            ));
        }
    }
    write_config(app, &next)?;
    crate::services::logging::info(
        app,
        "s3-backup",
        format!(
            "configuration saved enabled={} provider={} path_style={}",
            next.enabled, next.provider, next.path_style_access_enabled
        ),
    );
    Ok(public_config(&next))
}

pub async fn test_connection(app: &AppHandle) -> Result<Value, AppError> {
    crate::services::logging::info(app, "s3-backup", "connection test started");
    let result = async {
        let config = configured(app, false)?;
        let client = client()?;
        head_bucket(&client, &config).await?;
        let _ = head_object(&client, &config).await?;
        Ok(serde_json::json!({ "action": "test", "message": "S3 连接成功。" }))
    }
    .await;
    match &result {
        Ok(_) => crate::services::logging::info(app, "s3-backup", "connection test completed"),
        Err(error) => crate::services::logging::error(
            app,
            "s3-backup",
            format!("connection test failed: {error}"),
        ),
    }
    result
}

pub async fn upload(app: &AppHandle, mode: Option<&str>) -> Result<Value, AppError> {
    crate::services::logging::info(app, "s3-backup", "upload started");
    let result = upload_inner(app, webdav::parse_upload_mode(mode)?).await;
    match &result {
        Ok(_) => crate::services::logging::info(app, "s3-backup", "upload completed"),
        Err(error) => {
            crate::services::logging::error(app, "s3-backup", format!("upload failed: {error}"))
        }
    }
    result
}

async fn upload_inner(app: &AppHandle, mode: webdav::UploadMode) -> Result<Value, AppError> {
    let mut config = configured(app, true)?;
    let client = client()?;
    let remote_etag = head_object(&client, &config).await?;
    if let Some(last_etag) = config.last_etag.as_deref() {
        if remote_etag.as_deref() != Some(last_etag) {
            return Err(command_error(
                "远端配置自上次同步后已变更。请先下载并确认冲突，再上传。",
            ));
        }
    }
    let password = backup_prompt::request(app, "upload", "S3").await?;
    let (payload, content_hash) = if mode == webdav::UploadMode::MergeCloud {
        if remote_etag.is_some() {
            let (remote_bytes, _) = download_payload(&client, &config).await?;
            webdav::merge_bundle_with_local(app, &remote_bytes, &password)?
        } else {
            webdav::export_bundle(app, &password)?
        }
    } else {
        webdav::export_bundle(app, &password)?
    };
    let mut headers = BTreeMap::new();
    headers.insert(
        "content-type",
        "application/json; charset=utf-8".to_string(),
    );
    match remote_etag.as_deref() {
        Some(value) => {
            headers.insert("if-match", value.to_string());
        }
        None => {
            headers.insert("if-none-match", "*".to_string());
        }
    }
    let response = signed_request(
        &client,
        &config,
        Method::PUT,
        object_target(&config)?,
        payload,
        headers,
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 上传失败: {error}")))?;
    if response.status() == StatusCode::PRECONDITION_FAILED {
        return Err(command_error("S3 ETag 冲突：远端文件已被其他设备修改"));
    }
    if !response.status().is_success() {
        return Err(response_error("上传", response.status()));
    }
    config.last_etag = etag(response.headers()).or(remote_etag);
    config.last_synced_at = Some(webdav::export_timestamp());
    config.content_hash = Some(content_hash);
    write_config(app, &config)?;
    let message = match mode {
        webdav::UploadMode::OverwriteCloud => "已用本地连接配置覆盖 S3 云端备份。",
        webdav::UploadMode::MergeCloud => "已将本地连接配置合并到 S3 云端备份。",
    };
    Ok(serde_json::json!({
        "action": "upload",
        "mode": match mode {
            webdav::UploadMode::OverwriteCloud => "overwrite-cloud",
            webdav::UploadMode::MergeCloud => "merge-cloud",
        },
        "message": message,
    }))
}

pub async fn download(app: &AppHandle, mode: Option<&str>) -> Result<Value, AppError> {
    crate::services::logging::info(app, "s3-backup", "download started");
    let result = download_inner(app, webdav::parse_download_mode(mode)?).await;
    match &result {
        Ok(value) => crate::services::logging::info(
            app,
            "s3-backup",
            format!(
                "download completed imported={} updated={} skipped={}",
                value.get("imported").and_then(Value::as_u64).unwrap_or(0),
                value.get("updated").and_then(Value::as_u64).unwrap_or(0),
                value.get("skipped").and_then(Value::as_u64).unwrap_or(0)
            ),
        ),
        Err(error) => {
            crate::services::logging::error(app, "s3-backup", format!("download failed: {error}"))
        }
    }
    result
}

async fn download_inner(app: &AppHandle, mode: webdav::DownloadMode) -> Result<Value, AppError> {
    let mut config = configured(app, true)?;
    let client = client()?;
    let (bytes, remote_etag) = download_payload(&client, &config).await?;
    let password = if backup_crypto::requires_password(&bytes)
        .map_err(|error| command_error(error.to_string()))?
    {
        Some(backup_prompt::request(app, "download", "S3").await?)
    } else {
        None
    };
    let summary = webdav::import_bundle(
        app,
        &bytes,
        password.as_ref().map(|value| value.as_str()),
        mode,
    )?;
    config.last_etag = remote_etag;
    config.last_synced_at = Some(webdav::export_timestamp());
    config.content_hash = Some(sha256_hex(&bytes));
    write_config(app, &config)?;
    let action = match mode {
        webdav::DownloadMode::OverwriteLocal => format!(
            "已用 S3 云端备份覆盖本地连接：导入 {} 个，替换 {} 个，跳过 {} 个无效项。",
            summary.imported, summary.replaced, summary.skipped
        ),
        webdav::DownloadMode::MergeLocal => format!(
            "已将 S3 云端备份合并到本地：新增 {} 个，更新 {} 个，跳过 {} 个无效项。",
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
            webdav::DownloadMode::OverwriteLocal => "overwrite-local",
            webdav::DownloadMode::MergeLocal => "merge-local",
        },
        "message": message,
        "imported": summary.imported,
        "updated": summary.updated,
        "replaced": summary.replaced,
        "skipped": summary.skipped,
        "legacyPlaintext": summary.legacy_plaintext,
    }))
}

async fn download_payload(
    client: &Client,
    config: &StoredConfig,
) -> Result<(Vec<u8>, Option<String>), AppError> {
    let response = signed_request(
        client,
        config,
        Method::GET,
        object_target(config)?,
        Vec::new(),
        BTreeMap::new(),
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 下载失败: {error}")))?;
    if !response.status().is_success() {
        return Err(response_error("下载", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size as usize > MAX_BUNDLE_BYTES)
    {
        return Err(command_error("S3 配置包超过 5 MB 限制"));
    }
    let remote_etag = etag(response.headers());
    let bytes = response
        .bytes()
        .await
        .map_err(|error| command_error(format!("S3 下载内容失败: {error}")))?;
    if bytes.len() > MAX_BUNDLE_BYTES {
        return Err(command_error("S3 配置包超过 5 MB 限制"));
    }
    Ok((bytes.to_vec(), remote_etag))
}
