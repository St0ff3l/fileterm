// Public AI contracts and private persisted model types.
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
    #[serde(default)]
    pub models: Vec<String>,
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
    #[serde(default)]
    pub models: Vec<String>,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiCopilotMode {
    #[default]
    PureConversation,
    SemiAutomatic,
    FullyAutomatic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AiContextMode {
    #[serde(rename = "L0", alias = "l0")]
    Level0,
    /// `metadata` and `recent-terminal` are accepted as legacy aliases. Both
    /// now normalize to L2 so the removed L1 level cannot reappear.
    #[serde(
        rename = "L2",
        alias = "l2",
        alias = "metadata",
        alias = "recent-terminal"
    )]
    Level2,
}

impl AiCopilotMode {
    fn requires_l2(self) -> bool {
        !matches!(self, Self::PureConversation)
    }

    fn uses_tools(self) -> bool {
        !matches!(self, Self::PureConversation)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAutoModeGuardrailState {
    pub dangerous_command_restrictions_enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCopilotModeState {
    pub mode: AiCopilotMode,
    pub attach_terminal_context: bool,
    pub auto_mode_guardrails: AiAutoModeGuardrailState,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAiCopilotModeInput {
    pub mode: AiCopilotMode,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAiContextAttachInput {
    pub attach_terminal_context: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAiDangerousCommandRestrictionsInput {
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextTarget {
    pub tab_id: String,
    pub root_tab_id: String,
    pub session_type: String,
    pub session_revision: String,
    pub display_host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub connected: bool,
    /// Effective network-device mode bound to this one-time target. Older
    /// persisted targets omit it and remain compatible as normal servers.
    #[serde(default)]
    pub network_device: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiContextRedactionKind {
    Authorization,
    CredentialAssignment,
    PrivateKey,
    ControlSequence,
    LongLine,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextRedaction {
    pub kind: AiContextRedactionKind,
    pub count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextPreview {
    pub snapshot_id: String,
    pub expires_at: String,
    pub mode: AiContextMode,
    pub target: AiContextTarget,
    pub preview: String,
    pub redactions: Vec<AiContextRedaction>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextAttachment {
    pub mode: AiContextMode,
    pub target: AiContextTarget,
    pub redactions: Vec<AiContextRedaction>,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiCommandRisk {
    ReadOnly,
    Mutating,
    Destructive,
    Privileged,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum LegacyAiReviewOutcome {
    Completed,
    Rejected,
    ApprovalDismissed,
    ApprovalTimedOut,
    TargetChanged,
    CommandTimedOut,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LegacyAiReviewRecord {
    pub id: String,
    pub command_id: String,
    pub command: String,
    pub risk: AiCommandRisk,
    pub target: AiContextTarget,
    pub timeout_ms: u64,
    pub requested_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
    pub completed_at: String,
    pub outcome: LegacyAiReviewOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    pub timed_out: bool,
    pub output_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct LegacyAiCommandSuggestion {
    pub id: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    pub risk: AiCommandRisk,
    pub multiline: bool,
    pub target: AiContextTarget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum LegacyAiMessageRole {
    User,
    Assistant,
    Review,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyAiMessage {
    id: String,
    role: LegacyAiMessageRole,
    content: String,
    created_at: String,
    #[serde(default)]
    context: Option<AiContextAttachment>,
    #[serde(default)]
    tool_activities: Vec<AiToolActivity>,
    #[serde(default)]
    commands: Vec<LegacyAiCommandSuggestion>,
    #[serde(default)]
    review: Option<LegacyAiReviewRecord>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub id: String,
    pub role: AiMessageRole,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<AiContextAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_activities: Vec<AiToolActivity>,
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAiConversationInput {
    pub conversation_id: String,
    pub title: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAiMessageInput {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeAiConversationTitleInput {
    pub conversation_id: String,
    pub provider_id: String,
    #[serde(default)]
    pub model_override: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAiContextPreviewInput {
    pub tab_id: String,
    #[serde(default)]
    pub root_tab_id: Option<String>,
    pub provider_id: String,
    pub mode: AiContextMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum AiChatResponseMode {
    #[default]
    Chat,
}

/// Context can only travel through a Rust-owned, one-time snapshot. The
/// renderer never provides terminal, host, path, transcript, or command data
/// directly to the chat request.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAiChatInput {
    pub conversation_id: String,
    pub provider_id: String,
    #[serde(default)]
    pub model_override: Option<String>,
    pub user_message: String,
    #[serde(default)]
    pub context_snapshot_id: Option<String>,
    #[serde(default)]
    pub mode: AiCopilotMode,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryAiChatInput {
    pub conversation_id: String,
    pub provider_id: String,
    #[serde(default)]
    pub model_override: Option<String>,
    #[serde(default)]
    pub context_snapshot_id: Option<String>,
    #[serde(default)]
    pub mode: AiCopilotMode,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatRequest {
    pub request_id: String,
    pub conversation_id: String,
    pub user_message_id: String,
    pub assistant_message_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCallProposal {
    pub id: String,
    pub tool_name: String,
    pub command: String,
    pub risk: AiCommandRisk,
    pub target: AiContextTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolCallResult {
    pub proposal_id: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_truncated: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolActivity {
    pub proposal: AiToolCallProposal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<AiToolCallResult>,
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
    AssistantMessageStarted {
        #[serde(rename = "messageId")]
        message_id: String,
    },
    TextDelta {
        text: String,
    },
    ToolCall {
        proposal: AiToolCallProposal,
    },
    ToolResult {
        result: AiToolCallResult,
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
    #[serde(default)]
    models: Vec<String>,
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

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredConversationFile {
    #[serde(
        rename = "schemaVersion",
        default = "default_conversation_schema_version"
    )]
    _schema_version: u32,
    id: String,
    title: String,
    provider_id: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    messages: Vec<LegacyAiMessage>,
}

#[derive(Clone, Debug)]
struct StoredAiContextSnapshot {
    snapshot_id: String,
    expires_at_millis: u128,
    window_label: String,
    provider_id: String,
    mode: AiContextMode,
    target: AiContextTarget,
    preview: String,
    redactions: Vec<AiContextRedaction>,
    truncated: bool,
}

#[derive(Clone, Debug)]
struct StoredAiModeState {
    mode: AiCopilotMode,
    // Pure mode owns this preference. Semi/full mode expose an effective
    // `true` value without overwriting it, so returning to pure mode restores
    // the user's L0/L2 choice instead of leaking the forced L2 state.
    pure_context_preference: bool,
    session_generation: u64,
    dangerous_command_restrictions_enabled: bool,
}
