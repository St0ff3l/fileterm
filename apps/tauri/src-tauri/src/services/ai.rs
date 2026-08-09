//! Conservative AI Provider configuration and connection testing.
//!
//! This module deliberately owns provider credentials and outbound model
//! requests in Rust. The renderer may submit a one-time secret patch, but it
//! can never read a saved API key back from storage.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use tauri::{ipc::Channel, AppHandle};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

use crate::storage::workspace_file;
use crate::AppError;

const PUBLIC_CONFIG_FILE: &str = "ai-providers.json";
const SECRET_CONFIG_FILE: &str = "ai-provider-secrets.json";
const CONVERSATION_INDEX_FILE: &str = "ai-conversations.json";
const CONVERSATION_DIRECTORY: &str = "ai-conversations";
const CONFIG_SCHEMA_VERSION: u32 = 1;
const CONVERSATION_SCHEMA_VERSION: u32 = 1;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const CHAT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_PROVIDER_NAME_LENGTH: usize = 120;
const MAX_MODEL_LENGTH: usize = 240;
const MAX_BASE_URL_LENGTH: usize = 2_048;
const MAX_CONVERSATIONS: usize = 50;
const MAX_CONVERSATION_MESSAGES: usize = 200;
const MAX_CONVERSATION_BYTES: usize = 1_048_576;
const MAX_USER_MESSAGE_LENGTH: usize = 16_384;
const MAX_ASSISTANT_MESSAGE_LENGTH: usize = 262_144;
const MAX_HISTORY_CHARACTERS: usize = 48_000;
const MAX_SSE_LINE_BYTES: usize = 1_048_576;
const MAX_CONCURRENT_CHAT_REQUESTS: usize = 2;
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 2_048;

const L0_SYSTEM_PROMPT: &str = "You are FileTerm Copilot, a conservative assistant for developers and operators. This chat has no terminal, host, path, file, credential, or command-execution access. Never claim to have inspected a terminal or executed anything. Explain uncertainty clearly. If you suggest shell commands, make them reviewable and tell the user to inspect and run them manually.";

/// Provider writes need a single process-wide critical section. Public
/// configuration and secret updates are correlated, and default selection must
/// never observe a concurrent half-update.
static PROVIDER_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Conversation files are intentionally independent from workspace snapshots.
/// The renderer can only receive explicit conversation objects; it never gets
/// the provider secret store or an AI HTTP client.
static CONVERSATION_STORE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// One cancellation token per outbound request. The registry has process
/// lifetime because FileTerm has a single Rust service process, while a
/// request's stream channel belongs to exactly one invoking webview.
#[derive(Default)]
struct ActiveChatRequestRegistry {
    by_request: HashMap<String, CancellationToken>,
    by_conversation: HashMap<String, String>,
}

static ACTIVE_CHAT_REQUESTS: LazyLock<Mutex<ActiveChatRequestRegistry>> =
    LazyLock::new(|| Mutex::new(ActiveChatRequestRegistry::default()));
static CHAT_REQUEST_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(MAX_CONCURRENT_CHAT_REQUESTS)));

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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub id: String,
    pub role: AiMessageRole,
    pub content: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiMessageRole {
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationSummary {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversation {
    pub id: String,
    pub title: String,
    pub provider_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub messages: Vec<AiMessage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAiConversationInput {
    pub provider_id: String,
}

/// L0 deliberately has no context field. Terminal and target context must
/// later travel through a separate, Rust-owned one-time snapshot contract.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAiChatInput {
    pub conversation_id: String,
    pub provider_id: String,
    pub user_message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryAiChatInput {
    pub conversation_id: String,
    pub provider_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub request_id: String,
    pub conversation_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AiStreamEvent {
    Started {
        #[serde(rename = "requestId")]
        request_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
    },
    TextDelta {
        text: String,
    },
    Usage {
        #[serde(rename = "inputTokens", skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
        #[serde(rename = "outputTokens", skip_serializing_if = "Option::is_none")]
        output_tokens: Option<u64>,
    },
    Completed {
        conversation: AiConversation,
        #[serde(rename = "finishReason", skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
    },
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConversationIndex {
    #[serde(default = "default_conversation_schema_version")]
    schema_version: u32,
    #[serde(default)]
    conversations: Vec<AiConversationSummary>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredConversation {
    #[serde(default = "default_conversation_schema_version")]
    schema_version: u32,
    id: String,
    title: String,
    provider_id: String,
    created_at: String,
    updated_at: String,
    messages: Vec<AiMessage>,
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

fn default_conversation_schema_version() -> u32 {
    CONVERSATION_SCHEMA_VERSION
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

fn conversation_store_lock() -> Result<std::sync::MutexGuard<'static, ()>, AppError> {
    CONVERSATION_STORE_LOCK
        .lock()
        .map_err(|_| AppError::Storage("AI 对话历史配置锁不可用".to_string()))
}

fn conversation_index_path(app: &AppHandle) -> Result<std::path::PathBuf, AppError> {
    workspace_file(app, CONVERSATION_INDEX_FILE)
}

fn conversation_directory(app: &AppHandle) -> Result<std::path::PathBuf, AppError> {
    workspace_file(app, CONVERSATION_DIRECTORY)
}

fn validate_conversation_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 160
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(ai_error("AI_CONVERSATION_NOT_FOUND", "AI 对话 ID 无效"));
    }
    Ok(value.to_string())
}

fn conversation_file_path(
    app: &AppHandle,
    conversation_id: &str,
) -> Result<std::path::PathBuf, AppError> {
    let conversation_id = validate_conversation_id(conversation_id)?;
    Ok(conversation_directory(app)?.join(format!("{conversation_id}.json")))
}

fn ensure_conversation_directory(app: &AppHandle) -> Result<(), AppError> {
    let directory = conversation_directory(app)?;
    fs::create_dir_all(&directory).map_err(|error| AppError::Storage(error.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

fn read_conversation_index(app: &AppHandle) -> Result<StoredConversationIndex, AppError> {
    let path = conversation_index_path(app)?;
    if !path.exists() {
        return Ok(StoredConversationIndex::default());
    }
    let bytes = fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AppError::Serialization(error.to_string()))
}

fn write_conversation_index(
    app: &AppHandle,
    index: &StoredConversationIndex,
) -> Result<(), AppError> {
    write_json_file(&conversation_index_path(app)?, index)
}

fn read_stored_conversation(
    app: &AppHandle,
    conversation_id: &str,
) -> Result<StoredConversation, AppError> {
    let conversation_id = validate_conversation_id(conversation_id)?;
    let path = conversation_file_path(app, &conversation_id)?;
    if !path.exists() {
        return Err(ai_error(
            "AI_CONVERSATION_NOT_FOUND",
            "找不到指定的 AI 对话",
        ));
    }
    let bytes = fs::read(path).map_err(|error| AppError::Storage(error.to_string()))?;
    let conversation = serde_json::from_slice::<StoredConversation>(&bytes)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    if conversation.id != conversation_id {
        return Err(AppError::Storage("AI 对话文件标识不匹配".to_string()));
    }
    Ok(conversation)
}

fn conversation_summary(conversation: &StoredConversation) -> AiConversationSummary {
    AiConversationSummary {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        provider_id: conversation.provider_id.clone(),
        created_at: conversation.created_at.clone(),
        updated_at: conversation.updated_at.clone(),
        message_count: conversation.messages.len(),
    }
}

fn public_conversation(conversation: StoredConversation) -> AiConversation {
    AiConversation {
        id: conversation.id,
        title: conversation.title,
        provider_id: conversation.provider_id,
        created_at: conversation.created_at,
        updated_at: conversation.updated_at,
        message_count: conversation.messages.len(),
        messages: conversation.messages,
    }
}

fn update_conversation_index(
    index: &mut StoredConversationIndex,
    conversation: &StoredConversation,
) {
    index.schema_version = CONVERSATION_SCHEMA_VERSION;
    let summary = conversation_summary(conversation);
    if let Some(existing) = index
        .conversations
        .iter_mut()
        .find(|existing| existing.id == conversation.id)
    {
        *existing = summary;
    } else {
        index.conversations.push(summary);
    }
}

fn ensure_conversation_fits(conversation: &StoredConversation) -> Result<(), AppError> {
    if conversation.messages.len() > MAX_CONVERSATION_MESSAGES {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "单个 AI 对话已达到消息数量上限，请新建或清理对话",
        ));
    }
    let serialized_size = serde_json::to_vec(conversation)
        .map_err(|error| AppError::Serialization(error.to_string()))?
        .len();
    if serialized_size > MAX_CONVERSATION_BYTES {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "单个 AI 对话已达到本地存储大小上限，请新建或清理对话",
        ));
    }
    Ok(())
}

fn persist_conversation(
    app: &AppHandle,
    index: &mut StoredConversationIndex,
    conversation: &StoredConversation,
) -> Result<(), AppError> {
    ensure_conversation_fits(conversation)?;
    ensure_conversation_directory(app)?;
    // Write the conversation body first. If writing the smaller index fails,
    // the body stays an unreachable local orphan rather than exposing a
    // partially written conversation to the renderer.
    write_json_file(
        &conversation_file_path(app, &conversation.id)?,
        conversation,
    )?;
    update_conversation_index(index, conversation);
    write_conversation_index(app, index)
}

fn now_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn title_from_user_message(message: &str) -> String {
    let compact = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = compact.chars().take(52).collect::<String>();
    if compact.chars().count() > title.chars().count() {
        title.push('…');
    }
    if title.is_empty() {
        "新建 AI 对话".to_string()
    } else {
        title
    }
}

fn normalize_user_message(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ai_error(
            "AI_CONVERSATION_INVALID_INPUT",
            "请输入要发送给 AI 的内容",
        ));
    }
    if value.chars().count() > MAX_USER_MESSAGE_LENGTH {
        return Err(ai_error("AI_CONVERSATION_LIMIT", "单条消息超过长度限制"));
    }
    Ok(value.to_string())
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

fn normalize_provider_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 160 {
        return Err(ai_error("AI_PROVIDER_NOT_FOUND", "AI Provider ID 无效"));
    }
    Ok(value.to_string())
}

fn resolve_chat_provider(
    app: &AppHandle,
    provider_id: &str,
) -> Result<(StoredAiProvider, Option<String>), AppError> {
    let provider_id = normalize_provider_id(provider_id)?;
    let _guard = store_lock()?;
    let (config, secrets) = read_normalized_store(app)?;
    let provider = config
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .cloned()
        .ok_or_else(|| ai_error("AI_PROVIDER_NOT_FOUND", "找不到指定的 AI Provider"))?;
    if !provider_is_usable(&provider, &secrets) {
        return Err(ai_error(
            "AI_PROVIDER_INVALID_CONFIG",
            "AI Provider 不可用，请检查模型、密钥和连接设置",
        ));
    }
    let api_key = secrets
        .providers
        .get(&provider.id)
        .map(|secret| secret.api_key.trim().to_string())
        .filter(|api_key| !api_key.is_empty());
    Ok((provider, api_key))
}

fn require_indexed_conversation(
    index: &StoredConversationIndex,
    conversation_id: &str,
) -> Result<(), AppError> {
    if index
        .conversations
        .iter()
        .any(|conversation| conversation.id == conversation_id)
    {
        Ok(())
    } else {
        Err(ai_error(
            "AI_CONVERSATION_NOT_FOUND",
            "找不到指定的 AI 对话",
        ))
    }
}

pub fn list_conversations(app: &AppHandle) -> Result<Vec<AiConversationSummary>, AppError> {
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    index.schema_version = CONVERSATION_SCHEMA_VERSION;
    index.conversations.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(index.conversations)
}

pub fn get_conversation(
    app: &AppHandle,
    conversation_id: &str,
) -> Result<AiConversation, AppError> {
    let conversation_id = validate_conversation_id(conversation_id)?;
    let _guard = conversation_store_lock()?;
    let index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    read_stored_conversation(app, &conversation_id).map(public_conversation)
}

pub fn create_conversation(
    app: &AppHandle,
    input: CreateAiConversationInput,
) -> Result<AiConversation, AppError> {
    let provider_id = normalize_provider_id(&input.provider_id)?;
    // A conversation always starts against a current, usable provider. This
    // avoids creating dead history entries that can never be sent.
    let _ = resolve_chat_provider(app, &provider_id)?;

    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    if index.conversations.len() >= MAX_CONVERSATIONS {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "AI 对话数量已达到上限，请先删除不再需要的对话",
        ));
    }
    let timestamp = now_timestamp();
    let conversation = StoredConversation {
        schema_version: CONVERSATION_SCHEMA_VERSION,
        id: crate::storage::new_id("ai-conversation"),
        title: "新建 AI 对话".to_string(),
        provider_id,
        created_at: timestamp.clone(),
        updated_at: timestamp,
        messages: Vec::new(),
    };
    persist_conversation(app, &mut index, &conversation)?;
    Ok(public_conversation(conversation))
}

pub fn delete_conversation(app: &AppHandle, conversation_id: &str) -> Result<(), AppError> {
    let conversation_id = validate_conversation_id(conversation_id)?;
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    let before = index.conversations.len();
    index
        .conversations
        .retain(|conversation| conversation.id != conversation_id);
    if before == index.conversations.len() {
        return Err(ai_error(
            "AI_CONVERSATION_NOT_FOUND",
            "找不到指定的 AI 对话",
        ));
    }
    index.schema_version = CONVERSATION_SCHEMA_VERSION;
    write_conversation_index(app, &index)?;
    let path = conversation_file_path(app, &conversation_id)?;
    if path.exists() {
        fs::remove_file(path).map_err(|error| AppError::Storage(error.to_string()))?;
    }
    Ok(())
}

#[derive(Clone)]
struct PreparedChatRequest {
    request: AiChatRequest,
    provider: StoredAiProvider,
    api_key: Option<String>,
    conversation: StoredConversation,
}

fn prepare_start_chat(
    app: &AppHandle,
    input: StartAiChatInput,
) -> Result<PreparedChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let provider_id = normalize_provider_id(&input.provider_id)?;
    let user_message = normalize_user_message(&input.user_message)?;
    let (provider, api_key) = resolve_chat_provider(app, &provider_id)?;

    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    let mut conversation = read_stored_conversation(app, &conversation_id)?;
    let timestamp = now_timestamp();
    let user_message_id = crate::storage::new_id("ai-message");
    conversation.provider_id = provider.id.clone();
    conversation.updated_at = timestamp.clone();
    if conversation.messages.is_empty() {
        conversation.title = title_from_user_message(&user_message);
    }
    conversation.messages.push(AiMessage {
        id: user_message_id.clone(),
        role: AiMessageRole::User,
        content: user_message,
        created_at: timestamp,
    });
    persist_conversation(app, &mut index, &conversation)?;

    Ok(PreparedChatRequest {
        request: AiChatRequest {
            request_id: crate::storage::new_id("ai-request"),
            conversation_id,
            user_message_id,
            assistant_message_id: crate::storage::new_id("ai-message"),
        },
        provider,
        api_key,
        conversation,
    })
}

fn prepare_retry_chat(
    app: &AppHandle,
    input: RetryAiChatInput,
) -> Result<PreparedChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let provider_id = normalize_provider_id(&input.provider_id)?;
    let (provider, api_key) = resolve_chat_provider(app, &provider_id)?;

    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    let mut conversation = read_stored_conversation(app, &conversation_id)?;
    let user_message_id = conversation
        .messages
        .last()
        .filter(|message| message.role == AiMessageRole::User)
        .map(|message| message.id.clone())
        .ok_or_else(|| {
            ai_error(
                "AI_CONVERSATION_INVALID_INPUT",
                "只有未完成的最新提问可以重试",
            )
        })?;
    if conversation.provider_id != provider.id {
        conversation.provider_id = provider.id.clone();
        conversation.updated_at = now_timestamp();
        persist_conversation(app, &mut index, &conversation)?;
    }

    Ok(PreparedChatRequest {
        request: AiChatRequest {
            request_id: crate::storage::new_id("ai-request"),
            conversation_id,
            user_message_id,
            assistant_message_id: crate::storage::new_id("ai-message"),
        },
        provider,
        api_key,
        conversation,
    })
}

fn append_assistant_message(
    app: &AppHandle,
    request: &AiChatRequest,
    content: String,
) -> Result<AiConversation, AppError> {
    if content.trim().is_empty() {
        return Err(ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回可显示的回答",
        ));
    }
    if content.chars().count() > MAX_ASSISTANT_MESSAGE_LENGTH {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "AI 回答超过本地对话长度限制",
        ));
    }
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &request.conversation_id)?;
    let mut conversation = read_stored_conversation(app, &request.conversation_id)?;
    if conversation
        .messages
        .last()
        .map(|message| message.id.as_str())
        != Some(request.user_message_id.as_str())
    {
        return Err(ai_error(
            "AI_CONVERSATION_INVALID_INPUT",
            "AI 对话已变化，请重新发送提问",
        ));
    }
    conversation.updated_at = now_timestamp();
    conversation.messages.push(AiMessage {
        id: request.assistant_message_id.clone(),
        role: AiMessageRole::Assistant,
        content,
        created_at: conversation.updated_at.clone(),
    });
    persist_conversation(app, &mut index, &conversation)?;
    Ok(public_conversation(conversation))
}

fn chat_registry_lock(
) -> Result<std::sync::MutexGuard<'static, ActiveChatRequestRegistry>, AppError> {
    ACTIVE_CHAT_REQUESTS
        .lock()
        .map_err(|_| AppError::Storage("AI 对话请求锁不可用".to_string()))
}

fn register_chat_request(
    request_id: &str,
    conversation_id: &str,
    cancellation: CancellationToken,
) -> Result<(), AppError> {
    let mut registry = chat_registry_lock()?;
    if registry.by_conversation.contains_key(conversation_id) {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "当前对话正在生成，请等待完成或先停止",
        ));
    }
    registry
        .by_request
        .insert(request_id.to_string(), cancellation);
    registry
        .by_conversation
        .insert(conversation_id.to_string(), request_id.to_string());
    Ok(())
}

fn unregister_chat_request(request_id: &str, conversation_id: &str) {
    let Ok(mut registry) = ACTIVE_CHAT_REQUESTS.lock() else {
        return;
    };
    registry.by_request.remove(request_id);
    if registry
        .by_conversation
        .get(conversation_id)
        .is_some_and(|active_request_id| active_request_id == request_id)
    {
        registry.by_conversation.remove(conversation_id);
    }
}

pub fn cancel_chat(request_id: &str) -> Result<(), AppError> {
    let request_id = request_id.trim();
    if request_id.is_empty() {
        return Err(ai_error("AI_REQUEST_CANCELLED", "AI 请求 ID 无效"));
    }
    let cancellation = chat_registry_lock()?.by_request.get(request_id).cloned();
    if let Some(cancellation) = cancellation {
        cancellation.cancel();
    }
    Ok(())
}

fn emit_stream_event(
    channel: &Channel<AiStreamEvent>,
    event: AiStreamEvent,
) -> Result<(), AppError> {
    channel
        .send(event)
        .map_err(|_| ai_error("AI_REQUEST_CANCELLED", "AI 对话窗口已关闭，已停止生成"))
}

fn stream_error_event(error: AppError) -> AiStreamEvent {
    let raw = error.to_string();
    let known_code = raw
        .split_once(": ")
        .map(|(code, message)| (code, message.to_string()))
        .filter(|(code, _)| code.starts_with("AI_"));
    let (code, message) = known_code.unwrap_or((
        "AI_PROVIDER_CONNECTION_FAILED",
        "AI Provider 请求失败，请检查连接后重试。".to_string(),
    ));
    let retryable = matches!(
        code,
        "AI_PROVIDER_CONNECTION_FAILED" | "AI_PROVIDER_HTTP_ERROR" | "AI_PROVIDER_TIMEOUT"
    );
    AiStreamEvent::Error {
        code: code.to_string(),
        message,
        retryable,
    }
}

fn launch_chat_request(
    app: AppHandle,
    prepared: PreparedChatRequest,
    channel: Channel<AiStreamEvent>,
    cancellation: CancellationToken,
) {
    tauri::async_runtime::spawn(async move {
        let request_id = prepared.request.request_id.clone();
        let conversation_id = prepared.request.conversation_id.clone();
        let result = run_chat_request(&app, &prepared, &channel, &cancellation).await;
        if let Err(error) = result {
            let _ = emit_stream_event(&channel, stream_error_event(error));
        }
        unregister_chat_request(&request_id, &conversation_id);
    });
}

pub fn start_chat(
    app: &AppHandle,
    input: StartAiChatInput,
    channel: Channel<AiStreamEvent>,
) -> Result<AiChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let request_id = crate::storage::new_id("ai-request");
    let cancellation = CancellationToken::new();
    register_chat_request(&request_id, &conversation_id, cancellation.clone())?;
    let prepared = match prepare_start_chat(app, input) {
        Ok(mut prepared) => {
            prepared.request.request_id = request_id.clone();
            prepared
        }
        Err(error) => {
            unregister_chat_request(&request_id, &conversation_id);
            return Err(error);
        }
    };
    let request = prepared.request.clone();
    launch_chat_request(app.clone(), prepared, channel, cancellation);
    Ok(request)
}

pub fn retry_chat(
    app: &AppHandle,
    input: RetryAiChatInput,
    channel: Channel<AiStreamEvent>,
) -> Result<AiChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let request_id = crate::storage::new_id("ai-request");
    let cancellation = CancellationToken::new();
    register_chat_request(&request_id, &conversation_id, cancellation.clone())?;
    let prepared = match prepare_retry_chat(app, input) {
        Ok(mut prepared) => {
            prepared.request.request_id = request_id.clone();
            prepared
        }
        Err(error) => {
            unregister_chat_request(&request_id, &conversation_id);
            return Err(error);
        }
    };
    let request = prepared.request.clone();
    launch_chat_request(app.clone(), prepared, channel, cancellation);
    Ok(request)
}

#[derive(Default)]
struct ChatStreamResult {
    content: String,
    finish_reason: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

fn request_cancelled_error() -> AppError {
    ai_error("AI_REQUEST_CANCELLED", "已停止 AI 回复")
}

async fn run_chat_request(
    app: &AppHandle,
    prepared: &PreparedChatRequest,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<(), AppError> {
    emit_stream_event(
        channel,
        AiStreamEvent::Started {
            request_id: prepared.request.request_id.clone(),
            message_id: prepared.request.assistant_message_id.clone(),
        },
    )?;

    let semaphore = CHAT_REQUEST_SEMAPHORE.clone();
    let _permit = tokio::select! {
        _ = cancellation.cancelled() => return Err(request_cancelled_error()),
        permit = semaphore.acquire_owned() => permit.map_err(|_| {
            ai_error("AI_CONVERSATION_LIMIT", "AI 对话队列当前不可用，请稍后重试")
        })?,
    };

    let stream = match prepared.provider.kind {
        AiProviderKind::OpenaiCompatibleChat => {
            stream_openai_compatible_chat(
                &prepared.provider,
                prepared.api_key.as_deref(),
                &prepared.conversation,
                channel,
                cancellation,
            )
            .await?
        }
        AiProviderKind::OpenaiResponses => {
            stream_openai_responses(
                &prepared.provider,
                prepared.api_key.as_deref(),
                &prepared.conversation,
                channel,
                cancellation,
            )
            .await?
        }
        AiProviderKind::AnthropicMessages => {
            stream_anthropic_messages(
                &prepared.provider,
                prepared.api_key.as_deref(),
                &prepared.conversation,
                channel,
                cancellation,
            )
            .await?
        }
    };
    if cancellation.is_cancelled() {
        return Err(request_cancelled_error());
    }
    let conversation = append_assistant_message(app, &prepared.request, stream.content)?;
    if stream.input_tokens.is_some() || stream.output_tokens.is_some() {
        emit_stream_event(
            channel,
            AiStreamEvent::Usage {
                input_tokens: stream.input_tokens,
                output_tokens: stream.output_tokens,
            },
        )?;
    }
    emit_stream_event(
        channel,
        AiStreamEvent::Completed {
            conversation,
            finish_reason: stream.finish_reason,
        },
    )
}

fn selected_history_messages(conversation: &StoredConversation) -> Vec<&AiMessage> {
    let mut selected = Vec::new();
    let mut used_characters = 0usize;
    for message in conversation.messages.iter().rev() {
        let message_characters = message.content.chars().count();
        // The most recent message is the turn currently being sent. Preserve
        // it even when older local history has exhausted the budget.
        if !selected.is_empty()
            && used_characters.saturating_add(message_characters) > MAX_HISTORY_CHARACTERS
        {
            continue;
        }
        used_characters = used_characters.saturating_add(message_characters);
        selected.push(message);
    }
    selected.reverse();
    selected
}

fn provider_history_messages(conversation: &StoredConversation) -> Vec<Value> {
    let selected = selected_history_messages(conversation);
    let mut messages = Vec::with_capacity(selected.len() + 1);
    messages.push(json!({ "role": "system", "content": L0_SYSTEM_PROMPT }));
    messages.extend(selected.into_iter().map(|message| {
        let role = match message.role {
            AiMessageRole::User => "user",
            AiMessageRole::Assistant => "assistant",
        };
        json!({ "role": role, "content": message.content })
    }));
    messages
}

fn responses_input_items(conversation: &StoredConversation) -> Vec<Value> {
    selected_history_messages(conversation)
        .into_iter()
        .map(|message| {
            let role = match message.role {
                AiMessageRole::User => "user",
                AiMessageRole::Assistant => "assistant",
            };
            json!({ "role": role, "content": message.content })
        })
        .collect()
}

fn anthropic_history_messages(conversation: &StoredConversation) -> Vec<Value> {
    selected_history_messages(conversation)
        .into_iter()
        .map(|message| {
            let role = match message.role {
                AiMessageRole::User => "user",
                AiMessageRole::Assistant => "assistant",
            };
            json!({ "role": role, "content": message.content })
        })
        .collect()
}

fn text_from_content(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let parts = value.as_array()?;
    let text = parts
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(Value::as_str)
                .or_else(|| part.get("content").and_then(Value::as_str))
        })
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn apply_openai_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let Some(usage) = payload.get("usage") else {
        return;
    };
    stream.input_tokens = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn apply_responses_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let usage = payload
        .get("response")
        .and_then(|response| response.get("usage"))
        .or_else(|| payload.get("usage"));
    let Some(usage) = usage else {
        return;
    };
    stream.input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn apply_anthropic_usage(payload: &Value, stream: &mut ChatStreamResult) {
    let usage = payload
        .get("message")
        .and_then(|message| message.get("usage"))
        .or_else(|| payload.get("usage"));
    let Some(usage) = usage else {
        return;
    };
    stream.input_tokens = usage
        .get("input_tokens")
        .and_then(Value::as_u64)
        .or(stream.input_tokens);
    stream.output_tokens = usage
        .get("output_tokens")
        .and_then(Value::as_u64)
        .or(stream.output_tokens);
}

fn append_stream_text(
    stream: &mut ChatStreamResult,
    text: String,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if text.is_empty() {
        return Ok(());
    }
    if stream
        .content
        .chars()
        .count()
        .saturating_add(text.chars().count())
        > MAX_ASSISTANT_MESSAGE_LENGTH
    {
        return Err(ai_error(
            "AI_CONVERSATION_LIMIT",
            "AI 回答超过本地对话长度限制",
        ));
    }
    stream.content.push_str(&text);
    emit_stream_event(channel, AiStreamEvent::TextDelta { text })
}

fn process_openai_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("error").is_some_and(|error| !error.is_null()) {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "AI Provider 返回了对话错误",
        ));
    }
    apply_openai_usage(&payload, stream);
    let Some(choice) = payload
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
    else {
        return Ok(());
    };
    if let Some(finish_reason) = choice.get("finish_reason").and_then(Value::as_str) {
        stream.finish_reason = Some(finish_reason.to_string());
    }
    let text = choice
        .get("delta")
        .and_then(|delta| delta.get("content"))
        .and_then(text_from_content)
        .or_else(|| {
            choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(text_from_content)
        });
    if let Some(text) = text {
        append_stream_text(stream, text, channel)?;
    }
    Ok(())
}

fn response_output_text(response: &Value) -> Option<String> {
    let text = response
        .get("output")
        .and_then(Value::as_array)?
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("output_text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn anthropic_content_text(message: &Value) -> Option<String> {
    let text = message
        .get("content")
        .and_then(Value::as_array)?
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn process_openai_responses_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("type").and_then(Value::as_str) == Some("error")
        || payload.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "OpenAI Responses Provider 返回了对话错误",
        ));
    }
    apply_responses_usage(&payload, stream);
    let event_type = payload.get("type").and_then(Value::as_str);
    match event_type {
        Some("response.output_text.delta") => {
            if let Some(text) = payload.get("delta").and_then(Value::as_str) {
                append_stream_text(stream, text.to_string(), channel)?;
            }
        }
        Some("response.completed") => {
            let response = payload.get("response").unwrap_or(&payload);
            apply_responses_usage(response, stream);
            stream.finish_reason = response
                .get("status")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| stream.finish_reason.clone());
            if stream.content.is_empty() {
                if let Some(text) = response_output_text(response) {
                    append_stream_text(stream, text, channel)?;
                }
            }
        }
        Some("response.failed") | Some("response.incomplete") => {
            return Err(ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "OpenAI Responses Provider 未完成回答",
            ));
        }
        _ => {
            // Responses adds typed events over time. This L0 chat only needs
            // user-visible output text and must ignore unrelated events.
            if event_type.is_none() && stream.content.is_empty() {
                if let Some(text) = response_output_text(&payload) {
                    append_stream_text(stream, text, channel)?;
                }
                if let Some(status) = payload.get("status").and_then(Value::as_str) {
                    stream.finish_reason = Some(status.to_string());
                }
            }
        }
    }
    Ok(())
}

fn process_anthropic_payload(
    payload: Value,
    stream: &mut ChatStreamResult,
    channel: &Channel<AiStreamEvent>,
) -> Result<(), AppError> {
    if payload.get("type").and_then(Value::as_str) == Some("error")
        || payload.get("error").is_some_and(|error| !error.is_null())
    {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            "Anthropic Messages Provider 返回了对话错误",
        ));
    }
    apply_anthropic_usage(&payload, stream);
    match payload.get("type").and_then(Value::as_str) {
        Some("content_block_delta") => {
            let delta = payload.get("delta");
            if delta
                .and_then(|value| value.get("type"))
                .and_then(Value::as_str)
                == Some("text_delta")
            {
                if let Some(text) = delta
                    .and_then(|value| value.get("text"))
                    .and_then(Value::as_str)
                {
                    append_stream_text(stream, text.to_string(), channel)?;
                }
            }
        }
        Some("message_delta") => {
            if let Some(stop_reason) = payload
                .get("delta")
                .and_then(|delta| delta.get("stop_reason"))
                .and_then(Value::as_str)
            {
                stream.finish_reason = Some(stop_reason.to_string());
            }
        }
        Some("message_stop") => {
            if stream.finish_reason.is_none() {
                stream.finish_reason = Some("message_stop".to_string());
            }
        }
        _ => {
            // A few compatible gateways return the final non-streaming
            // Messages object even when streaming was requested.
            if stream.content.is_empty() {
                if let Some(text) = anthropic_content_text(&payload) {
                    append_stream_text(stream, text, channel)?;
                }
                if let Some(stop_reason) = payload.get("stop_reason").and_then(Value::as_str) {
                    stream.finish_reason = Some(stop_reason.to_string());
                }
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct SseDecoder {
    line_buffer: Vec<u8>,
    data_lines: Vec<String>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, AppError> {
        self.line_buffer.extend_from_slice(chunk);
        if self.line_buffer.len() > MAX_SSE_LINE_BYTES {
            return Err(ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "AI Provider 返回了过长的流式事件",
            ));
        }
        let mut events = Vec::new();
        while let Some(position) = self.line_buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.line_buffer.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.consume_line(line)? {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<String>, AppError> {
        let mut events = Vec::new();
        if !self.line_buffer.is_empty() {
            let mut line = std::mem::take(&mut self.line_buffer);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if let Some(event) = self.consume_line(line)? {
                events.push(event);
            }
        }
        if !self.data_lines.is_empty() {
            events.push(std::mem::take(&mut self.data_lines).join("\n"));
        }
        Ok(events)
    }

    fn consume_line(&mut self, line: Vec<u8>) -> Result<Option<String>, AppError> {
        if line.is_empty() {
            return Ok((!self.data_lines.is_empty())
                .then(|| std::mem::take(&mut self.data_lines).join("\n")));
        }
        if line.first() == Some(&b':') {
            return Ok(None);
        }
        let Some(separator) = line.iter().position(|byte| *byte == b':') else {
            return Ok(None);
        };
        if &line[..separator] != b"data" {
            return Ok(None);
        }
        let mut raw = &line[separator + 1..];
        if raw.first() == Some(&b' ') {
            raw = &raw[1..];
        }
        let data = String::from_utf8(raw.to_vec()).map_err(|_| {
            ai_error(
                "AI_PROVIDER_RESPONSE_INVALID",
                "AI Provider 返回了无效的流式 UTF-8 数据",
            )
        })?;
        self.data_lines.push(data);
        Ok(None)
    }
}

type StreamPayloadProcessor =
    fn(Value, &mut ChatStreamResult, &Channel<AiStreamEvent>) -> Result<(), AppError>;

fn chat_request_error(error: reqwest::Error, stage: &str) -> AppError {
    if error.is_timeout() {
        ai_error("AI_PROVIDER_TIMEOUT", format!("AI Provider {stage}超时"))
    } else {
        ai_error(
            "AI_PROVIDER_CONNECTION_FAILED",
            format!("AI Provider {stage}失败，请检查网络和 API 地址"),
        )
    }
}

async fn send_streaming_request(
    request: reqwest::RequestBuilder,
    cancellation: &CancellationToken,
) -> Result<reqwest::Response, AppError> {
    let response = tokio::select! {
        _ = cancellation.cancelled() => return Err(request_cancelled_error()),
        response = request.send() => response.map_err(|error| chat_request_error(error, "对话请求"))?,
    };
    if !response.status().is_success() {
        return Err(ai_error(
            "AI_PROVIDER_HTTP_ERROR",
            format!("AI Provider 返回 HTTP {}", response.status()),
        ));
    }
    Ok(response)
}

fn parse_stream_payload(event: &str) -> Result<Value, AppError> {
    serde_json::from_str::<Value>(event).map_err(|_| {
        ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 返回了无效的流式 JSON 数据",
        )
    })
}

async fn consume_streaming_response(
    mut response: reqwest::Response,
    cancellation: &CancellationToken,
    channel: &Channel<AiStreamEvent>,
    process_payload: StreamPayloadProcessor,
) -> Result<ChatStreamResult, AppError> {
    let is_json_response = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("application/json"));
    let mut stream = ChatStreamResult::default();
    if is_json_response {
        let payload = tokio::select! {
            _ = cancellation.cancelled() => return Err(request_cancelled_error()),
            payload = response.json::<Value>() => payload.map_err(|_| {
                ai_error("AI_PROVIDER_RESPONSE_INVALID", "AI Provider 未返回有效 JSON 对象")
            })?,
        };
        process_payload(payload, &mut stream, channel)?;
        return Ok(stream);
    }

    let mut decoder = SseDecoder::default();
    loop {
        let chunk = tokio::select! {
            _ = cancellation.cancelled() => return Err(request_cancelled_error()),
            chunk = response.chunk() => chunk.map_err(|error| chat_request_error(error, "流式响应"))?,
        };
        let Some(chunk) = chunk else {
            break;
        };
        for event in decoder.push(&chunk)? {
            if event.trim() == "[DONE]" {
                return Ok(stream);
            }
            process_payload(parse_stream_payload(&event)?, &mut stream, channel)?;
        }
    }
    for event in decoder.finish()? {
        if event.trim() == "[DONE]" {
            break;
        }
        process_payload(parse_stream_payload(&event)?, &mut stream, channel)?;
    }
    Ok(stream)
}

async fn stream_openai_compatible_chat(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = chat_completions_url(provider)?;
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&json!({
            "model": provider.model,
            "messages": provider_history_messages(conversation),
            "stream": true
        }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = send_streaming_request(request, cancellation).await?;
    consume_streaming_response(response, cancellation, channel, process_openai_payload).await
}

async fn stream_openai_responses(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = responses_url(provider)?;
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&json!({
            "model": provider.model,
            "instructions": L0_SYSTEM_PROMPT,
            "input": responses_input_items(conversation),
            "stream": true,
            // FileTerm persists the minimal conversation locally; do not ask
            // the remote provider to retain a second server-side history.
            "store": false
        }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = send_streaming_request(request, cancellation).await?;
    consume_streaming_response(
        response,
        cancellation,
        channel,
        process_openai_responses_payload,
    )
    .await
}

async fn stream_anthropic_messages(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = anthropic_messages_url(provider)?;
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .json(&json!({
            "model": provider.model,
            "system": L0_SYSTEM_PROMPT,
            "messages": anthropic_history_messages(conversation),
            "max_tokens": ANTHROPIC_DEFAULT_MAX_TOKENS,
            "stream": true
        }));
    if let Some(api_key) = api_key {
        request = request.header("x-api-key", api_key);
    }
    let response = send_streaming_request(request, cancellation).await?;
    consume_streaming_response(response, cancellation, channel, process_anthropic_payload).await
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

    let client = client(&provider)?;
    match &provider.kind {
        AiProviderKind::OpenaiCompatibleChat => {
            test_openai_compatible_chat(&client, &provider, api_key.as_deref()).await
        }
        AiProviderKind::OpenaiResponses => {
            test_openai_responses(&client, &provider, api_key.as_deref()).await
        }
        AiProviderKind::AnthropicMessages => {
            test_anthropic_messages(&client, &provider, api_key.as_deref()).await
        }
    }
}

fn client(provider: &StoredAiProvider) -> Result<Client, AppError> {
    build_client(provider, REQUEST_TIMEOUT)
}

fn chat_client(provider: &StoredAiProvider) -> Result<Client, AppError> {
    build_client(provider, CHAT_REQUEST_TIMEOUT)
}

fn build_client(
    provider: &StoredAiProvider,
    request_timeout: Duration,
) -> Result<Client, AppError> {
    let mut builder = Client::builder()
        .connect_timeout(CONNECTION_TIMEOUT)
        .timeout(request_timeout)
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

fn responses_url(provider: &StoredAiProvider) -> Result<Url, AppError> {
    Url::parse(&format!("{}/responses", provider.base_url))
        .map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "无法构造 Provider 请求地址"))
}

fn anthropic_messages_url(provider: &StoredAiProvider) -> Result<Url, AppError> {
    Url::parse(&format!("{}/messages", provider.base_url))
        .map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "无法构造 Provider 请求地址"))
}

async fn validate_provider_test_response(
    response: reqwest::Response,
) -> Result<AiProviderTestResult, AppError> {
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

    let response = request
        .send()
        .await
        .map_err(|error| chat_request_error(error, "连接测试"))?;
    validate_provider_test_response(response).await
}

async fn test_openai_responses(
    client: &Client,
    provider: &StoredAiProvider,
    api_key: Option<&str>,
) -> Result<AiProviderTestResult, AppError> {
    let request_url = responses_url(provider)?;
    let mut request = client.post(request_url).json(&json!({
        "model": provider.model,
        "instructions": "You are a connection test. Reply with exactly OK.",
        "input": "Reply with exactly OK.",
        "stream": false,
        "store": false
    }));
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| chat_request_error(error, "连接测试"))?;
    validate_provider_test_response(response).await
}

async fn test_anthropic_messages(
    client: &Client,
    provider: &StoredAiProvider,
    api_key: Option<&str>,
) -> Result<AiProviderTestResult, AppError> {
    let request_url = anthropic_messages_url(provider)?;
    let mut request = client
        .post(request_url)
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .json(&json!({
            "model": provider.model,
            "system": "You are a connection test. Reply with exactly OK.",
            "messages": [{
                "role": "user",
                "content": "Reply with exactly OK."
            }],
            "max_tokens": 8,
            "stream": false
        }));
    if let Some(api_key) = api_key {
        request = request.header("x-api-key", api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| chat_request_error(error, "连接测试"))?;
    validate_provider_test_response(response).await
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_conversation_fits, normalize_base_url, provider_history_messages,
        provider_is_usable, provider_summary, repair_default_provider, stream_anthropic_messages,
        stream_openai_responses, test_openai_compatible_chat, title_from_user_message,
        write_json_file, AiMessage, AiMessageRole, AiProviderKind, AiProviderSummary,
        AiStreamEvent, SseDecoder, StoredAiProvider, StoredConversation, StoredProviderConfig,
        StoredProviderSecret, StoredProviderSecrets, ANTHROPIC_API_VERSION,
        ANTHROPIC_DEFAULT_MAX_TOKENS, CONVERSATION_SCHEMA_VERSION,
    };
    use reqwest::Client;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tauri::ipc::Channel;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_util::sync::CancellationToken;

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

    fn conversation(messages: Vec<AiMessage>) -> StoredConversation {
        StoredConversation {
            schema_version: CONVERSATION_SCHEMA_VERSION,
            id: "ai-conversation-1".to_string(),
            title: "Conversation".to_string(),
            provider_id: "provider-1".to_string(),
            created_at: "1".to_string(),
            updated_at: "2".to_string(),
            messages,
        }
    }

    fn test_client() -> Client {
        Client::builder()
            .no_proxy()
            .build()
            .expect("test client must build")
    }

    fn stream_channel(events: Arc<Mutex<Vec<Value>>>) -> Channel<AiStreamEvent> {
        Channel::new(move |body| {
            let payload: Value = body.deserialize().expect("stream event should deserialize");
            events
                .lock()
                .expect("events lock should be available")
                .push(payload);
            Ok(())
        })
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

    #[test]
    fn l0_provider_payload_contains_only_system_policy_and_local_messages() {
        let conversation = conversation(vec![
            AiMessage {
                id: "message-user".to_string(),
                role: AiMessageRole::User,
                content: "Explain this command".to_string(),
                created_at: "1".to_string(),
            },
            AiMessage {
                id: "message-assistant".to_string(),
                role: AiMessageRole::Assistant,
                content: "It lists files.".to_string(),
                created_at: "2".to_string(),
            },
        ]);

        let messages = provider_history_messages(&conversation);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(
            messages[1],
            json!({ "role": "user", "content": "Explain this command" })
        );
        assert_eq!(
            messages[2],
            json!({ "role": "assistant", "content": "It lists files." })
        );
        let payload = json!({ "messages": messages });
        assert!(payload["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|message| {
                message.get("tabId").is_none()
                    && message.get("host").is_none()
                    && message.get("cwd").is_none()
                    && message.get("transcript").is_none()
            }));
    }

    #[test]
    fn sse_decoder_accepts_crlf_and_split_chunks_without_losing_data() {
        let mut decoder = SseDecoder::default();
        assert!(decoder
            .push(b"event: message\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"hel")
            .unwrap()
            .is_empty());
        assert_eq!(
            decoder
                .push(b"lo\"}}]}\r\n\r\n")
                .expect("second chunk should parse"),
            vec![r#"{"choices":[{"delta":{"content":"hello"}}]}"#.to_string()]
        );
    }

    #[test]
    fn sse_decoder_flushes_a_final_event_without_a_trailing_blank_line() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"data: [DONE]").unwrap().is_empty());
        assert_eq!(decoder.finish().unwrap(), vec!["[DONE]".to_string()]);
    }

    #[test]
    fn stream_events_keep_the_core_discriminated_union_shape() {
        let payload = serde_json::to_value(AiStreamEvent::Started {
            request_id: "request-1".to_string(),
            message_id: "message-1".to_string(),
        })
        .expect("event should serialize");
        assert_eq!(
            payload,
            json!({ "type": "started", "requestId": "request-1", "messageId": "message-1" })
        );
    }

    #[test]
    fn conversation_title_and_storage_limit_keep_local_history_bounded() {
        assert_eq!(
            title_from_user_message("  inspect   nginx logs  "),
            "inspect nginx logs"
        );
        let conversation = conversation(vec![AiMessage {
            id: "message-1".to_string(),
            role: AiMessageRole::User,
            content: "hello".to_string(),
            created_at: "1".to_string(),
        }]);
        assert!(ensure_conversation_fits(&conversation).is_ok());
    }

    #[tokio::test]
    async fn responses_adapter_streams_typed_text_and_keeps_history_local() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture should bind");
        let address = listener.local_addr().expect("fixture should have address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("fixture should accept");
            let request = read_request(&mut socket).await;
            assert!(request.starts_with("POST /v1/responses HTTP/1.1"));
            assert!(request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("authorization: Bearer test-key")));
            let body = request
                .split("\r\n\r\n")
                .nth(1)
                .expect("request should include body");
            let body: Value = serde_json::from_str(body).expect("body should be json");
            assert_eq!(body["model"], "test-model");
            assert_eq!(body["stream"], true);
            assert_eq!(body["store"], false);
            assert!(body["instructions"]
                .as_str()
                .is_some_and(|instructions| instructions.contains("no terminal")));
            assert_eq!(body["input"][0]["role"], "user");
            assert_eq!(body["input"][0]["content"], "Explain this command");
            assert!(body.to_string().contains("Explain this command"));
            assert!(!body.to_string().contains("transcript"));

            let response_body = concat!(
                "event: response.created\n",
                "data: {\"type\":\"response.created\"}\n\n",
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n\n",
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\" there\"}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":7,\"output_tokens\":2},\"output\":[]}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("fixture should respond");
        });

        let mut provider = provider(&format!("http://{address}/v1"));
        provider.kind = AiProviderKind::OpenaiResponses;
        provider.allow_insecure_http = true;
        let conversation = conversation(vec![AiMessage {
            id: "message-user".to_string(),
            role: AiMessageRole::User,
            content: "Explain this command".to_string(),
            created_at: "1".to_string(),
        }]);
        let events = Arc::new(Mutex::new(Vec::new()));
        let result = stream_openai_responses(
            &provider,
            Some("test-key"),
            &conversation,
            &stream_channel(Arc::clone(&events)),
            &CancellationToken::new(),
        )
        .await
        .expect("Responses stream should succeed");

        assert_eq!(result.content, "Hello there");
        assert_eq!(result.finish_reason.as_deref(), Some("completed"));
        assert_eq!(result.input_tokens, Some(7));
        assert_eq!(result.output_tokens, Some(2));
        assert_eq!(
            *events.lock().expect("events lock should be available"),
            vec![
                json!({ "type": "text-delta", "text": "Hello" }),
                json!({ "type": "text-delta", "text": " there" }),
            ]
        );
        server.await.expect("fixture should finish");
    }

    #[tokio::test]
    async fn anthropic_adapter_streams_text_deltas_and_uses_messages_headers() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture should bind");
        let address = listener.local_addr().expect("fixture should have address");
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("fixture should accept");
            let request = read_request(&mut socket).await;
            assert!(request.starts_with("POST /v1/messages HTTP/1.1"));
            assert!(request
                .lines()
                .any(|line| line.eq_ignore_ascii_case("x-api-key: test-key")));
            assert!(request.lines().any(|line| {
                line.eq_ignore_ascii_case(&format!("anthropic-version: {ANTHROPIC_API_VERSION}"))
            }));
            let body = request
                .split("\r\n\r\n")
                .nth(1)
                .expect("request should include body");
            let body: Value = serde_json::from_str(body).expect("body should be json");
            assert_eq!(body["model"], "test-model");
            assert_eq!(body["stream"], true);
            assert_eq!(body["max_tokens"], ANTHROPIC_DEFAULT_MAX_TOKENS);
            assert!(body["system"]
                .as_str()
                .is_some_and(|instructions| instructions.contains("no terminal")));
            assert_eq!(body["messages"][0]["role"], "user");
            assert_eq!(body["messages"][0]["content"], "Check the service");
            assert!(!body.to_string().contains("transcript"));

            let response_body = concat!(
                "event: message_start\n",
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9,\"output_tokens\":1}}}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Check\"}}\n\n",
                "event: ping\n",
                "data: {\"type\":\"ping\"}\n\n",
                "event: content_block_delta\n",
                "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\" logs\"}}\n\n",
                "event: message_delta\n",
                "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":3}}\n\n",
                "event: message_stop\n",
                "data: {\"type\":\"message_stop\"}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("fixture should respond");
        });

        let mut provider = provider(&format!("http://{address}/v1"));
        provider.kind = AiProviderKind::AnthropicMessages;
        provider.allow_insecure_http = true;
        let conversation = conversation(vec![AiMessage {
            id: "message-user".to_string(),
            role: AiMessageRole::User,
            content: "Check the service".to_string(),
            created_at: "1".to_string(),
        }]);
        let events = Arc::new(Mutex::new(Vec::new()));
        let result = stream_anthropic_messages(
            &provider,
            Some("test-key"),
            &conversation,
            &stream_channel(Arc::clone(&events)),
            &CancellationToken::new(),
        )
        .await
        .expect("Anthropic stream should succeed");

        assert_eq!(result.content, "Check logs");
        assert_eq!(result.finish_reason.as_deref(), Some("end_turn"));
        assert_eq!(result.input_tokens, Some(9));
        assert_eq!(result.output_tokens, Some(3));
        assert_eq!(
            *events.lock().expect("events lock should be available"),
            vec![
                json!({ "type": "text-delta", "text": "Check" }),
                json!({ "type": "text-delta", "text": " logs" }),
            ]
        );
        server.await.expect("fixture should finish");
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
