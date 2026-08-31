// Conversation validation, CRUD, summaries, and title generation.
fn review_target_label(target: &AiContextTarget) -> String {
    match target.user.as_deref() {
        Some(user) if !user.is_empty() => format!("{user}@{}", target.display_host),
        _ => target.display_host.clone(),
    }
}

fn review_risk_label(risk: &AiCommandRisk) -> &'static str {
    match risk {
        AiCommandRisk::ReadOnly => "只读",
        AiCommandRisk::Mutating => "会修改状态",
        AiCommandRisk::Destructive => "可能破坏数据",
        AiCommandRisk::Privileged => "需要高权限",
        AiCommandRisk::Unknown => "风险未知",
    }
}

fn sanitize_review_error(value: &str) -> String {
    let (message, _, _) = sanitize_recent_terminal_output(value);
    truncate_characters(&message, 1_024)
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

pub fn rename_conversation(
    app: &AppHandle,
    input: RenameAiConversationInput,
) -> Result<AiConversation, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let title = normalize_conversation_title(&input.title)?;
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    let mut conversation = read_stored_conversation(app, &conversation_id)?;
    conversation.title = title;
    conversation.updated_at = now_timestamp();
    persist_conversation(app, &mut index, &conversation)?;
    Ok(public_conversation(conversation))
}

/// Generate and persist a title automatically after the first user message.
/// This request never consumes a terminal context snapshot and never persists
/// an AI message or a remote conversation copy.
pub async fn summarize_conversation_title(
    app: &AppHandle,
    input: SummarizeAiConversationTitleInput,
) -> Result<AiConversation, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let provider_id = normalize_provider_id(&input.provider_id)?;
    let (mut provider, api_key) = resolve_chat_provider(app, &provider_id)?;
    if let Some(ref model_override) = input.model_override {
        let model = model_override.trim();
        if !model.is_empty() {
            provider.model = model.to_string();
        }
    }

    let (conversation, initial_title) = {
        let _guard = conversation_store_lock()?;
        let index = read_conversation_index(app)?;
        require_indexed_conversation(&index, &conversation_id)?;
        let conversation = read_stored_conversation(app, &conversation_id)?;
        let initial_title = conversation.title.clone();
        (conversation, initial_title)
    };
    if title_summary_history_items(&conversation).is_empty() {
        return Err(ai_error(
            "AI_CONVERSATION_INVALID_INPUT",
            "当前对话还没有可用于总结标题的本地文本",
        ));
    }

    let semaphore = CHAT_REQUEST_SEMAPHORE.clone();
    let _permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| ai_error("AI_CONVERSATION_LIMIT", "AI 对话队列当前不可用，请稍后重试"))?;
    let client = chat_client(&provider)?;
    let request_url = match &provider.kind {
        AiProviderKind::OpenaiCompatibleChat => chat_completions_url(&provider)?,
        AiProviderKind::OpenaiResponses => responses_url(&provider)?,
        AiProviderKind::AnthropicMessages => anthropic_messages_url(&provider)?,
    };
    let mut request = match &provider.kind {
        AiProviderKind::OpenaiCompatibleChat => client.post(request_url).json(&json!({
            "model": provider.model,
            "messages": title_summary_chat_messages(&conversation),
            "stream": false
        })),
        AiProviderKind::OpenaiResponses => client.post(request_url).json(&json!({
            "model": provider.model,
            "instructions": TITLE_SUMMARY_SYSTEM_PROMPT,
            "input": title_summary_input_items(&conversation),
            "stream": false,
            "store": false
        })),
        AiProviderKind::AnthropicMessages => client
            .post(request_url)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .json(&json!({
                "model": provider.model,
                "system": TITLE_SUMMARY_SYSTEM_PROMPT,
                "messages": title_summary_input_items(&conversation),
                "max_tokens": 64,
                "stream": false
            })),
    };
    if let Some(api_key) = api_key.as_deref() {
        request = match &provider.kind {
            AiProviderKind::AnthropicMessages => request.header("x-api-key", api_key),
            AiProviderKind::OpenaiCompatibleChat | AiProviderKind::OpenaiResponses => {
                request.bearer_auth(api_key)
            }
        };
    }
    let payload = send_json_request(request).await?;
    let raw_title = match &provider.kind {
        AiProviderKind::OpenaiCompatibleChat => payload
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(text_from_content),
        AiProviderKind::OpenaiResponses => response_output_text(&payload),
        AiProviderKind::AnthropicMessages => anthropic_content_text(&payload),
    }
    .ok_or_else(|| {
        ai_error(
            "AI_PROVIDER_RESPONSE_INVALID",
            "AI Provider 未返回可用的标题",
        )
    })?;
    let title = normalize_ai_title_suggestion(&raw_title)?;
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    let mut latest = read_stored_conversation(app, &conversation_id)?;
    // Match OpenCode's default-title guard: a user rename that happened while
    // the background request was running always wins over the AI suggestion.
    if latest.title == initial_title {
        latest.title = title;
        latest.updated_at = now_timestamp();
        persist_conversation(app, &mut index, &latest)?;
    }
    Ok(public_conversation(latest))
}

pub fn delete_message(
    app: &AppHandle,
    input: DeleteAiMessageInput,
) -> Result<AiConversation, AppError> {
    let conversation_id = validate_conversation_id(&input.conversation_id)?;
    let message_id = validate_message_id(&input.message_id)?;
    // Keep the request registry lock while acquiring the file-store lock so a
    // message cannot be removed while the same conversation is still being
    // generated. Idle conversations remain removable even when another
    // conversation is streaming.
    let chat_guard = chat_registry_lock()?;
    if chat_guard.by_conversation.contains_key(&conversation_id) {
        return Err(ai_error(
            "AI_CONVERSATION_ACTIVE",
            "当前对话正在生成，请等待完成后再删除消息",
        ));
    }
    let _guard = conversation_store_lock()?;
    let mut index = read_conversation_index(app)?;
    require_indexed_conversation(&index, &conversation_id)?;
    let mut conversation = read_stored_conversation(app, &conversation_id)?;
    let before = conversation.messages.len();
    conversation
        .messages
        .retain(|message| message.id != message_id);
    if before == conversation.messages.len() {
        return Err(ai_error("AI_MESSAGE_NOT_FOUND", "找不到指定的 AI 消息"));
    }
    conversation.updated_at = now_timestamp();
    persist_conversation(app, &mut index, &conversation)?;
    Ok(public_conversation(conversation))
}

pub fn delete_conversation(app: &AppHandle, conversation_id: &str) -> Result<(), AppError> {
    let conversation_id = validate_conversation_id(conversation_id)?;
    // Keep the request registry lock while acquiring the file-store lock so a
    // conversation cannot disappear between the active-request check and the
    // index/file deletion. The renderer may delete an idle history item while
    // another conversation is streaming, but the currently generating item
    // must remain available until its request is stopped or completed.
    let chat_guard = chat_registry_lock()?;
    if chat_guard.by_conversation.contains_key(&conversation_id) {
        return Err(ai_error(
            "AI_CONVERSATION_ACTIVE",
            "当前对话正在生成，请等待完成后再删除",
        ));
    }
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
