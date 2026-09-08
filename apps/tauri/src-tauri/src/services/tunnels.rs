use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};

use crate::AppError;

const MAX_SCRIPT_URL_LENGTH: usize = 2048;
const MIN_HTTP_TUNNEL_TIMEOUT_SECONDS: u64 = 1;
const MAX_HTTP_TUNNEL_TIMEOUT_SECONDS: u64 = 300;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredTunnelProfile {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub tunnel_type: String,
    #[serde(default)]
    pub forwards: Vec<Value>,
    #[serde(default)]
    pub script_url: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTunnelInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type")]
    pub tunnel_type: String,
    #[serde(default)]
    pub forwards: Option<Vec<Value>>,
    #[serde(default)]
    pub script_url: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// `None` keeps an existing token; `Some(None)` explicitly clears it.
    #[serde(default)]
    pub token: Option<Option<String>>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct StoredTunnelSecrets {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    tokens: HashMap<String, String>,
}

struct TunnelPaths {
    storage_root: PathBuf,
    tunnels_path: PathBuf,
    secrets_path: PathBuf,
}

impl TunnelPaths {
    fn for_app(app: &AppHandle) -> Result<Self, AppError> {
        let tunnels_path = crate::storage::workspace_file(app, "tunnels.json")?;
        let storage_root = tunnels_path
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Storage("无法解析隧道配置存储目录".to_string()))?;
        Ok(Self {
            secrets_path: storage_root.join("tunnel-secrets.json"),
            storage_root,
            tunnels_path,
        })
    }

    fn for_root(storage_root: &Path) -> Self {
        Self {
            storage_root: storage_root.to_path_buf(),
            tunnels_path: storage_root.join("tunnels.json"),
            secrets_path: storage_root.join("tunnel-secrets.json"),
        }
    }
}

pub fn list(app: &AppHandle) -> Result<Vec<Value>, AppError> {
    let paths = TunnelPaths::for_app(app)?;
    let tunnels = read_tunnels(&paths)?;
    let secrets = read_secrets(&paths).unwrap_or_default();
    let usage = tunnel_usage_info(app)?;

    Ok(tunnels
        .into_iter()
        .map(|tunnel| public_value(&tunnel, &secrets, usage.get(&tunnel.id)))
        .collect())
}

pub fn get_internal(
    storage_root: &Path,
    tunnel_id: &str,
) -> Result<Option<StoredTunnelProfile>, AppError> {
    let paths = TunnelPaths::for_root(storage_root);
    Ok(read_tunnels(&paths)?
        .into_iter()
        .find(|tunnel| tunnel.id == tunnel_id))
}

pub fn get_secret_internal(
    storage_root: &Path,
    tunnel_id: &str,
) -> Result<Option<String>, AppError> {
    let paths = TunnelPaths::for_root(storage_root);
    let secrets = read_secrets(&paths)?;
    let Some(stored_value) = secrets.tokens.get(tunnel_id) else {
        return Ok(None);
    };
    let scope = format!("tunnel/{tunnel_id}/token");
    let (value, _) = crate::services::secret_crypto::decrypt_or_migrate(
        &paths.storage_root,
        &scope,
        stored_value,
    )?;
    Ok(Some(value))
}

pub fn save(app: &AppHandle, input: SaveTunnelInput) -> Result<Value, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Command("请输入隧道名称".to_string()));
    }
    if !matches!(input.tunnel_type.as_str(), "ssh" | "http") {
        return Err(AppError::Command(
            "不支持的隧道类型，仅支持 SSH 和 HTTP".to_string(),
        ));
    }

    let forwards = if input.tunnel_type == "ssh" {
        let forwards = input.forwards.unwrap_or_default();
        validate_forwards(&forwards)?;
        forwards
    } else {
        Vec::new()
    };
    let script_url = if input.tunnel_type == "http" {
        Some(validate_script_url(
            input.script_url.as_deref().unwrap_or(""),
        )?)
    } else {
        None
    };
    let timeout_seconds = if input.tunnel_type == "http" {
        let timeout = input.timeout_seconds.unwrap_or(30);
        if !(MIN_HTTP_TUNNEL_TIMEOUT_SECONDS..=MAX_HTTP_TUNNEL_TIMEOUT_SECONDS).contains(&timeout) {
            return Err(AppError::Command(format!(
                "HTTP 隧道超时必须在 {} 到 {} 秒之间",
                MIN_HTTP_TUNNEL_TIMEOUT_SECONDS, MAX_HTTP_TUNNEL_TIMEOUT_SECONDS
            )));
        }
        Some(timeout)
    } else {
        None
    };

    let paths = TunnelPaths::for_app(app)?;
    let mut tunnels = read_tunnels(&paths)?;
    let mut secrets = read_secrets(&paths).unwrap_or_default();
    let now = now_millis();
    let tunnel_id = match input.id.filter(|id| !id.trim().is_empty()) {
        Some(id) => {
            let position = tunnels
                .iter()
                .position(|tunnel| tunnel.id == id)
                .ok_or_else(|| AppError::Command("找不到要修改的隧道配置".to_string()))?;
            let existing = &mut tunnels[position];
            existing.name = name.clone();
            existing.tunnel_type = input.tunnel_type.clone();
            existing.forwards = forwards.clone();
            existing.script_url = script_url.clone();
            existing.timeout_seconds = timeout_seconds;
            existing.updated_at = now;
            id
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            let tunnel = StoredTunnelProfile {
                id: id.clone(),
                name: name.clone(),
                tunnel_type: input.tunnel_type.clone(),
                forwards: forwards.clone(),
                script_url: script_url.clone(),
                timeout_seconds,
                created_at: now,
                updated_at: now,
            };
            tunnels.insert(0, tunnel);
            id
        }
    };

    let mut secrets_changed = false;
    if input.tunnel_type == "ssh" {
        // SSH forwarding profiles never carry HTTP relay credentials. Clear
        // a stale token when an existing profile changes type so the two
        // tunnel models cannot accidentally share authentication state.
        secrets_changed = secrets.tokens.remove(&tunnel_id).is_some();
    } else if let Some(token) = input.token {
        match token.filter(|token| !token.trim().is_empty()) {
            Some(token) => {
                let scope = format!("tunnel/{tunnel_id}/token");
                let encrypted = crate::services::secret_crypto::encrypt(
                    &paths.storage_root,
                    &scope,
                    token.trim(),
                )?;
                secrets.tokens.insert(tunnel_id.clone(), encrypted);
                secrets_changed = true;
            }
            None => {
                secrets_changed = secrets.tokens.remove(&tunnel_id).is_some();
            }
        }
    }
    if secrets_changed {
        write_secrets(&paths, &secrets)?;
    }

    write_tunnels(&paths, &tunnels)?;
    let _ = app.emit("tunnels:changed", ());

    let usage = tunnel_usage_info(app)?;
    let saved = tunnels
        .into_iter()
        .find(|tunnel| tunnel.id == tunnel_id)
        .ok_or_else(|| AppError::Storage("隧道配置保存后无法读取".to_string()))?;
    Ok(public_value(&saved, &secrets, usage.get(&saved.id)))
}

pub fn delete(app: &AppHandle, tunnel_id: &str) -> Result<(), AppError> {
    let paths = TunnelPaths::for_app(app)?;
    let mut tunnels = read_tunnels(&paths)?;
    let position = tunnels
        .iter()
        .position(|tunnel| tunnel.id == tunnel_id)
        .ok_or_else(|| AppError::Command("要删除的隧道不存在".to_string()))?;
    let usage = tunnel_usage_info(app)?;
    if let Some((count, names)) = usage.get(tunnel_id) {
        if *count > 0 {
            return Err(AppError::Command(format!(
                "该隧道正在被以下 {} 个连接使用，请先解绑再删除：{}",
                count,
                names.join("、")
            )));
        }
    }

    tunnels.remove(position);
    write_tunnels(&paths, &tunnels)?;
    let mut secrets = read_secrets(&paths).unwrap_or_default();
    if secrets.tokens.remove(tunnel_id).is_some() {
        write_secrets(&paths, &secrets)?;
    }
    let _ = app.emit("tunnels:changed", ());
    Ok(())
}

fn public_value(
    tunnel: &StoredTunnelProfile,
    secrets: &StoredTunnelSecrets,
    usage: Option<&(usize, Vec<String>)>,
) -> Value {
    let (usage_count, connection_names) = usage
        .map(|(count, names)| (*count, names.clone()))
        .unwrap_or_default();
    serde_json::json!({
        "id": tunnel.id,
        "name": tunnel.name,
        "type": tunnel.tunnel_type,
        "forwards": tunnel.forwards,
        "scriptUrl": tunnel.script_url,
        "timeoutSeconds": tunnel.timeout_seconds,
        "hasToken": secrets.tokens.contains_key(&tunnel.id),
        "usageCount": usage_count,
        "boundConnectionNames": connection_names,
        "createdAt": tunnel.created_at,
        "updatedAt": tunnel.updated_at,
    })
}

fn validate_forwards(forwards: &[Value]) -> Result<(), AppError> {
    for forward in forwards {
        let object = forward
            .as_object()
            .ok_or_else(|| AppError::Command("SSH 隧道规则格式无效".to_string()))?;
        let kind = object
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Command("SSH 隧道规则缺少类型".to_string()))?;
        if !matches!(kind, "local" | "remote" | "dynamic") {
            return Err(AppError::Command("SSH 隧道规则类型无效".to_string()));
        }
        let bind_host = object
            .get("bindHost")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|host| !host.is_empty())
            .ok_or_else(|| AppError::Command("SSH 隧道监听地址不能为空".to_string()))?;
        validate_text(bind_host, "SSH 隧道监听地址", 255)?;
        let bind_port = object
            .get("bindPort")
            .and_then(Value::as_u64)
            .ok_or_else(|| AppError::Command("SSH 隧道监听端口无效".to_string()))?;
        if bind_port > u64::from(u16::MAX) {
            return Err(AppError::Command(
                "SSH 隧道监听端口必须在 0 到 65535 之间".to_string(),
            ));
        }
        if kind != "dynamic" {
            let target_host = object
                .get("targetHost")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .ok_or_else(|| AppError::Command("SSH 隧道目标地址不能为空".to_string()))?;
            validate_text(target_host, "SSH 隧道目标地址", 255)?;
            let target_port = object
                .get("targetPort")
                .and_then(Value::as_u64)
                .ok_or_else(|| AppError::Command("SSH 隧道目标端口无效".to_string()))?;
            if !(1..=u64::from(u16::MAX)).contains(&target_port) {
                return Err(AppError::Command(
                    "SSH 隧道目标端口必须在 1 到 65535 之间".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_script_url(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Command("请输入 HTTP 隧道脚本 URL".to_string()));
    }
    if value.len() > MAX_SCRIPT_URL_LENGTH || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err(AppError::Command("HTTP 隧道脚本 URL 无效".to_string()));
    }
    let url = url::Url::parse(value)
        .map_err(|_| AppError::Command("HTTP 隧道脚本 URL 无效".to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(AppError::Command(
            "HTTP 隧道脚本 URL 必须是带主机的 HTTP 或 HTTPS 地址".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn validate_text(value: &str, label: &str, max_len: usize) -> Result<(), AppError> {
    if value.len() > max_len || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(AppError::Command(format!("{label}无效")));
    }
    Ok(())
}

fn read_tunnels(paths: &TunnelPaths) -> Result<Vec<StoredTunnelProfile>, AppError> {
    if !paths.tunnels_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&paths.tunnels_path)
        .map_err(|error| AppError::Storage(format!("无法读取隧道配置文件: {error}")))?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&content)
        .map_err(|error| AppError::Storage(format!("隧道配置 JSON 解析失败: {error}")))
}

fn write_tunnels(paths: &TunnelPaths, tunnels: &[StoredTunnelProfile]) -> Result<(), AppError> {
    let content = serde_json::to_string_pretty(tunnels)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let temporary = paths
        .storage_root
        .join(format!("tunnels.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, content)
        .map_err(|error| AppError::Storage(format!("无法写入隧道临时文件: {error}")))?;
    crate::storage::replace_file_atomically(&temporary, &paths.tunnels_path)
}

fn read_secrets(paths: &TunnelPaths) -> Result<StoredTunnelSecrets, AppError> {
    if !paths.secrets_path.exists() {
        return Ok(StoredTunnelSecrets::default());
    }
    lock_down_file(&paths.secrets_path)?;
    let content = fs::read_to_string(&paths.secrets_path)
        .map_err(|error| AppError::Storage(format!("无法读取隧道令牌文件: {error}")))?;
    if content.trim().is_empty() {
        return Ok(StoredTunnelSecrets::default());
    }
    serde_json::from_str(&content)
        .map_err(|error| AppError::Storage(format!("隧道令牌 JSON 解析失败: {error}")))
}

fn write_secrets(paths: &TunnelPaths, secrets: &StoredTunnelSecrets) -> Result<(), AppError> {
    let content = serde_json::to_string_pretty(secrets)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let temporary = paths
        .storage_root
        .join(format!("tunnel-secrets.{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, content)
        .map_err(|error| AppError::Storage(format!("无法写入隧道令牌临时文件: {error}")))?;
    crate::storage::replace_file_atomically(&temporary, &paths.secrets_path)?;
    lock_down_file(&paths.secrets_path)
}

fn tunnel_usage_info(app: &AppHandle) -> Result<HashMap<String, (usize, Vec<String>)>, AppError> {
    let mut usage = HashMap::new();
    for profile in crate::storage::read_json_array(app, "profiles.json")? {
        let Some(tunnel_id) = profile.get("tunnelProfileId").and_then(Value::as_str) else {
            continue;
        };
        let name = profile
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("未命名连接")
            .to_string();
        let entry = usage
            .entry(tunnel_id.to_string())
            .or_insert_with(|| (0, Vec::new()));
        entry.0 += 1;
        entry.1.push(name);
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
    fn validates_ssh_forward_rules() {
        let rules = vec![serde_json::json!({
            "id": "forward-1",
            "kind": "local",
            "bindHost": "127.0.0.1",
            "bindPort": 8080,
            "targetHost": "internal.example.com",
            "targetPort": 443,
            "autoStart": true
        })];
        validate_forwards(&rules).unwrap();
    }

    #[test]
    fn rejects_invalid_http_tunnel_url() {
        assert!(validate_script_url("127.0.0.1:8080/tunnel").is_err());
        assert!(validate_script_url("https://example.com/tunnel.php").is_ok());
    }

    #[test]
    fn serializes_tunnel_profile_without_token() {
        let tunnel = StoredTunnelProfile {
            id: "tunnel-1".to_string(),
            name: "HTTP Relay".to_string(),
            tunnel_type: "http".to_string(),
            forwards: Vec::new(),
            script_url: Some("https://example.com/tunnel.php".to_string()),
            timeout_seconds: Some(30),
            created_at: 1,
            updated_at: 2,
        };
        let json = serde_json::to_string(&tunnel).unwrap();
        assert!(!json.contains("token"));
        assert!(json.contains("scriptUrl"));
        assert_eq!(
            serde_json::from_str::<StoredTunnelProfile>(&json).unwrap(),
            tunnel
        );
    }
}
