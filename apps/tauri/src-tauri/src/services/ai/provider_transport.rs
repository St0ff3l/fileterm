// Provider HTTP clients, streaming entry points, and provider checks.
const MAX_MODEL_LIST_ITEMS: usize = 1_000;

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn stream_openai_compatible_chat(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_openai_compatible_chat_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
        reasoning_effort,
        &[],
        false,
        channel,
        cancellation,
    )
    .await
}

// Provider adapters intentionally spell out the protocol boundary: the
// provider, secret, conversation projection, tool history, stream channel and
// cancellation token must remain independently auditable.
#[allow(clippy::too_many_arguments)]
async fn stream_openai_compatible_chat_with_tools(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    tool_turns: &[ToolLoopTurn],
    tools_enabled: bool,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = chat_completions_url(provider)?;
    let mut payload = json!({
        "model": provider.model,
        "messages": provider_history_messages_with_tools(
            conversation,
            context,
            response_mode,
            tool_turns,
            tools_enabled,
        ),
        "stream": true
    });
    if tools_enabled {
        payload["tools"] = json!([openai_chat_tool_schema()]);
        payload["tool_choice"] = json!("auto");
    }
    apply_openai_compatible_reasoning(&mut payload, provider, reasoning_effort);
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&payload);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = send_streaming_request(request, cancellation).await?;
    consume_streaming_response(response, cancellation, channel, process_openai_payload).await
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn stream_openai_responses(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_openai_responses_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
        reasoning_effort,
        &[],
        false,
        channel,
        cancellation,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn stream_openai_responses_with_tools(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    tool_turns: &[ToolLoopTurn],
    tools_enabled: bool,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = responses_url(provider)?;
    let tool_policy_enabled = tools_enabled || !tool_turns.is_empty();
    let mut payload = json!({
        "model": provider.model,
        "instructions": system_prompt_for_request(context, response_mode, tool_policy_enabled),
        "input": responses_input_items_with_tools(conversation, tool_turns),
        "stream": true,
        "store": false
    });
    if tools_enabled {
        payload["tools"] = json!([responses_tool_schema()]);
        payload["tool_choice"] = json!("auto");
    }
    apply_model_reasoning(
        &mut payload,
        provider,
        reasoning_effort,
        AiModelReasoningParameter::ReasoningObject,
    );
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&payload);
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

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
async fn stream_anthropic_messages(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_anthropic_messages_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
        reasoning_effort,
        &[],
        false,
        channel,
        cancellation,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn stream_anthropic_messages_with_tools(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    reasoning_effort: Option<AiReasoningEffort>,
    tool_turns: &[ToolLoopTurn],
    tools_enabled: bool,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    let client = chat_client(provider)?;
    let request_url = anthropic_messages_url(provider)?;
    let tool_policy_enabled = tools_enabled || !tool_turns.is_empty();
    let mut payload = json!({
        "model": provider.model,
        "system": system_prompt_for_request(context, response_mode, tool_policy_enabled),
        "messages": anthropic_history_messages_with_tools(conversation, tool_turns),
        "max_tokens": ANTHROPIC_DEFAULT_MAX_TOKENS,
        "stream": true
    });
    if tools_enabled {
        payload["tools"] = json!([anthropic_tool_schema()]);
        payload["tool_choice"] = json!({"type": "auto"});
    }
    apply_model_reasoning(
        &mut payload,
        provider,
        reasoning_effort,
        AiModelReasoningParameter::OutputConfigEffort,
    );
    let mut request = client
        .post(request_url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .header("anthropic-version", ANTHROPIC_API_VERSION)
        .json(&payload);
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

/// Fetch the live model directory without persisting it. The settings UI uses
/// this as a DBX-style catalog refresh; configured model IDs remain the source
/// of truth and manual IDs continue to work when an endpoint has no directory.
/// Local Ollama/LM Studio results are suggestions only and never replace the
/// user's explicitly configured model.
pub async fn list_models(
    app: &AppHandle,
    input: ListAiModelsInput,
) -> Result<Vec<AiModelInfo>, AppError> {
    let mut draft = input.provider;
    let secrets_patch = input.secrets;
    let (provider, api_key) = {
        let _guard = store_lock()?;
        let (config, secrets) = read_normalized_store(app)?;
        let existing_id = selected_existing_id(&config, draft.id.as_deref())?;
        let provider_id = existing_id
            .clone()
            .unwrap_or_else(|| "model-list".to_string());
        if draft.model.trim().is_empty() {
            draft.model = draft
                .models
                .iter()
                .find(|model| !model.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "__fileterm_model_list__".to_string());
        }
        let provider = normalize_provider(draft, provider_id)?;
        let api_key = resolve_test_api_key(
            &secrets,
            existing_id.as_deref(),
            secrets_patch.as_ref(),
        );
        validate_test_provider(&provider, api_key.as_deref())?;
        (provider, api_key)
    };

    let request_url = models_url(&provider)?;
    let client = client(&provider)?;
    let mut request = client
        .get(request_url)
        .header(reqwest::header::ACCEPT, "application/json");
    match &provider.kind {
        AiProviderKind::AnthropicMessages => {
            request = request.header("anthropic-version", ANTHROPIC_API_VERSION);
            if let Some(api_key) = api_key.as_deref() {
                request = request.header("x-api-key", api_key);
            }
        }
        AiProviderKind::OpenaiCompatibleChat | AiProviderKind::OpenaiResponses => {
            if let Some(api_key) = api_key.as_deref() {
                request = request.bearer_auth(api_key);
            }
        }
    }

    let response = request
        .send()
        .await
        .map_err(|error| chat_request_error(error, "获取模型列表"))?;
    if !response.status().is_success() {
        return Err(ai_error(
            "AI_PROVIDER_MODEL_LIST_HTTP_ERROR",
            format!("模型列表请求返回 HTTP {}", response.status()),
        ));
    }
    let payload = response.json::<Value>().await.map_err(|_| {
        ai_error(
            "AI_PROVIDER_MODEL_LIST_INVALID",
            "Provider 未返回有效的模型列表 JSON",
        )
    })?;
    Ok(extract_model_ids(&payload))
}

fn client(provider: &StoredAiProvider) -> Result<Client, AppError> {
    build_client(provider, REQUEST_TIMEOUT)
}

fn chat_client(provider: &StoredAiProvider) -> Result<Client, AppError> {
    build_client(provider, CHAT_REQUEST_TIMEOUT)
}

fn siliconflow_budget_model(provider: &StoredAiProvider) -> bool {
    let identity = format!(
        "{} {}",
        provider.base_url.to_ascii_lowercase(),
        provider.name.to_ascii_lowercase()
    );
    if !identity.contains("siliconflow") {
        return false;
    }

    let model = provider.model.trim().to_ascii_lowercase();
    [
        "deepseek-ai/deepseek-v4-pro",
        "deepseek-ai/deepseek-v4-flash",
        "deepseek-ai/deepseek-v3.2",
        "deepseek-ai/deepseek-r1",
        "moonshotai/kimi-k2.6",
        "zai-org/glm-5.1",
        "zai-org/glm-5",
    ]
    .iter()
    .any(|candidate| model == *candidate || model.ends_with(candidate))
        || model.starts_with("qwen/qwen3-")
}

fn siliconflow_thinking_budget(effort: AiReasoningEffort) -> Option<u32> {
    match effort {
        AiReasoningEffort::Auto => None,
        AiReasoningEffort::None => Some(0),
        AiReasoningEffort::Minimal => Some(1_024),
        AiReasoningEffort::Low => Some(4_096),
        AiReasoningEffort::Medium => Some(8_192),
        AiReasoningEffort::High => Some(16_384),
        AiReasoningEffort::Xhigh => Some(24_576),
        AiReasoningEffort::Max => Some(32_768),
    }
}

fn thinking_toggle_model(provider: &StoredAiProvider) -> bool {
    let model = provider.model.trim().to_ascii_lowercase();
    [
        "glm-5.1",
        "glm-5",
        "glm-5-turbo",
        "glm-5v-turbo",
        "glm-4.7",
        "glm-4.6",
        "glm-4.6v",
        "glm-4.5",
        "glm-4.5v",
        "kimi-k2.6",
    ]
    .iter()
    .any(|candidate| {
        model == *candidate
            || model.starts_with(&format!("{candidate}-"))
            || model.starts_with(&format!("{candidate}["))
    })
}

fn deepseek_reasoning_model(provider: &StoredAiProvider) -> bool {
    if siliconflow_budget_model(provider) {
        return false;
    }
    let model = provider.model.trim().to_ascii_lowercase();
    model.starts_with("deepseek-v4-flash")
        || model.starts_with("deepseek-v4-pro")
        || model.starts_with("deepseek-ai/deepseek-v4-flash")
        || model.starts_with("deepseek-ai/deepseek-v4-pro")
}

fn apply_deepseek_reasoning(payload: &mut Value, effort: AiReasoningEffort) {
    if effort == AiReasoningEffort::None {
        payload["thinking"] = json!({"type": "disabled"});
        return;
    }

    payload["thinking"] = json!({"type": "enabled"});
    let mapped_effort = match effort {
        AiReasoningEffort::Auto | AiReasoningEffort::None => None,
        AiReasoningEffort::Minimal | AiReasoningEffort::Low => Some("low"),
        AiReasoningEffort::Medium | AiReasoningEffort::High | AiReasoningEffort::Xhigh => Some("high"),
        AiReasoningEffort::Max => Some("max"),
    };
    if let Some(mapped_effort) = mapped_effort {
        payload["reasoning_effort"] = json!(mapped_effort);
    }
}

fn model_reasoning_config(provider: &StoredAiProvider) -> Option<&AiModelReasoningConfig> {
    provider
        .model_capabilities
        .get(&provider.model)
        .map(|capabilities| &capabilities.reasoning)
}

fn model_supports_reasoning(provider: &StoredAiProvider, effort: AiReasoningEffort) -> bool {
    if effort == AiReasoningEffort::Auto {
        return true;
    }
    let Some(config) = model_reasoning_config(provider) else {
        // Keep the pre-capability behavior for existing saved Providers. The
        // The renderer only exposes explicit values for known model templates;
        // newly configured custom models remain Auto until capability metadata
        // is available.
        return true;
    };
    config.mode != AiModelReasoningMode::None && config.efforts.contains(&effort)
}

fn configured_reasoning_parameter(
    provider: &StoredAiProvider,
    default: AiModelReasoningParameter,
) -> Option<AiModelReasoningParameter> {
    let Some(config) = model_reasoning_config(provider) else {
        return Some(default);
    };
    if config.mode == AiModelReasoningMode::None {
        return None;
    }
    Some(match config.parameter {
        AiModelReasoningParameter::Auto => default,
        parameter => parameter,
    })
}

fn default_thinking_budget(effort: AiReasoningEffort) -> Option<u32> {
    siliconflow_thinking_budget(effort)
}

fn model_thinking_budget(provider: &StoredAiProvider, effort: AiReasoningEffort) -> Option<u32> {
    let key = effort.request_value()?;
    model_reasoning_config(provider)
        .and_then(|config| config.budgets.get(key).copied())
        .or_else(|| default_thinking_budget(effort))
}

fn apply_thinking_budget(payload: &mut Value, provider: &StoredAiProvider, effort: AiReasoningEffort) {
    if effort == AiReasoningEffort::None {
        payload["enable_thinking"] = json!(false);
        return;
    }
    payload["enable_thinking"] = json!(true);
    if let Some(budget) = model_thinking_budget(provider, effort) {
        payload["thinking_budget"] = json!(budget);
    }
}

fn apply_model_reasoning(
    payload: &mut Value,
    provider: &StoredAiProvider,
    reasoning_effort: Option<AiReasoningEffort>,
    default_parameter: AiModelReasoningParameter,
) {
    let Some(effort) = reasoning_effort else {
        return;
    };
    if effort == AiReasoningEffort::Auto || !model_supports_reasoning(provider, effort) {
        return;
    }
    let Some(parameter) = configured_reasoning_parameter(provider, default_parameter) else {
        return;
    };
    match parameter {
        AiModelReasoningParameter::Auto => {}
        AiModelReasoningParameter::ReasoningEffort => {
            if let Some(value) = effort.request_value() {
                payload["reasoning_effort"] = json!(value);
            }
        }
        AiModelReasoningParameter::ReasoningObject => {
            if let Some(value) = effort.request_value() {
                payload["reasoning"] = json!({"effort": value});
            }
        }
        AiModelReasoningParameter::OutputConfigEffort => {
            if let Some(value) = effort.request_value() {
                payload["output_config"] = json!({"effort": value});
            }
        }
        AiModelReasoningParameter::ThinkingToggle => {
            payload["thinking"] = json!({
                "type": if effort == AiReasoningEffort::None {
                    "disabled"
                } else {
                    "enabled"
                }
            });
        }
        AiModelReasoningParameter::ThinkingBudget => {
            apply_thinking_budget(payload, provider, effort);
        }
        AiModelReasoningParameter::ChatTemplateReasoningEffort => {
            if let Some(value) = effort.request_value() {
                payload["chat_template_kwargs"]["reasoning_effort"] = json!(value);
            }
        }
    }
}

fn apply_openai_compatible_reasoning(
    payload: &mut Value,
    provider: &StoredAiProvider,
    reasoning_effort: Option<AiReasoningEffort>,
) {
    if deepseek_reasoning_model(provider) && model_reasoning_config(provider).is_none() {
        if let Some(effort) = reasoning_effort {
            apply_deepseek_reasoning(payload, effort);
        }
        return;
    }
    let default_parameter = if siliconflow_budget_model(provider) {
        AiModelReasoningParameter::ThinkingBudget
    } else if thinking_toggle_model(provider) {
        AiModelReasoningParameter::ThinkingToggle
    } else {
        AiModelReasoningParameter::ReasoningEffort
    };
    apply_model_reasoning(payload, provider, reasoning_effort, default_parameter);
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

fn models_url(provider: &StoredAiProvider) -> Result<Url, AppError> {
    let mut url = Url::parse(&provider.base_url)
        .map_err(|_| ai_error("AI_PROVIDER_INVALID_URL", "无法构造 Provider 模型列表地址"))?;
    let identity = format!(
        "{} {}",
        provider.base_url.to_ascii_lowercase(),
        provider.name.to_ascii_lowercase()
    );
    let is_loopback = is_trusted_loopback(&url);
    let port = url.port_or_known_default();

    if is_loopback && (port == Some(11_434) || identity.contains("ollama")) {
        url.set_path("/api/tags");
        url.set_query(None);
        return Ok(url);
    }
    if is_loopback
        && (port == Some(1_234)
            || identity.contains("lm studio")
            || identity.contains("lm-studio"))
    {
        url.set_path("/api/v1/models");
        url.set_query(None);
        return Ok(url);
    }

    let base_path = url.path().trim_end_matches('/');
    let path = if base_path.is_empty() {
        "/models".to_string()
    } else {
        format!("{base_path}/models")
    };
    url.set_path(&path);
    if identity.contains("siliconflow") {
        url.query_pairs_mut()
            .append_pair("type", "text")
            .append_pair("sub_type", "chat");
    } else {
        url.set_query(None);
    }
    Ok(url)
}

fn extract_model_ids(payload: &Value) -> Vec<AiModelInfo> {
    let mut arrays = Vec::new();
    if let Some(entries) = payload.as_array() {
        arrays.push(entries);
    }
    for key in ["data", "models"] {
        if let Some(entries) = payload.get(key).and_then(Value::as_array) {
            arrays.push(entries);
        }
    }

    let mut models = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entries in arrays {
        for entry in entries {
            let Some(object) = entry.as_object() else {
                continue;
            };
            let type_name = object
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if type_name.contains("embedding") || type_name == "embed" {
                continue;
            }
            let Some(model_id) = ["id", "key", "name", "model"]
                .iter()
                .find_map(|key| object.get(*key).and_then(Value::as_str))
                .map(str::trim)
                .filter(|model| !model.is_empty() && model.len() <= MAX_MODEL_LENGTH)
            else {
                continue;
            };
            if seen.insert(model_id.to_string()) {
                models.push(AiModelInfo {
                    id: model_id.to_string(),
                });
                if models.len() >= MAX_MODEL_LIST_ITEMS {
                    return models;
                }
            }
        }
    }
    models
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
