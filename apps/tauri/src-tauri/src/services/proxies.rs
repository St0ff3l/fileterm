use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::AppError;

const PROXY_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredProxyProfile {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String, // "socks5" | "http"
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProxyInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProxyInput {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub proxy_type: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    pub success: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct StoredProxySecrets {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    passwords: HashMap<String, String>,
}

struct ProxyPaths {
    storage_root: PathBuf,
    proxies_path: PathBuf,
    secrets_path: PathBuf,
}

impl ProxyPaths {
    fn for_app(app: &AppHandle) -> Result<Self, AppError> {
        let proxies_path = crate::storage::workspace_file(app, "proxies.json")?;
        let base_dir = proxies_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Storage("无法解析代理配置存储目录".to_string()))?;
        Ok(Self {
            storage_root: base_dir.clone(),
            secrets_path: base_dir.join("proxy-secrets.json"),
            proxies_path,
        })
    }

    fn for_root(storage_root: &Path) -> Self {
        Self {
            storage_root: storage_root.to_path_buf(),
            proxies_path: storage_root.join("proxies.json"),
            secrets_path: storage_root.join("proxy-secrets.json"),
        }
    }
}

pub fn list(app: &AppHandle) -> Result<Vec<Value>, AppError> {
    let paths = ProxyPaths::for_app(app)?;
    let proxies = read_proxies(&paths)?;
    let secrets = read_secrets(&paths).unwrap_or_default();
    let usage = proxy_usage_info(app)?;

    let list = proxies
        .into_iter()
        .map(|proxy| {
            let info = usage.get(&proxy.id);
            let usage_count = info.map(|(c, _)| *c).unwrap_or(0);
            let connection_names = info.map(|(_, names)| names.clone()).unwrap_or_default();
            let has_password = secrets.passwords.contains_key(&proxy.id);

            serde_json::json!({
                "id": proxy.id,
                "name": proxy.name,
                "type": proxy.proxy_type,
                "host": proxy.host,
                "port": proxy.port,
                "username": proxy.username,
                "hasPassword": has_password,
                "usageCount": usage_count,
                "boundConnectionNames": connection_names,
                "createdAt": proxy.created_at,
                "updatedAt": proxy.updated_at,
            })
        })
        .collect();

    Ok(list)
}

pub fn get_internal(
    storage_root: &Path,
    proxy_id: &str,
) -> Result<Option<StoredProxyProfile>, AppError> {
    let paths = ProxyPaths::for_root(storage_root);
    let proxies = read_proxies(&paths)?;
    Ok(proxies.into_iter().find(|p| p.id == proxy_id))
}

pub fn get_secret_internal(
    storage_root: &Path,
    proxy_id: &str,
) -> Result<Option<String>, AppError> {
    let paths = ProxyPaths::for_root(storage_root);
    let secrets = read_secrets(&paths)?;
    let Some(stored_value) = secrets.passwords.get(proxy_id) else {
        return Ok(None);
    };
    let scope = format!("proxy/{proxy_id}/password");
    let (value, _) = crate::services::secret_crypto::decrypt_or_migrate(
        &paths.storage_root,
        &scope,
        stored_value,
    )?;
    Ok(Some(value))
}

pub fn save(app: &AppHandle, input: SaveProxyInput) -> Result<Value, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Command("请输入代理名称".to_string()));
    }
    let host = input.host.trim().to_string();
    if host.is_empty() {
        return Err(AppError::Command("请输入代理服务器地址".to_string()));
    }
    if input.port == 0 {
        return Err(AppError::Command("请输入有效的代理端口".to_string()));
    }
    if !matches!(input.proxy_type.as_str(), "socks5" | "http") {
        return Err(AppError::Command(
            "不支持的代理类型，仅支持 SOCKS5 和 HTTP CONNECT".to_string(),
        ));
    }

    let paths = ProxyPaths::for_app(app)?;
    let mut proxies = read_proxies(&paths)?;
    let mut secrets = read_secrets(&paths).unwrap_or_default();
    let now = now_millis();

    let proxy_id = match input.id.filter(|s| !s.trim().is_empty()) {
        Some(id) => {
            let pos = proxies
                .iter()
                .position(|p| p.id == id)
                .ok_or_else(|| AppError::Command("找不到要修改的代理配置".to_string()))?;
            let existing = &mut proxies[pos];
            existing.name = name.clone();
            existing.proxy_type = input.proxy_type.clone();
            existing.host = host.clone();
            existing.port = input.port;
            existing.username = input.username.filter(|u| !u.trim().is_empty());
            existing.updated_at = now;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            let new_proxy = StoredProxyProfile {
                id: id.clone(),
                name: name.clone(),
                proxy_type: input.proxy_type.clone(),
                host: host.clone(),
                port: input.port,
                username: input.username.filter(|u| !u.trim().is_empty()),
                created_at: now,
                updated_at: now,
            };
            proxies.push(new_proxy);
            id
        }
    };

    if let Some(password) = input.password {
        let trimmed = password.trim();
        if trimmed.is_empty() {
            secrets.passwords.remove(&proxy_id);
        } else {
            let scope = format!("proxy/{proxy_id}/password");
            let encrypted =
                crate::services::secret_crypto::encrypt(&paths.storage_root, &scope, trimmed)?;
            secrets.passwords.insert(proxy_id.clone(), encrypted);
        }
        write_secrets(&paths, &secrets)?;
    }

    write_proxies(&paths, &proxies)?;
    let _ = app.emit("proxies:changed", ());

    let usage = proxy_usage_info(app)?;
    let info = usage.get(&proxy_id);
    let usage_count = info.map(|(c, _)| *c).unwrap_or(0);
    let connection_names = info.map(|(_, names)| names.clone()).unwrap_or_default();
    let has_password = secrets.passwords.contains_key(&proxy_id);

    let saved = proxies.into_iter().find(|p| p.id == proxy_id).unwrap();
    Ok(serde_json::json!({
        "id": saved.id,
        "name": saved.name,
        "type": saved.proxy_type,
        "host": saved.host,
        "port": saved.port,
        "username": saved.username,
        "hasPassword": has_password,
        "usageCount": usage_count,
        "boundConnectionNames": connection_names,
        "createdAt": saved.created_at,
        "updatedAt": saved.updated_at,
    }))
}

pub fn delete(app: &AppHandle, proxy_id: &str) -> Result<(), AppError> {
    let paths = ProxyPaths::for_app(app)?;
    let mut proxies = read_proxies(&paths)?;
    let position = proxies
        .iter()
        .position(|p| p.id == proxy_id)
        .ok_or_else(|| AppError::Command("要删除的代理不存在".to_string()))?;

    let usage = proxy_usage_info(app)?;
    if let Some((count, names)) = usage.get(proxy_id) {
        if *count > 0 {
            return Err(AppError::Command(format!(
                "该代理正在被以下 {} 个连接使用，请先解绑再删除：{}",
                count,
                names.join("、")
            )));
        }
    }

    proxies.remove(position);
    write_proxies(&paths, &proxies)?;

    let mut secrets = read_secrets(&paths).unwrap_or_default();
    if secrets.passwords.remove(proxy_id).is_some() {
        write_secrets(&paths, &secrets)?;
    }
    let _ = app.emit("proxies:changed", ());

    Ok(())
}

pub async fn test_proxy(
    app: &AppHandle,
    input: TestProxyInput,
) -> Result<ProxyTestResult, AppError> {
    let (proxy_type, host, port) = if let Some(id) = input.id.filter(|s| !s.trim().is_empty()) {
        let paths = ProxyPaths::for_app(app)?;
        let proxies = read_proxies(&paths)?;
        let stored = proxies
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::Command("找不到指定的代理配置".to_string()))?;
        (stored.proxy_type, stored.host, stored.port)
    } else {
        let pt = input
            .proxy_type
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "socks5".to_string());
        let h = input
            .host
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| AppError::Command("请填写代理主机地址".to_string()))?;
        let p = input
            .port
            .filter(|&p| p > 0)
            .ok_or_else(|| AppError::Command("请填写有效的代理端口".to_string()))?;
        (pt, h, p)
    };

    let start = Instant::now();
    let addr = format!("{}:{}", host, port);

    let connect_result = timeout(PROXY_CONNECT_TIMEOUT, TcpStream::connect(&addr)).await;
    let mut stream = match connect_result {
        Ok(Ok(stream)) => stream,
        Ok(Err(err)) => {
            return Ok(ProxyTestResult {
                success: false,
                latency_ms: None,
                error: Some(format!("无法连接到代理服务器 ({err})")),
            });
        }
        Err(_) => {
            return Ok(ProxyTestResult {
                success: false,
                latency_ms: None,
                error: Some(format!(
                    "连接代理服务器超时 (超过 {} 秒)",
                    PROXY_CONNECT_TIMEOUT.as_secs()
                )),
            });
        }
    };

    let latency = start.elapsed().as_millis() as u64;

    if proxy_type == "socks5" {
        // SOCKS5 握手测试: 发送 [VER 0x05, NMETHODS 1, METHOD 0x00 (NO AUTH)]
        let greeting = [0x05, 0x01, 0x00];
        if let Err(err) = timeout(Duration::from_secs(3), stream.write_all(&greeting)).await {
            return Ok(ProxyTestResult {
                success: false,
                latency_ms: None,
                error: Some(format!("SOCKS5 握手写入失败: {err}")),
            });
        }
        let mut response = [0u8; 2];
        match timeout(Duration::from_secs(3), stream.read_exact(&mut response)).await {
            Ok(Ok(_)) => {
                if response[0] != 0x05 {
                    return Ok(ProxyTestResult {
                        success: false,
                        latency_ms: None,
                        error: Some("目标端口响应但不是 SOCKS5 代理".to_string()),
                    });
                }
            }
            Ok(Err(err)) => {
                return Ok(ProxyTestResult {
                    success: false,
                    latency_ms: None,
                    error: Some(format!("SOCKS5 握手读取失败: {err}")),
                });
            }
            Err(_) => {
                return Ok(ProxyTestResult {
                    success: false,
                    latency_ms: None,
                    error: Some("SOCKS5 握手读取超时".to_string()),
                });
            }
        }
    }

    Ok(ProxyTestResult {
        success: true,
        latency_ms: Some(latency),
        error: None,
    })
}

fn read_proxies(paths: &ProxyPaths) -> Result<Vec<StoredProxyProfile>, AppError> {
    if !paths.proxies_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&paths.proxies_path)
        .map_err(|error| AppError::Storage(format!("无法读取代理配置文件: {error}")))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content)
        .map_err(|error| AppError::Storage(format!("代理配置 JSON 解析失败: {error}")))
}

fn write_proxies(paths: &ProxyPaths, proxies: &[StoredProxyProfile]) -> Result<(), AppError> {
    let content = serde_json::to_string_pretty(proxies)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let temporary = paths
        .storage_root
        .join(format!("proxies.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, content)
        .map_err(|error| AppError::Storage(format!("无法写入代理临时文件: {error}")))?;
    crate::storage::replace_file_atomically(&temporary, &paths.proxies_path)
}

fn read_secrets(paths: &ProxyPaths) -> Result<StoredProxySecrets, AppError> {
    if !paths.secrets_path.exists() {
        return Ok(StoredProxySecrets::default());
    }
    lock_down_file(&paths.secrets_path)?;
    let content = fs::read_to_string(&paths.secrets_path)
        .map_err(|error| AppError::Storage(format!("无法读取代理密码文件: {error}")))?;
    if content.trim().is_empty() {
        return Ok(StoredProxySecrets::default());
    }
    serde_json::from_str(&content)
        .map_err(|error| AppError::Storage(format!("代理密码 JSON 解析失败: {error}")))
}

fn write_secrets(paths: &ProxyPaths, secrets: &StoredProxySecrets) -> Result<(), AppError> {
    let content = serde_json::to_string_pretty(secrets)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let temporary = paths
        .storage_root
        .join(format!("proxy-secrets.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, content)
        .map_err(|error| AppError::Storage(format!("无法写入代理密码临时文件: {error}")))?;
    crate::storage::replace_file_atomically(&temporary, &paths.secrets_path)?;
    lock_down_file(&paths.secrets_path)
}

fn proxy_usage_info(app: &AppHandle) -> Result<HashMap<String, (usize, Vec<String>)>, AppError> {
    let mut usage = HashMap::new();
    let profiles = crate::storage::read_json_array(app, "profiles.json")?;
    for profile in profiles {
        if let Some(proxy_id) = profile.get("proxyProfileId").and_then(|v| v.as_str()) {
            let name = profile
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("未命名连接")
                .to_string();
            let entry = usage
                .entry(proxy_id.to_string())
                .or_insert_with(|| (0, Vec::new()));
            entry.0 += 1;
            entry.1.push(name);
        }
    }
    Ok(usage)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(unix)]
fn lock_down_file(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(not(unix))]
fn lock_down_file(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_stored_proxy_profile_serialization() {
        let proxy = StoredProxyProfile {
            id: "test-id-1".to_string(),
            name: "My SOCKS5".to_string(),
            proxy_type: "socks5".to_string(),
            host: "127.0.0.1".to_string(),
            port: 1080,
            username: Some("user".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };
        let json = serde_json::to_string(&proxy).unwrap();
        assert!(json.contains("\"type\":\"socks5\""));
        assert!(json.contains("\"host\":\"127.0.0.1\""));
        assert!(json.contains("\"port\":1080"));
        let decoded: StoredProxyProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, proxy);
    }

    #[test]
    fn validates_proxies_file_storage_and_secrets() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-proxy-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        let paths = ProxyPaths::for_root(&directory);

        // Initially empty
        let list = read_proxies(&paths).unwrap();
        assert!(list.is_empty());

        // Write proxy
        let proxy = StoredProxyProfile {
            id: "p1".to_string(),
            name: "Dev Proxy".to_string(),
            proxy_type: "http".to_string(),
            host: "10.0.0.1".to_string(),
            port: 8080,
            username: Some("admin".to_string()),
            created_at: 12345,
            updated_at: 12345,
        };
        write_proxies(&paths, std::slice::from_ref(&proxy)).unwrap();

        let loaded = read_proxies(&paths).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], proxy);

        // Internal lookup
        let found = get_internal(&directory, "p1").unwrap();
        assert_eq!(found, Some(proxy));
        let not_found = get_internal(&directory, "nonexistent").unwrap();
        assert_eq!(not_found, None);

        // Write and read secrets
        let mut secrets = StoredProxySecrets::default();
        let encrypted = crate::services::secret_crypto::encrypt(
            &directory,
            "proxy/p1/password",
            "secret-pass-123",
        )
        .unwrap();
        secrets.passwords.insert("p1".to_string(), encrypted);
        write_secrets(&paths, &secrets).unwrap();

        let secret_val = get_secret_internal(&directory, "p1").unwrap();
        assert_eq!(secret_val.as_deref(), Some("secret-pass-123"));

        fs::remove_dir_all(directory).expect("fixture directory should be removed");
    }
}
