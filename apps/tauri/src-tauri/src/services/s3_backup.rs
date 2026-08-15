//! Manual S3-compatible backup for the complete FileTerm connection bundle.
//!
//! The renderer only receives non-secret connection settings. Access keys and
//! the bundle containing profile credentials stay in this Rust service and are
//! persisted in a user-only file. Cloudflare R2 is an S3-compatible preset
//! using its required `auto` region and path-style addressing.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, ETAG};
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use url::{Position, Url};

use crate::services::{backup_crypto, backup_prompt, webdav};
use crate::storage::workspace_file;
use crate::AppError;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_REMOTE_PATH: &str = "fileterm/connections.json";
const MAX_BUNDLE_BYTES: usize = 5 * 1024 * 1024;
const PROVIDER_CUSTOM: &str = "custom";
const PROVIDER_CLOUDFLARE_R2: &str = "cloudflare-r2";
const PROVIDER_BITIFUL_S4: &str = "bitiful-s4";
const BITIFUL_S4_ENDPOINT: &str = "https://s3.bitiful.net";
const BITIFUL_S4_REGION: &str = "cn-east-1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    enabled: bool,
    provider: String,
    endpoint: String,
    region: String,
    bucket: String,
    remote_path: String,
    path_style_access_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret_access_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
}

impl Default for StoredConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: PROVIDER_CUSTOM.to_string(),
            endpoint: String::new(),
            region: "us-east-1".to_string(),
            bucket: String::new(),
            remote_path: DEFAULT_REMOTE_PATH.to_string(),
            path_style_access_enabled: true,
            access_key_id: None,
            secret_access_key: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        }
    }
}

struct ObjectTarget {
    url: Url,
    canonical_uri: String,
    host: String,
}

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

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    bytes_to_hex(&hasher.finalize())
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

fn signature(
    secret_access_key: &str,
    date_stamp: &str,
    region: &str,
    string_to_sign: &str,
) -> String {
    let date_key = hmac(
        format!("AWS4{secret_access_key}").as_bytes(),
        date_stamp.as_bytes(),
    );
    let region_key = hmac(&date_key, region.as_bytes());
    let service_key = hmac(&region_key, b"s3");
    let signing_key = hmac(&service_key, b"aws4_request");
    bytes_to_hex(&hmac(&signing_key, string_to_sign.as_bytes()))
}

fn aws_timestamp() -> (String, String) {
    let timestamp = webdav::export_timestamp();
    let amz_date = timestamp.replace(['-', ':'], "");
    let date_stamp = amz_date[..8].to_string();
    (amz_date, date_stamp)
}

fn signed_request(
    client: &Client,
    config: &StoredConfig,
    method: Method,
    target: ObjectTarget,
    body: Vec<u8>,
    extra_headers: BTreeMap<&str, String>,
) -> Result<reqwest::RequestBuilder, AppError> {
    let access_key_id = config
        .access_key_id
        .as_deref()
        .ok_or_else(|| command_error("缺少 S3 Access Key ID"))?;
    let secret_access_key = config
        .secret_access_key
        .as_deref()
        .ok_or_else(|| command_error("缺少 S3 Secret Access Key"))?;
    let (amz_date, date_stamp) = aws_timestamp();
    let payload_hash = sha256_hex(&body);
    let mut headers = BTreeMap::new();
    headers.insert("host", target.host.clone());
    headers.insert("x-amz-content-sha256", payload_hash.clone());
    headers.insert("x-amz-date", amz_date.clone());
    for (name, value) in extra_headers {
        headers.insert(name, value);
    }
    let canonical_headers = headers
        .iter()
        .map(|(name, value)| {
            format!(
                "{name}:{}\n",
                value.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        })
        .collect::<String>();
    let signed_headers = headers.keys().copied().collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{}\n{}\n\n{}\n{}\n{}",
        method.as_str(),
        target.canonical_uri,
        canonical_headers,
        signed_headers,
        payload_hash
    );
    let credential_scope = format!("{date_stamp}/{}/s3/aws4_request", config.region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = signature(
        secret_access_key,
        &date_stamp,
        &config.region,
        &string_to_sign,
    );
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={access_key_id}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    let mut request = client
        .request(method, target.url)
        .header("authorization", authorization)
        .header("host", target.host)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date);
    for (name, value) in headers {
        if !matches!(name, "host" | "x-amz-content-sha256" | "x-amz-date") {
            request = request.header(name, value);
        }
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    Ok(request)
}

fn etag(headers: &HeaderMap) -> Option<String> {
    headers
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn response_error(action: &str, status: StatusCode) -> AppError {
    command_error(format!("S3 {action}失败 ({status})"))
}

async fn head_object(client: &Client, config: &StoredConfig) -> Result<Option<String>, AppError> {
    let response = signed_request(
        client,
        config,
        Method::HEAD,
        object_target(config)?,
        Vec::new(),
        BTreeMap::new(),
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 预检失败: {error}")))?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(response_error("预检", response.status()));
    }
    Ok(etag(response.headers()))
}

async fn head_bucket(client: &Client, config: &StoredConfig) -> Result<(), AppError> {
    let response = signed_request(
        client,
        config,
        Method::HEAD,
        bucket_target(config)?,
        Vec::new(),
        BTreeMap::new(),
    )?
    .send()
    .await
    .map_err(|error| command_error(format!("S3 Bucket 预检失败: {error}")))?;
    if !response.status().is_success() {
        return Err(response_error("Bucket 预检", response.status()));
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::{
        apply_provider_defaults, normalize_bucket, normalize_provider, normalize_remote_path,
        percent_encode, read_config_at, signature, target, validate_config, write_config_at,
        StoredConfig, BITIFUL_S4_ENDPOINT, BITIFUL_S4_REGION, DEFAULT_REMOTE_PATH,
        PROVIDER_BITIFUL_S4, PROVIDER_CLOUDFLARE_R2,
    };
    use std::fs;

    #[test]
    fn uses_cloudflare_r2_defaults_from_its_endpoint() {
        assert_eq!(
            normalize_provider(None, "https://account.r2.cloudflarestorage.com"),
            PROVIDER_CLOUDFLARE_R2
        );
    }

    #[test]
    fn recognizes_the_bitiful_s4_endpoint() {
        assert_eq!(
            normalize_provider(None, BITIFUL_S4_ENDPOINT),
            PROVIDER_BITIFUL_S4
        );
        let mut config = StoredConfig {
            provider: PROVIDER_BITIFUL_S4.to_string(),
            endpoint: "https://not-bitiful.example".to_string(),
            region: "us-east-1".to_string(),
            bucket: "fileterm-backups".to_string(),
            remote_path: "sync/profiles.json".to_string(),
            path_style_access_enabled: true,
            ..StoredConfig::default()
        };
        apply_provider_defaults(&mut config);
        assert_eq!(config.endpoint, BITIFUL_S4_ENDPOINT);
        assert_eq!(config.region, BITIFUL_S4_REGION);
        assert!(!config.path_style_access_enabled);
        let target = target(&config, Some(&config.remote_path)).unwrap();
        assert_eq!(
            target.url.as_str(),
            "https://fileterm-backups.s3.bitiful.net/sync/profiles.json"
        );
    }

    #[test]
    fn validates_bucket_and_object_key() {
        assert_eq!(StoredConfig::default().remote_path, DEFAULT_REMOTE_PATH);
        assert!(normalize_bucket("FileTerm").is_err());
        assert!(normalize_bucket("a").is_err());
        assert_eq!(
            normalize_bucket("fileterm-backups").unwrap(),
            "fileterm-backups"
        );
        assert!(normalize_remote_path("../profiles.json").is_err());
        assert!(normalize_remote_path("sync/../profiles.json").is_err());
        assert_eq!(
            normalize_remote_path("sync/profiles.json").unwrap(),
            "sync/profiles.json"
        );
    }

    #[test]
    fn allows_connection_tests_without_enabling_s3_backup() {
        let config = StoredConfig {
            enabled: false,
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            region: "auto".to_string(),
            bucket: "fileterm-backups".to_string(),
            access_key_id: Some("access-key".to_string()),
            secret_access_key: Some("secret-key".to_string()),
            ..StoredConfig::default()
        };
        assert!(validate_config(&config, false).is_ok());
        assert!(validate_config(&config, true).is_err());
    }

    #[test]
    fn plaintext_s3_credentials_are_migrated_to_encrypted_storage() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-s3-secret-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        let path = directory.join("s3-backup.json");
        let legacy = StoredConfig {
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            access_key_id: Some("s3-access-key".to_string()),
            secret_access_key: Some("s3-secret-key".to_string()),
            ..StoredConfig::default()
        };
        fs::write(
            &path,
            serde_json::to_vec(&legacy).expect("legacy config json"),
        )
        .expect("legacy config write");

        let (config, migrated) = read_config_at(&path).expect("legacy config read");
        assert!(migrated);
        assert_eq!(config.access_key_id.as_deref(), Some("s3-access-key"));
        assert_eq!(config.secret_access_key.as_deref(), Some("s3-secret-key"));
        write_config_at(&path, &config).expect("migrated config write");
        let raw = fs::read_to_string(&path).expect("migrated config read");
        assert!(!raw.contains("s3-access-key"));
        assert!(!raw.contains("s3-secret-key"));

        let (decoded, migrated_again) = read_config_at(&path).expect("encrypted config read");
        assert!(!migrated_again);
        assert_eq!(decoded.access_key_id.as_deref(), Some("s3-access-key"));
        assert_eq!(decoded.secret_access_key.as_deref(), Some("s3-secret-key"));
        fs::remove_dir_all(directory).expect("fixture directory should be removed");
    }

    #[test]
    fn canonical_uri_keeps_key_path_separators() {
        assert_eq!(
            percent_encode("backup folder/profiles.json", true),
            "backup%20folder/profiles.json"
        );
        assert_eq!(
            percent_encode("backup folder/profiles.json", false),
            "backup%20folder%2Fprofiles.json"
        );
    }

    #[test]
    fn bucket_preflight_targets_the_bucket_not_the_backup_object() {
        let config = StoredConfig {
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            bucket: "fileterm-backups".to_string(),
            remote_path: "sync/profiles.json".to_string(),
            ..StoredConfig::default()
        };
        let bucket = target(&config, None).unwrap();
        let object = target(&config, Some(&config.remote_path)).unwrap();
        assert_eq!(bucket.canonical_uri, "/fileterm-backups");
        assert_eq!(object.canonical_uri, "/fileterm-backups/sync/profiles.json");
    }

    #[test]
    fn matches_the_aws_sigv4_get_object_reference_signature() {
        // AWS's documented GET Object example, using the fixed credentials
        // and canonical request timestamp from the SigV4 guide.
        let string_to_sign = concat!(
            "AWS4-HMAC-SHA256\n",
            "20130524T000000Z\n",
            "20130524/us-east-1/s3/aws4_request\n",
            "7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972"
        );
        assert_eq!(
            signature(
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "20130524",
                "us-east-1",
                string_to_sign
            ),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }
}
