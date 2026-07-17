//! Manual, conflict-aware WebDAV synchronization for the complete profile
//! bundle. Connection credentials stay inside the Rust service boundary but
//! are intentionally included in the remote bundle so another FileTerm
//! installation can reconnect after downloading it.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::header::{HeaderMap, ETAG, IF_MATCH, IF_NONE_MATCH};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use url::Url;

use crate::services::profile_ops;
use crate::storage::workspace_file;
use crate::AppError;

const MAX_BUNDLE_BYTES: usize = 5 * 1024 * 1024;
const DEFAULT_REMOTE_PATH: &str = "fileterm-connections.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConfig {
    enabled: bool,
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    remote_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    allow_insecure_tls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
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
            url: String::new(),
            username: None,
            remote_path: DEFAULT_REMOTE_PATH.to_string(),
            allow_insecure_tls: None,
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        }
    }
}

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
    if !path.exists() {
        return Ok(StoredConfig::default());
    }
    lock_down_config_file(&path)?;
    let content = fs::read_to_string(path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut config: StoredConfig = serde_json::from_str(&content)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    if config.remote_path.trim().is_empty() {
        config.remote_path = DEFAULT_REMOTE_PATH.to_string();
    }
    Ok(config)
}

fn write_config(app: &AppHandle, config: &StoredConfig) -> Result<(), AppError> {
    let path = config_path(app)?;
    let temporary = path.with_file_name(format!(".webdav-sync.json.{}.tmp", uuid::Uuid::new_v4()));
    let content = serde_json::to_vec_pretty(config)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&temporary, &content)?;
    if let Err(error) = lock_down_config_file(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    crate::storage::replace_file_atomically(&temporary, &path)?;
    lock_down_config_file(&path)
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

fn configured(app: &AppHandle) -> Result<StoredConfig, AppError> {
    let config = read_config(app)?;
    if !config.enabled {
        return Err(command_error("请先启用 WebDAV 配置同步"));
    }
    normalize_base_url(&config.url, config.allow_insecure_tls == Some(true))?;
    normalize_remote_path(&config.remote_path)?;
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

pub fn export_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    // Howard Hinnant's civil date conversion, with 1970-01-01 as day 0.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_parameter = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_parameter + 2) / 5 + 1;
    let month = month_parameter + if month_parameter < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

fn export_bundle(app: &AppHandle) -> Result<(Vec<u8>, String), AppError> {
    let (profiles, _) = profile_ops::read_and_heal_profiles(app)?;
    build_export_bundle(&profiles)
}

fn build_export_bundle(profiles: &[Value]) -> Result<(Vec<u8>, String), AppError> {
    let profile_bytes = serde_json::to_vec(&profiles)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let content_hash = sha256_hex(&profile_bytes);
    let payload = serde_json::json!({
        "schemaVersion": 2,
        "containsSecrets": true,
        "generatedAt": export_timestamp(),
        "contentHash": content_hash,
        "profiles": profiles,
    });
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    Ok((bytes, content_hash))
}

fn profile_fingerprint(profile: &Value) -> Option<(String, String, String, u64, String)> {
    Some((
        profile.get("type")?.as_str()?.to_ascii_lowercase(),
        profile.get("name")?.as_str()?.trim().to_string(),
        profile
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
        profile.get("port").and_then(Value::as_u64).unwrap_or(0),
        profile
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string(),
    ))
}

fn sanitize_import_profile(value: &Value) -> Result<Value, String> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| "配置项不是对象".to_string())?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("ssh")
        .to_ascii_lowercase();
    if !matches!(kind.as_str(), "ssh" | "ftp" | "telnet" | "serial") {
        return Err("不支持的连接类型".to_string());
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("连接名称为空".to_string());
    }
    if kind != "serial" {
        let host = object
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let port = object.get("port").and_then(Value::as_u64).unwrap_or(0);
        if host.is_empty() || !(1..=65535).contains(&port) {
            return Err("主机或端口无效".to_string());
        }
    } else if object
        .get("devicePath")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Err("串口设备路径为空".to_string());
    }
    object.insert("type".to_string(), Value::String(kind));
    object
        .entry("group".to_string())
        .or_insert_with(|| Value::String("默认".to_string()));
    object
        .entry("remotePath".to_string())
        .or_insert_with(|| Value::String("/".to_string()));
    object
        .entry("username".to_string())
        .or_insert_with(|| Value::String(String::new()));
    for key in ["id", "parentId", "order", "lastUsedAt"] {
        object.remove(key);
    }
    Ok(Value::Object(object))
}

fn merge_synced_profile(existing: &Value, incoming: &Value) -> Result<Value, String> {
    let mut merged = existing
        .as_object()
        .cloned()
        .ok_or_else(|| "本地配置项不是对象".to_string())?;
    let incoming = incoming
        .as_object()
        .ok_or_else(|| "远端配置项不是对象".to_string())?;
    for (key, value) in incoming {
        if !matches!(key.as_str(), "id" | "parentId" | "order" | "lastUsedAt") {
            merged.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Object(merged))
}

fn parse_bundle(bytes: &[u8]) -> Result<Vec<Value>, AppError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| command_error(format!("WebDAV 配置包无效: {error}")))?;
    let profiles = match &value {
        Value::Array(items) => items.clone(),
        Value::Object(object) => object
            .get("profiles")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| command_error("WebDAV 配置包缺少 profiles"))?,
        _ => return Err(command_error("WebDAV 配置包格式无效")),
    };
    if let Some(expected_hash) = value.get("contentHash").and_then(Value::as_str) {
        let canonical = serde_json::to_vec(&profiles)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        if sha256_hex(&canonical) != expected_hash {
            return Err(command_error(
                "WebDAV 配置包 hash 校验失败，文件可能已损坏或被篡改",
            ));
        }
    }
    Ok(profiles)
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

pub async fn upload(app: &AppHandle) -> Result<Value, AppError> {
    crate::services::logging::info(app, "webdav", "upload started");
    let result = upload_inner(app).await;
    match &result {
        Ok(_) => crate::services::logging::info(app, "webdav", "upload completed"),
        Err(error) => {
            crate::services::logging::error(app, "webdav", format!("upload failed: {error}"))
        }
    }
    result
}

async fn upload_inner(app: &AppHandle) -> Result<Value, AppError> {
    let mut config = configured(app)?;
    let client = client()?;
    let (payload, content_hash) = export_bundle(app)?;
    let next_etag = upload_payload(&client, &config, payload).await?;
    config.last_etag = next_etag;
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(content_hash);
    write_config(app, &config)?;
    Ok(serde_json::json!({ "action": "upload", "message": "连接配置已上传到 WebDAV。" }))
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
    let head = authenticated(client.head(remote.clone()), config)
        .send()
        .await
        .map_err(|error| command_error(format!("WebDAV 预检失败: {error}")))?;
    let remote_exists = head.status().is_success();
    if !remote_exists && head.status() != StatusCode::NOT_FOUND {
        return Err(response_error("预检", head.status()));
    }
    let remote_etag = etag(head.headers());
    if remote_exists && config.last_etag.is_none() {
        return Err(command_error(
            "远端已存在配置包。请先下载并确认内容，再上传以避免首次同步覆盖。",
        ));
    }
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

pub async fn download(app: &AppHandle) -> Result<Value, AppError> {
    crate::services::logging::info(app, "webdav", "download started");
    let result = download_inner(app).await;
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

async fn download_inner(app: &AppHandle) -> Result<Value, AppError> {
    let mut config = configured(app)?;
    let client = client()?;
    let (bytes, remote_etag) = download_payload(&client, &config).await?;
    let profiles = parse_bundle(&bytes)?;
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut known = existing
        .iter()
        .filter_map(|profile| {
            profile_fingerprint(profile).map(|fingerprint| (fingerprint, profile.clone()))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let mut imported = 0_u64;
    let mut updated = 0_u64;
    let mut skipped = 0_u64;
    for profile in profiles {
        let sanitized = match sanitize_import_profile(&profile) {
            Ok(profile) => profile,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let Some(fingerprint) = profile_fingerprint(&sanitized) else {
            skipped += 1;
            continue;
        };
        if let Some(existing_profile) = known.get(&fingerprint).cloned() {
            let Some(profile_id) = existing_profile
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
            else {
                skipped += 1;
                continue;
            };
            let merged = match merge_synced_profile(&existing_profile, &sanitized) {
                Ok(profile) => profile,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let saved = profile_ops::update_profile(app, &profile_id, merged)?;
            known.insert(fingerprint, saved);
            updated += 1;
            continue;
        }
        let created = profile_ops::create_profile(app, sanitized)?;
        known.insert(fingerprint, created);
        imported += 1;
    }
    config.last_etag = remote_etag;
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(sha256_hex(&bytes));
    write_config(app, &config)?;
    Ok(serde_json::json!({
        "action": "download",
        "message": format!("已从 WebDAV 导入 {imported} 个连接，更新 {updated} 个现有连接；跳过 {skipped} 个无效项。"),
        "imported": imported,
        "updated": updated,
        "skipped": skipped,
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

#[cfg(test)]
mod tests {
    use super::{
        build_export_bundle, client, download_payload, merge_synced_profile, normalize_remote_path,
        parse_bundle, sanitize_import_profile, sha256_hex, upload_payload, StoredConfig,
    };
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert!(count > 0, "client closed before completing HTTP headers");
            request.extend_from_slice(&byte[..count]);
        }

        let headers = String::from_utf8(request.clone()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or_default();
        if content_length > 0 {
            let mut body = vec![0_u8; content_length];
            socket.read_exact(&mut body).await.unwrap();
            request.extend_from_slice(&body);
        }

        String::from_utf8(request).unwrap()
    }

    #[test]
    fn rejects_traversal_remote_paths() {
        assert!(normalize_remote_path("../profiles.json").is_err());
        assert!(normalize_remote_path("profiles/../secret.json").is_err());
        assert_eq!(
            normalize_remote_path("sync/profiles.json").unwrap(),
            "sync/profiles.json"
        );
    }

    #[test]
    fn verifies_profile_bundle_hash() {
        let profiles =
            json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
        let hash = sha256_hex(&serde_json::to_vec(&profiles).unwrap());
        let payload = json!({ "profiles": profiles, "contentHash": hash });
        assert_eq!(
            parse_bundle(&serde_json::to_vec(&payload).unwrap())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn keeps_connection_credentials_when_sanitizing_webdav_profiles() {
        let profile = sanitize_import_profile(&json!({
            "id": "remote-id",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "secret",
            "passphrase": "key-secret",
            "proxy": { "type": "http", "password": "proxy-secret" }
        }))
        .unwrap();
        assert_eq!(profile["password"], "secret");
        assert_eq!(profile["passphrase"], "key-secret");
        assert_eq!(profile["proxy"]["password"], "proxy-secret");
        assert!(profile.get("id").is_none());
    }

    #[test]
    fn webdav_upload_bundle_contains_saved_credentials() {
        let profiles = vec![json!({
            "id": "profile-1",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "secret",
            "passphrase": "key-secret",
            "proxy": { "type": "http", "password": "proxy-secret" }
        })];
        let (bytes, _) = build_export_bundle(&profiles).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["schemaVersion"], 2);
        assert_eq!(payload["containsSecrets"], true);
        assert_eq!(payload["profiles"][0]["password"], "secret");
        assert_eq!(payload["profiles"][0]["passphrase"], "key-secret");
        assert_eq!(payload["profiles"][0]["proxy"]["password"], "proxy-secret");
    }

    #[test]
    fn remote_duplicate_updates_credentials_without_replacing_local_identity() {
        let existing = json!({
            "id": "local-id",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "old",
            "order": 42
        });
        let incoming = json!({
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "new"
        });
        let merged = merge_synced_profile(&existing, &incoming).unwrap();
        assert_eq!(merged["id"], "local-id");
        assert_eq!(merged["order"], 42);
        assert_eq!(merged["password"], "new");
    }

    #[tokio::test]
    async fn real_webdav_server_rejects_stale_etag_with_if_match() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut head, _) = listener.accept().await.unwrap();
            let head_request = read_request(&mut head).await;
            assert!(head_request.starts_with("HEAD /profiles.json HTTP/1.1\r\n"));
            head.write_all(
                b"HTTP/1.1 200 OK\r\nETag: \"etag-before-write\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();

            let (mut put, _) = listener.accept().await.unwrap();
            let put_request = read_request(&mut put).await;
            assert!(put_request.starts_with("PUT /profiles.json HTTP/1.1\r\n"));
            assert!(put_request.contains("if-match: \"etag-before-write\"\r\n"));
            put.write_all(
                b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: Some("\"etag-before-write\"".to_string()),
            content_hash: None,
        };
        let result = upload_payload(&client().unwrap(), &config, b"{}".to_vec()).await;
        server.await.unwrap();
        let error = result.unwrap_err();
        assert!(error.to_string().contains("ETag 冲突"), "{error}");
    }

    #[tokio::test]
    async fn real_webdav_server_uploads_payload_and_returns_fresh_etag() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let payload = br#"{"profiles":[]}"#.to_vec();
        let expected_payload = payload.clone();
        let server = tokio::spawn(async move {
            let (mut head, _) = listener.accept().await.unwrap();
            let head_request = read_request(&mut head).await;
            assert!(head_request.starts_with("HEAD /profiles.json HTTP/1.1\r\n"));
            head.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();

            let (mut put, _) = listener.accept().await.unwrap();
            let put_request = read_request(&mut put).await;
            assert!(put_request.starts_with("PUT /profiles.json HTTP/1.1\r\n"));
            assert!(put_request.contains("if-none-match: *\r\n"));
            assert!(
                put_request.contains(&format!("content-length: {}\r\n", expected_payload.len()))
            );
            let body = put_request.split_once("\r\n\r\n").unwrap().1.as_bytes();
            assert_eq!(body, expected_payload);
            put.write_all(
                b"HTTP/1.1 201 Created\r\nETag: \"etag-after-write\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        };
        assert_eq!(
            upload_payload(&client().unwrap(), &config, payload)
                .await
                .unwrap(),
            Some("\"etag-after-write\"".to_string())
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn real_webdav_server_downloads_payload_and_hash_is_verified() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let profiles =
            json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
        let profile_bytes = serde_json::to_vec(&profiles).unwrap();
        let payload = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "contentHash": sha256_hex(&profile_bytes),
            "profiles": profiles,
        }))
        .unwrap();
        let server_payload = payload.clone();
        let server = tokio::spawn(async move {
            let (mut get, _) = listener.accept().await.unwrap();
            let get_request = read_request(&mut get).await;
            assert!(get_request.starts_with("GET /profiles.json HTTP/1.1\r\n"));
            get.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nETag: \"etag-download\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    server_payload.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            get.write_all(&server_payload).await.unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        };
        let (downloaded, etag) = download_payload(&client().unwrap(), &config).await.unwrap();
        assert_eq!(downloaded, payload);
        assert_eq!(etag.as_deref(), Some("\"etag-download\""));
        assert_eq!(parse_bundle(&downloaded).unwrap().len(), 1);
        server.await.unwrap();
    }
}
