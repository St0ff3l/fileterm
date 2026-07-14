//! Manual, conflict-aware WebDAV synchronization for the portable profile
//! bundle. The renderer only ever receives the public config; passwords and
//! local content hashes stay in the main process' data directory.

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
    let temporary = path.with_file_name("webdav-sync.json.tmp");
    let content = serde_json::to_vec_pretty(config)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    fs::write(&temporary, content).map_err(|error| AppError::Storage(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&temporary, path).map_err(|error| AppError::Storage(error.to_string()))
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
    let profiles = profiles
        .iter()
        .map(profile_ops::strip_secret_fields_public)
        .collect::<Vec<_>>();
    let profile_bytes = serde_json::to_vec(&profiles)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let content_hash = sha256_hex(&profile_bytes);
    let payload = serde_json::json!({
        "schemaVersion": 1,
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
    for key in [
        "id",
        "parentId",
        "order",
        "lastUsedAt",
        "password",
        "passphrase",
        "privateKeyPath",
    ] {
        object.remove(key);
    }
    if let Some(proxy) = object.get_mut("proxy").and_then(Value::as_object_mut) {
        proxy.remove("password");
    }
    Ok(Value::Object(object))
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
    Ok(public_config(&next))
}

pub async fn upload(app: &AppHandle) -> Result<Value, AppError> {
    let mut config = configured(app)?;
    let remote = remote_url(&config)?;
    let client = client()?;
    let head = authenticated(client.head(remote.clone()), &config)
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
    let (payload, content_hash) = export_bundle(app)?;
    let mut request = authenticated(
        client
            .put(remote)
            .header("content-type", "application/json; charset=utf-8")
            .body(payload),
        &config,
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
    config.last_etag = etag(response.headers()).or(remote_etag);
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(content_hash);
    write_config(app, &config)?;
    Ok(serde_json::json!({ "action": "upload", "message": "连接配置已上传到 WebDAV。" }))
}

pub async fn download(app: &AppHandle) -> Result<Value, AppError> {
    let mut config = configured(app)?;
    let client = client()?;
    let response = authenticated(client.get(remote_url(&config)?), &config)
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
    let profiles = parse_bundle(&bytes)?;
    let (existing, _) = profile_ops::read_and_heal_profiles(app)?;
    let mut known = existing
        .iter()
        .filter_map(profile_fingerprint)
        .collect::<std::collections::HashSet<_>>();
    let mut imported = 0_u64;
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
        if !known.insert(fingerprint) {
            skipped += 1;
            continue;
        }
        profile_ops::create_profile(app, sanitized)?;
        imported += 1;
    }
    config.last_etag = remote_etag;
    config.last_synced_at = Some(export_timestamp());
    config.content_hash = Some(sha256_hex(&bytes));
    write_config(app, &config)?;
    Ok(serde_json::json!({
        "action": "download",
        "message": format!("已从 WebDAV 导入 {imported} 个连接；跳过 {skipped} 个重复或无效项。"),
        "imported": imported,
        "skipped": skipped,
    }))
}

#[cfg(test)]
mod tests {
    use super::{normalize_remote_path, parse_bundle, sha256_hex};
    use serde_json::json;

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
}
