// Provider HTTP clients, streaming entry points, and provider checks.
#[cfg(test)]
async fn stream_openai_compatible_chat(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_openai_compatible_chat_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
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
async fn stream_openai_responses(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_openai_responses_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
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
async fn stream_anthropic_messages(
    provider: &StoredAiProvider,
    api_key: Option<&str>,
    conversation: &StoredConversation,
    context: Option<&AiPromptContext>,
    response_mode: AiChatResponseMode,
    channel: &Channel<AiStreamEvent>,
    cancellation: &CancellationToken,
) -> Result<ChatStreamResult, AppError> {
    stream_anthropic_messages_with_tools(
        provider,
        api_key,
        conversation,
        context,
        response_mode,
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

