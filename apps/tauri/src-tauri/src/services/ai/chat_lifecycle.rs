// Chat preparation, request registry, cancellation, and public chat entry points.
#[derive(Clone)]
struct PreparedChatRequest {
    request: AiChatRequest,
    provider: StoredAiProvider,
    api_key: Option<String>,
    conversation: StoredConversation,
    context_attachment: Option<AiContextAttachment>,
    prompt_context: Option<AiPromptContext>,
    response_mode: AiChatResponseMode,
    copilot_mode: AiCopilotMode,
    copilot_session_generation: u64,
    source_window_label: Option<String>,
}

fn prepare_copilot_mode(
    window_label: &str,
    requested_mode: AiCopilotMode,
) -> Result<StoredAiModeState, AppError> {
    let state = mode_state_for_window(window_label)?;
    if state.mode != requested_mode {
        return Err(ai_error(
            "AI_MODE_CHANGED",
            "Copilot 模式已变化，请刷新当前面板后重试",
        ));
    }
    Ok(state)
}

fn copilot_mode_state_is_current(
    state: &StoredAiModeState,
    mode: AiCopilotMode,
    session_generation: u64,
) -> bool {
    state.mode == mode && state.session_generation == session_generation && state.mode.uses_tools()
}

fn validate_context_for_mode(
    mode_state: &StoredAiModeState,
    context_attachment: Option<&AiContextAttachment>,
) -> Result<(), AppError> {
    let needs_context = effective_context_attachment(mode_state);
    if needs_context && context_attachment.is_none() {
        return Err(ai_error(
            "AI_CONTEXT_NOT_FOUND",
            "当前 Copilot 模式需要先预览并确认 L2 终端上下文",
        ));
    }
    if context_attachment.is_some_and(|attachment| attachment.mode != AiContextMode::Level2) {
        return Err(ai_error(
            "AI_CONTEXT_TARGET_CHANGED",
            "当前 Copilot 模式只接受 L2 终端上下文，请重新预览",
        ));
    }
    Ok(())
}

async fn prepare_start_chat(
    app: &AppHandle,
    window_label: &str,
    input: StartAiChatInput,
) -> Result<PreparedChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let provider_id = normalize_provider_id(&input.provider_id)?;
    let user_message = normalize_user_message(&input.user_message)?;
    let mode_state = prepare_copilot_mode(window_label, input.mode)?;
    let response_mode = AiChatResponseMode::Chat;
    let (mut provider, api_key) = resolve_chat_provider(app, &provider_id)?;
    if let Some(ref model_override) = input.model_override {
        let m = model_override.trim();
        if !m.is_empty() {
            provider.model = m.to_string();
        }
    };
    let (context_attachment, prompt_context) = match input.context_snapshot_id.as_deref() {
        Some(snapshot_id) => consume_context_snapshot(app, window_label, &provider.id, snapshot_id)
            .await
            .map(|(attachment, prompt)| (Some(attachment), Some(prompt)))?,
        None => (None, None),
    };
    validate_context_for_mode(&mode_state, context_attachment.as_ref())?;

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
        context: context_attachment.clone(),
        tool_activities: Vec::new(),
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
        context_attachment,
        prompt_context,
        response_mode,
        copilot_mode: mode_state.mode,
        copilot_session_generation: mode_state.session_generation,
        source_window_label: Some(window_label.to_string()),
    })
}

async fn prepare_retry_chat(
    app: &AppHandle,
    window_label: &str,
    input: RetryAiChatInput,
) -> Result<PreparedChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let provider_id = normalize_provider_id(&input.provider_id)?;
    let mode_state = prepare_copilot_mode(window_label, input.mode)?;
    let response_mode = AiChatResponseMode::Chat;
    let (mut provider, api_key) = resolve_chat_provider(app, &provider_id)?;
    if let Some(ref model_override) = input.model_override {
        let m = model_override.trim();
        if !m.is_empty() {
            provider.model = m.to_string();
        }
    }
    let (context_attachment, prompt_context) = match input.context_snapshot_id.as_deref() {
        Some(snapshot_id) => consume_context_snapshot(app, window_label, &provider.id, snapshot_id)
            .await
            .map(|(attachment, prompt)| (Some(attachment), Some(prompt)))?,
        None => (None, None),
    };
    validate_context_for_mode(&mode_state, context_attachment.as_ref())?;

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
    let mut conversation_changed = false;
    if let Some(context_attachment) = context_attachment.as_ref() {
        if let Some(message) = conversation
            .messages
            .iter_mut()
            .find(|message| message.id == user_message_id)
        {
            message.context = Some(context_attachment.clone());
            conversation_changed = true;
        }
    }
    if conversation.provider_id != provider.id {
        conversation.provider_id = provider.id.clone();
        conversation_changed = true;
    }
    if conversation_changed {
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
        context_attachment,
        prompt_context,
        response_mode,
        copilot_mode: mode_state.mode,
        copilot_session_generation: mode_state.session_generation,
        source_window_label: Some(window_label.to_string()),
    })
}

fn append_assistant_messages(
    app: &AppHandle,
    request: &AiChatRequest,
    messages: Vec<AssistantMessageDraft>,
) -> Result<AiConversation, AppError> {
    if messages
        .iter()
        .all(|message| message.content.trim().is_empty())
    {
        return Err(ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回可显示的回答",
        ));
    }
    let content_length = messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum::<usize>();
    if content_length > MAX_ASSISTANT_MESSAGE_LENGTH {
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
    for message in messages {
        conversation.messages.push(AiMessage {
            id: message.id,
            role: AiMessageRole::Assistant,
            content: message.content,
            created_at: conversation.updated_at.clone(),
            context: None,
            tool_activities: message.tool_activities,
        });
    }
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
    // AI failures are created as `AppError::Command`, whose display wrapper
    // prefixes the payload with `command error: `. Strip only that stable
    // wrapper before decoding the typed AI error. Otherwise every streamed
    // AI failure silently degrades to the generic connection error.
    let raw = raw.strip_prefix("command error: ").unwrap_or(&raw);
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

pub async fn start_chat(
    app: &AppHandle,
    window: &WebviewWindow,
    input: StartAiChatInput,
    channel: Channel<AiStreamEvent>,
) -> Result<AiChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let request_id = crate::storage::new_id("ai-request");
    let cancellation = CancellationToken::new();
    register_chat_request(&request_id, &conversation_id, cancellation.clone())?;
    let prepared = match prepare_start_chat(app, window.label(), input).await {
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

pub async fn retry_chat(
    app: &AppHandle,
    window: &WebviewWindow,
    input: RetryAiChatInput,
    channel: Channel<AiStreamEvent>,
) -> Result<AiChatRequest, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let request_id = crate::storage::new_id("ai-request");
    let cancellation = CancellationToken::new();
    register_chat_request(&request_id, &conversation_id, cancellation.clone())?;
    let prepared = match prepare_retry_chat(app, window.label(), input).await {
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

#[derive(Clone, Debug)]
struct ProviderToolCall {
    id: String,
    item_id: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ToolCallAccumulator {
    id: String,
    item_id: String,
    name: String,
    arguments: String,
}

#[derive(Clone, Debug)]
struct ToolLoopResult {
    call_id: String,
    content: String,
}

#[derive(Clone, Debug)]
struct ToolLoopTurn {
    assistant_text: String,
    calls: Vec<ProviderToolCall>,
    results: Vec<ToolLoopResult>,
}

struct AssistantMessageDraft {
    id: String,
    content: String,
    tool_activities: Vec<AiToolActivity>,
}

#[derive(Default)]
struct ChatStreamResult {
    content: String,
    finish_reason: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    tool_calls: Vec<ProviderToolCall>,
    tool_call_accumulators: BTreeMap<String, ToolCallAccumulator>,
}

impl ChatStreamResult {
    fn finalize_tool_calls(&mut self) {
        if !self.tool_calls.is_empty() {
            return;
        }
        self.tool_calls = self
            .tool_call_accumulators
            .values()
            .filter(|call| !call.name.trim().is_empty())
            .enumerate()
            .map(|(index, call)| ProviderToolCall {
                id: if call.id.trim().is_empty() {
                    format!("fileterm-tool-call-{index}")
                } else {
                    call.id.clone()
                },
                item_id: (!call.item_id.trim().is_empty()).then(|| call.item_id.clone()),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            })
            .collect();
    }
}
