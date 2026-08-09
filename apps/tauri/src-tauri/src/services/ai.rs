//! Conservative AI Provider configuration and connection testing.
//!
//! This module deliberately owns provider credentials and outbound model
//! requests in Rust. The renderer may submit a one-time secret patch, but it
//! can never read a saved API key back from storage.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;
use url::{Host, Url};

use crate::storage::workspace_file;
use crate::AppError;

const PUBLIC_CONFIG_FILE: &str = "ai-providers.json";
const SECRET_CONFIG_FILE: &str = "ai-provider-secrets.json";
const CONFIG_SCHEMA_VERSION: u32 = 1;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_PROVIDER_NAME_LENGTH: usize = 120;
const MAX_MODEL_LENGTH: usize = 240;
const MAX_BASE_URL_LENGTH: usize = 2_048;

/// Provider writes need a single process-wide critical section. Public
/// configuration and secret updates are correlated, and default selection must
/// never observe a concurrent half-update.
static PROVIDER_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiProviderKind {
    OpenaiCompatibleChat,
    OpenaiResponses,
    AnthropicMessages,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    pub is_default: bool,
    pub allow_no_auth: bool,
    pub allow_insecure_http: bool,
}

#[derive(Clone, Debug, Default)]
enum SecretPatchValue {
    #[default]
    Unchanged,
    Clear,
    Replace(String),
}

#[derive(Clone, Debug, Default)]
pub struct AiProviderSecretPatch {
    api_key: SecretPatchValue,
}

impl<'de> Deserialize<'de> for AiProviderSecretPatch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| serde::de::Error::custom("AI secret patch must be an object"))?;
        let api_key = match object.get("apiKey") {
            None => SecretPatchValue::Unchanged,
            Some(Value::Null) => SecretPatchValue::Clear,
            Some(Value::String(value)) if value.trim().is_empty() => SecretPatchValue::Unchanged,
            Some(Value::String(value)) => SecretPatchValue::Replace(value.trim().to_string()),
            Some(_) => {
                return Err(serde::de::Error::custom(
                    "AI secret patch apiKey must be a string or null",
                ))
            }
        };
        Ok(Self { api_key })
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiProviderInput {
    pub provider: AiProviderDraft,
    #[serde(default)]
    pub secrets: Option<AiProviderSecretPatch>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestAiProviderInput {
    pub provider: AiProviderDraft,
    #[serde(default)]
    pub secrets: Option<AiProviderSecretPatch>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSummary {
    pub id: String,
    pub name: String,
    pub kind: AiProviderKind,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    pub has_api_key: bool,
    pub usable: bool,
    pub is_default: bool,
    pub allow_no_auth: bool,
    pub allow_insecure_http: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderTestResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderConfig {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    providers: Vec<StoredAiProvider>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredAiProvider {
    id: String,
    name: String,
    kind: AiProviderKind,
    base_url: String,
    model: String,
    enabled: bool,
    is_default: bool,
    allow_no_auth: bool,
    allow_insecure_http: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderSecrets {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    providers: BTreeMap<String, StoredProviderSecret>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredProviderSecret {
    api_key: String,
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn ai_error(code: &str, message: impl Into<String>) -> AppError {
    AppError::Command(format!("{code}: {}", message.into()))
}

fn store_lock() -> Result<std::sync::MutexGuard<'static, ()>, AppError> {
    PROVIDER_STORE_LOCK
        .lock()
        .map_err(|_| AppError::Storage("AI Provider 配置锁不可用".to_string()))
}

fn public_config_path(app: &AppHandle) -> Result<std::path::PathBuf, AppError> {
    workspace_file(app, PUBLIC_CONFIG_FILE)
}

fn secret_config_path(app: &AppHandle) -> Result<std::path::PathBuf, AppError> {
    workspace_file(app, SECRET_CONFIG_FILE)
}

fn read_public_config(app: &AppHandle) -> Result<StoredProviderConfig, AppError> {
    let path = public_config_path(app)?;
    if !path.exists() {
        return Ok(StoredProviderConfig::default());
    }
    let bytes = fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AppError::Serialization(error.to_string()))
}

fn read_secret_config(app: &AppHandle) -> Result<StoredProviderSecrets, AppError> {
    let path = secret_config_path(app)?;
    if !path.exists() {
        return Ok(StoredProviderSecrets::default());
    }
    let bytes = fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AppError::Serialization(error.to_string()))
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ai-provider-config");
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    crate::storage::write_restricted_file(&temporary, &bytes)?;
    crate::storage::replace_file_atomically(&temporary, path)
}

fn write_public_config(app: &AppHandle, config: &StoredProviderConfig) -> Result<(), AppError> {
    write_json_file(&public_config_path(app)?, config)
}

fn write_secret_config(app: &AppHandle, config: &StoredProviderSecrets) -> Result<(), AppError> {
    write_json_file(&secret_config_path(app)?, config)
}

fn read_normalized_store(
    app: &AppHandle,
) -> Result<(StoredProviderConfig, StoredProviderSecrets), AppError> {
    let mut config = read_public_config(app)?;
    let secrets = read_secret_config(app)?;
    config.schema_version = CONFIG_SCHEMA_VERSION;
    if repair_default_provider(&mut config, &secrets) {
        write_public_config(app, &config)?;
    }
    Ok((config, secrets))
}

fn normalize_text(value: &str, field: &str, maximum_length: usize) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            format!("{field} 不能为空"),
        ));
    }
    if value.len() > maximum_length {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            format!("{field} 超过长度限制"),
        ));
    }
    Ok(value.to_string())
}

fn normalize_base_url(value: &str, allow_insecure_http: bool) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_BASE_URL_LENGTH {
        return Err(ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"));
    }

    let url = Url::parse(value).map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"))?;
    if !matches!(url.scheme(), "https" | "http") || url.host().is_none() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址必须是包含主机名的 HTTP(S) 地址",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址不得内嵌用户名或密码",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "API 地址不得包含 query 或 fragment",
        ));
    }
    if url.scheme() == "http" && !allow_insecure_http {
        return Err(ai_error(
            "AI_PROVIDER_INSECURE_HTTP",
            "HTTP 连接需要明确启用不安全连接选项",
        ));
    }

    let path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if ["/chat/completions", "/responses", "/messages"]
        .iter()
        .any(|endpoint| path.ends_with(endpoint))
    {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_URL",
            "请填写 API root，不要填写具体的请求 endpoint",
        ));
    }

    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn is_trusted_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(host)) => host.octets()[0] == 127,
        Some(Host::Ipv6(host)) => host.is_loopback(),
        None => false,
    }
}

fn normalize_provider(draft: AiProviderDraft, id: String) -> Result<StoredAiProvider, AppError> {
    let name = normalize_text(&draft.name, "Provider 名称", MAX_PROVIDER_NAME_LENGTH)?;
    let model = normalize_text(&draft.model, "模型名称", MAX_MODEL_LENGTH)?;
    let base_url = normalize_base_url(&draft.base_url, draft.allow_insecure_http)?;
    let parsed =
        Url::parse(&base_url).map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"))?;
    if draft.allow_no_auth && !is_trusted_loopback(&parsed) {
        return Err(ai_error(
            "AI_PROVIDER_AUTH_REQUIRED",
            "无 API Key 仅允许可信 loopback 地址",
        ));
    }

    Ok(StoredAiProvider {
        id,
        name,
        kind: draft.kind,
        base_url,
        model,
        enabled: draft.enabled,
        is_default: draft.is_default,
        allow_no_auth: draft.allow_no_auth,
        allow_insecure_http: draft.allow_insecure_http,
    })
}

fn has_api_key(secrets: &StoredProviderSecrets, provider_id: &str) -> bool {
    secrets
        .providers
        .get(provider_id)
        .is_some_and(|secret| !secret.api_key.trim().is_empty())
}

fn provider_is_usable(provider: &StoredAiProvider, secrets: &StoredProviderSecrets) -> bool {
    if !provider.enabled {
        return false;
    }
    let Ok(base_url) = normalize_base_url(&provider.base_url, provider.allow_insecure_http) else {
        return false;
    };
    let Ok(parsed) = Url::parse(&base_url) else {
        return false;
    };
    if provider.allow_no_auth && !is_trusted_loopback(&parsed) {
        return false;
    }
    if provider.model.trim().is_empty() {
        return false;
    }
    if has_api_key(secrets, &provider.id) {
        return true;
    }
    provider.allow_no_auth
}

fn validate_secret_patch(patch: Option<&AiProviderSecretPatch>) -> Result<(), AppError> {
    let Some(AiProviderSecretPatch {
        api_key: SecretPatchValue::Replace(api_key),
    }) = patch
    else {
        return Ok(());
    };
    if api_key
        .chars()
        .any(|character| matches!(character, '\r' | '\n' | '\0'))
    {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "API Key 不得包含控制换行符",
        ));
    }
    Ok(())
}

fn provider_summary(
    provider: &StoredAiProvider,
    secrets: &StoredProviderSecrets,
) -> AiProviderSummary {
    AiProviderSummary {
        id: provider.id.clone(),
        name: provider.name.clone(),
        kind: provider.kind.clone(),
        base_url: provider.base_url.clone(),
        model: provider.model.clone(),
        enabled: provider.enabled,
        has_api_key: has_api_key(secrets, &provider.id),
        usable: provider_is_usable(provider, secrets),
        is_default: provider.is_default,
        allow_no_auth: provider.allow_no_auth,
        allow_insecure_http: provider.allow_insecure_http,
    }
}

fn default_candidate_key(provider: &StoredAiProvider) -> (String, String) {
    (provider.name.to_ascii_lowercase(), provider.id.clone())
}

fn repair_default_provider(
    config: &mut StoredProviderConfig,
    secrets: &StoredProviderSecrets,
) -> bool {
    let candidate_indices = config
        .providers
        .iter()
        .enumerate()
        .filter_map(|(index, provider)| provider_is_usable(provider, secrets).then_some(index))
        .collect::<Vec<_>>();

    let preferred = candidate_indices
        .iter()
        .copied()
        .filter(|index| config.providers[*index].is_default)
        .min_by_key(|index| default_candidate_key(&config.providers[*index]));
    let fallback = candidate_indices
        .iter()
        .copied()
        .min_by_key(|index| default_candidate_key(&config.providers[*index]));
    let winner = preferred.or(fallback);

    let mut changed = false;
    for (index, provider) in config.providers.iter_mut().enumerate() {
        let should_be_default = winner == Some(index);
        if provider.is_default != should_be_default {
            provider.is_default = should_be_default;
            changed = true;
        }
    }
    changed
}

fn selected_existing_id(
    config: &StoredProviderConfig,
    draft_id: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(draft_id) = draft_id else {
        return Ok(None);
    };
    let id = draft_id.trim();
    if id.is_empty() {
        return Err(ai_error("AI_PROVIDER_INVALID_CONFIG", "Provider ID 无效"));
    }
    config
        .providers
        .iter()
        .find(|provider| provider.id == id)
        .map(|provider| Some(provider.id.clone()))
        .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))
}

fn apply_secret_patch(
    secrets: &mut StoredProviderSecrets,
    provider_id: &str,
    patch: Option<&AiProviderSecretPatch>,
) -> bool {
    let Some(patch) = patch else {
        return false;
    };
    match &patch.api_key {
        SecretPatchValue::Unchanged => false,
        SecretPatchValue::Clear => secrets.providers.remove(provider_id).is_some(),
        SecretPatchValue::Replace(api_key) => {
            let previous = secrets.providers.insert(
                provider_id.to_string(),
                StoredProviderSecret {
                    api_key: api_key.clone(),
                },
            );
            previous
                .as_ref()
                .is_none_or(|previous| previous.api_key != *api_key)
        }
    }
}

pub fn list_providers(app: &AppHandle) -> Result<Vec<AiProviderSummary>, AppError> {
    let _guard = store_lock()?;
    let (config, secrets) = read_normalized_store(app)?;
    Ok(config
        .providers
        .iter()
        .map(|provider| provider_summary(provider, &secrets))
        .collect())
}

pub fn save_provider(
    app: &AppHandle,
    input: SaveAiProviderInput,
) -> Result<AiProviderSummary, AppError> {
    let _guard = store_lock()?;
    let (mut config, mut secrets) = read_normalized_store(app)?;
    let existing_id = selected_existing_id(&config, input.provider.id.as_deref())?;
    let provider_id = existing_id
        .clone()
        .unwrap_or_else(|| crate::storage::new_id("ai-provider"));
    let provider = normalize_provider(input.provider, provider_id.clone())?;
    validate_secret_patch(input.secrets.as_ref())?;

    let secret_changed = apply_secret_patch(&mut secrets, &provider_id, input.secrets.as_ref());
    if let Some(existing_id) = existing_id {
        let index = config
            .providers
            .iter()
            .position(|existing| existing.id == existing_id)
            .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))?;
        config.providers[index] = provider;
    } else {
        config.providers.push(provider);
    }
    config.schema_version = CONFIG_SCHEMA_VERSION;
    repair_default_provider(&mut config, &secrets);

    // Secrets are written before the public reference. If the second write
    // fails, a potential orphan key is still unreachable from the public
    // provider list and can be safely replaced on the next save.
    if secret_changed {
        write_secret_config(app, &secrets)?;
    }
    write_public_config(app, &config)?;

    config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider_summary(provider, &secrets))
        .ok_or_else(|| AppError::Storage("已保存的 AI Provider 不存在".to_string()))
}

pub fn delete_provider(
    app: &AppHandle,
    provider_id: &str,
) -> Result<Vec<AiProviderSummary>, AppError> {
    let _guard = store_lock()?;
    let (mut config, mut secrets) = read_normalized_store(app)?;
    let original_length = config.providers.len();
    config
        .providers
        .retain(|provider| provider.id != provider_id);
    if config.providers.len() == original_length {
        return Err(ai_error(
            "AI_PROVIDER_NOT_FOUND",
            "找不到指定的 AI Provider",
        ));
    }
    let secret_changed = secrets.providers.remove(provider_id).is_some();
    repair_default_provider(&mut config, &secrets);
    write_public_config(app, &config)?;
    if secret_changed {
        write_secret_config(app, &secrets)?;
    }
    Ok(config
        .providers
        .iter()
        .map(|provider| provider_summary(provider, &secrets))
        .collect())
}

fn resolve_test_api_key(
    secrets: &StoredProviderSecrets,
    provider_id: Option<&str>,
    patch: Option<&AiProviderSecretPatch>,
) -> Option<String> {
    let patch_value = patch.map(|patch| &patch.api_key);
    match patch_value {
        Some(SecretPatchValue::Clear) => None,
        Some(SecretPatchValue::Replace(value)) => Some(value.clone()),
        Some(SecretPatchValue::Unchanged) | None => provider_id
            .and_then(|provider_id| secrets.providers.get(provider_id))
            .map(|secret| secret.api_key.trim().to_string())
            .filter(|api_key| !api_key.is_empty()),
    }
}

fn validate_test_provider(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
) -> Result<(), AppError> {
    let parsed = Url::parse(&provider.base_url)
        .map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "API 地址无效"))?;
    if api_key.is_none() && !(provider.allow_no_auth && is_trusted_loopback(&parsed)) {
        return Err(ai_error(
            "AI_PROVIDER_AUTH_REQUIRED",
            "连接测试需要 API Key；无鉴权仅允许可信 loopback 地址",
        ));
    }
    if api_key.is_some_and(|api_key| {
        api_key
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '\0'))
    }) {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "API Key 不得包含换行符",
        ));
    }
    Ok(())
}

pub async fn test_provider(
    app: &AppHandle,
    input: TestAiProviderInput,
) -> Result<AiProviderTestResult, AppError> {
    let (provider, api_key) = {
        let _guard = store_lock()?;
        let (config, secrets) = read_normalized_store(app)?;
        let existing_id = selected_existing_id(&config, input.provider.id.as_deref())?;
        let provider_id = existing_id
            .clone()
            .unwrap_or_else(|| "connection-test".to_string());
        let provider = normalize_provider(input.provider, provider_id)?;
        let api_key =
            resolve_test_api_key(&secrets, existing_id.as_deref(), input.secrets.as_ref());
        validate_test_provider(&provider, api_key.as_deref())?;
        (provider, api_key)
    };

    if provider.kind != AiProviderKind::OpenaiCompatibleChat {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "当前阶段仅支持 OpenAI-compatible Chat 的连接测试",
        ));
    }

    test_openai_compatible_chat(&client(&provider)?, &provider, api_key.as_deref()).await
}

fn client(provider: &StoredAiProvider) -> Result<Client, AppError> {
    let mut builder = Client::builder()
        .connect_timeout(CONNECTION_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none());
    if Url::parse(&provider.base_url)
        .ok()
        .is_some_and(|url| is_trusted_loopback(&url))
    {
        // A local model request must never have its request or API key
        // silently relayed through a developer machine's HTTP proxy.
        builder = builder.no_proxy();
    }
    builder.build().map_err(|_| {
        ai_error(
            "AI_PROVIDER_CONNECTION_FAILED",
            "无法初始化 AI Provider 连接",
        )
    })
}

fn chat_completions_url(provider: &StoredAiProvider) -> Result<Url, AppError> {
    Url::parse(&format!("{}/chat/completions", provider.base_url))
        .map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "无法构造 Provider 请求地址"))
}

async fn test_openai_compatible_chat(
    client: &Client,
    provider: &StoredAiProvider,
    api_key: Option<&str>,
) -> Result<AiProviderTestResult, AppError> {
    let request_url = chat_completions_url(provider)?;
    let mut request = client.post(request_url).json(&json!({
        "model": provider.model,
        "messages": [{
            "role": "user",
            "content": "Reply with exactly OK."
        }],
        "max_tokens": 8,
        "stream": false
    }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            ai_error("AI_PROVIDER_TIMEOUT", "AI Provider 连接测试超时")
        } else {
            ai_error("AI_PROVIDER_CONNECTION_FAILED", "无法连接 AI Provider")
        }
    })?;
    if !response.status().is_success() {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            format!("AI Provider 返回 HTTP {}", response.status()),
        ));
    }
    let payload = response.json::<Value>().await.map_err(|_| {
        ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回有效 JSON 对象",
        )
    })?;
    if !payload.is_object() {
        return Err(ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回有效 JSON 对象",
        ));
    }

    Ok(AiProviderTestResult {
        ok: true,
        message: "连接成功：Provider 已响应测试请求。".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_base_url, provider_is_usable, provider_summary, repair_default_provider,
        test_openai_compatible_chat, write_json_file, AiProviderKind, AiProviderSummary,
        StoredAiProvider, StoredProviderConfig, StoredProviderSecret, StoredProviderSecrets,
    };
    use reqwest::Client;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::fs;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn provider(base_url: &str) -> StoredAiProvider {
        StoredAiProvider {
            id: "provider-1".to_string(),
            name: "Provider".to_string(),
            kind: AiProviderKind::OpenaiCompatibleChat,
            base_url: base_url.to_string(),
            model: "test-model".to_string(),
            enabled: true,
            is_default: false,
            allow_no_auth: false,
            allow_insecure_http: false,
        }
    }

    fn test_client() -> Client {
        Client::builder()
            .no_proxy()
            .build()
            .expect("test client must build")
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket
                .read(&mut byte)
                .await
                .expect("request should be readable");
            assert!(count > 0, "client closed before completing request headers");
            request.extend_from_slice(&byte[..count]);
        }

        let headers = String::from_utf8(request.clone()).expect("headers should be utf-8");
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().expect("content length"))
            })
            .unwrap_or_default();
        let mut body = vec![0_u8; content_length];
        socket
            .read_exact(&mut body)
            .await
            .expect("request body should be readable");
        request.extend_from_slice(&body);
        String::from_utf8(request).expect("request should be utf-8")
    }

    #[test]
    fn rejects_full_protocol_endpoints_and_http_without_opt_in() {
        assert!(normalize_base_url("https://provider.test/v1/chat/completions", false).is_err());
        assert!(normalize_base_url("http://127.0.0.1:11434/v1", false).is_err());
        assert_eq!(
            normalize_base_url("http://127.0.0.1:11434/v1/", true).unwrap(),
            "http://127.0.0.1:11434/v1"
        );
    }

    #[test]
    fn default_repair_uses_a_stable_usable_provider() {
        let mut alpha = provider("https://alpha.test/v1");
        alpha.id = "alpha".to_string();
        alpha.name = "Alpha".to_string();
        alpha.is_default = true;
        let mut beta = provider("https://beta.test/v1");
        beta.id = "beta".to_string();
        beta.name = "Beta".to_string();
        beta.is_default = true;
        let mut secrets = StoredProviderSecrets {
            schema_version: 1,
            providers: BTreeMap::new(),
        };
        secrets.providers.insert(
            "alpha".to_string(),
            StoredProviderSecret {
                api_key: "alpha-key".to_string(),
            },
        );
        secrets.providers.insert(
            "beta".to_string(),
            StoredProviderSecret {
                api_key: "beta-key".to_string(),
            },
        );
        let mut config = StoredProviderConfig {
            schema_version: 1,
            providers: vec![beta, alpha],
        };

        assert!(repair_default_provider(&mut config, &secrets));
        assert_eq!(
            config
                .providers
                .iter()
                .find(|provider| provider.is_default)
                .map(|provider| provider.id.as_str()),
            Some("alpha")
        );
        assert!(provider_is_usable(&config.providers[0], &secrets));
    }

    #[test]
    fn remote_no_auth_configuration_is_never_usable_even_with_a_saved_key() {
        let mut provider = provider("https://provider.test/v1");
        provider.allow_no_auth = true;
        let mut secrets = StoredProviderSecrets {
            schema_version: 1,
            providers: BTreeMap::new(),
        };
        secrets.providers.insert(
            provider.id.clone(),
            StoredProviderSecret {
                api_key: "secret-key".to_string(),
            },
        );

        assert!(!provider_is_usable(&provider, &secrets));
    }

    #[test]
    fn public_summary_uses_the_bridge_contract_without_exposing_the_key() {
        let provider = provider("https://provider.test/v1");
        let mut secrets = StoredProviderSecrets {
            schema_version: 1,
            providers: BTreeMap::new(),
        };
        secrets.providers.insert(
            provider.id.clone(),
            StoredProviderSecret {
                api_key: "secret-key".to_string(),
            },
        );

        let summary: AiProviderSummary = provider_summary(&provider, &secrets);
        let payload = serde_json::to_value(summary).expect("summary should serialize");
        assert_eq!(
            payload,
            json!({
                "id": "provider-1",
                "name": "Provider",
                "kind": "openai-compatible-chat",
                "baseUrl": "https://provider.test/v1",
                "model": "test-model",
                "enabled": true,
                "hasApiKey": true,
                "usable": true,
                "isDefault": false,
                "allowNoAuth": false,
                "allowInsecureHttp": false
            })
        );
        assert!(!payload.to_string().contains("secret-key"));
    }

    #[cfg(unix)]
    #[test]
    fn secret_config_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "fileterm-ai-provider-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        let path = directory.join("ai-provider-secrets.json");
        let secrets = StoredProviderSecrets {
            schema_version: 1,
            providers: BTreeMap::new(),
        };

        write_json_file(&path, &secrets).expect("secret config should be written");
        let mode = fs::metadata(&path)
            .expect("secret config should exist")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        fs::remove_dir_all(directory).expect("fixture directory should be removed");
    }

    #[tokio::test]
    async fn connection_test_uses_a_small_openai_compatible_request() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture should bind");
        let address = listener.local_addr().expect("fixture should have address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("fixture should accept");
            let request = read_request(&mut socket).await;
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer test-key")));
            let body = request
                .split("\r\n\r\n")
                .nth(1)
                .expect("request should include body");
            let body: Value = serde_json::from_str(body).expect("body should be json");
            assert_eq!(body["model"], "test-model");
            assert_eq!(body["max_tokens"], 8);
            assert_eq!(body["stream"], false);
            assert_eq!(body["messages"][0]["content"], "Reply with exactly OK.");

            let response_body = r#"{"id":"test","object":"chat.completion"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("fixture should respond");
        });

        let mut provider = provider(&format!("http://{address}/v1"));
        provider.allow_insecure_http = true;
        let result = test_openai_compatible_chat(&test_client(), &provider, Some("test-key"))
            .await
            .expect("connection test should succeed");
        assert!(result.ok);
        server.await.expect("fixture should finish");
    }
}
