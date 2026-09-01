// AI provider secret storage and conversation persistence helpers.
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
    let bytes = fs::read(&path).map_err(|error| AppError::Storage(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| AppError::Serialization(error.to_string()))
}

fn read_secret_config(app: &AppHandle) -> Result<StoredProviderSecrets, AppError> {
    let path = secret_config_path(app)?;
    if !path.exists() {
        return Ok(StoredProviderSecrets::default());
    }
    let bytes = fs::read(&path).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut config: StoredProviderSecrets = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    if decrypt_provider_secrets(&path, &mut config)? {
        write_secret_config(app, &config)?;
    }
    Ok(config)
}

fn decrypt_provider_secrets(
    path: &Path,
    config: &mut StoredProviderSecrets,
) -> Result<bool, AppError> {
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析 AI 凭据存储目录".to_string()))?;
    let mut migrated = false;
    for (provider_id, secret) in &mut config.providers {
        let (api_key, should_migrate) = crate::services::secret_crypto::decrypt_or_migrate(
            storage_root,
            &format!("ai-provider/{provider_id}/api-key"),
            &secret.api_key,
        )?;
        secret.api_key = api_key;
        migrated |= should_migrate;
    }
    Ok(migrated)
}

fn encrypt_provider_secrets(
    path: &Path,
    config: &StoredProviderSecrets,
) -> Result<StoredProviderSecrets, AppError> {
    let storage_root = path
        .parent()
        .ok_or_else(|| AppError::Storage("无法解析 AI 凭据存储目录".to_string()))?;
    let mut encrypted = config.clone();
    for (provider_id, secret) in &mut encrypted.providers {
        secret.api_key = crate::services::secret_crypto::encrypt(
            storage_root,
            &format!("ai-provider/{provider_id}/api-key"),
            &secret.api_key,
        )?;
    }
    Ok(encrypted)
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
    let path = secret_config_path(app)?;
    let encrypted = encrypt_provider_secrets(&path, config)?;
    write_json_file(&path, &encrypted)
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

fn validate_message_id(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 160
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(ai_error("AI_MESSAGE_NOT_FOUND", "AI 消息 ID 无效"));
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
    let bytes = fs::read(&path).map_err(|error| AppError::Storage(error.to_string()))?;
    let file = serde_json::from_slice::<StoredConversationFile>(&bytes)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let (messages, migrated) = migrate_legacy_messages(file.messages);
    let conversation = StoredConversation {
        schema_version: CONVERSATION_SCHEMA_VERSION,
        id: file.id,
        title: file.title,
        provider_id: file.provider_id,
        created_at: file.created_at,
        updated_at: file.updated_at,
        messages,
    };
    if conversation.id != conversation_id {
        return Err(AppError::Storage("AI 对话文件标识不匹配".to_string()));
    }
    if migrated {
        // Read-time migration is deliberately atomic at the conversation-file
        // level: after this function returns, no renderer or Provider history
        // projection can observe the legacy commands/review shape again.
        ensure_conversation_fits(&conversation)?;
        write_json_file(&path, &conversation)?;
        let mut index = read_conversation_index(app)?;
        update_conversation_index(&mut index, &conversation);
        write_conversation_index(app, &index)?;
    }
    Ok(conversation)
}

fn legacy_tool_proposal(command: LegacyAiCommandSuggestion) -> AiToolCallProposal {
    AiToolCallProposal {
        id: command.id,
        tool_name: COPILOT_EXECUTE_REMOTE_COMMAND_TOOL.to_string(),
        command: command.command,
        risk: command.risk,
        target: command.target,
        explanation: command.explanation,
        approval_request_id: None,
    }
}

fn legacy_review_result(review: LegacyAiReviewRecord) -> AiToolCallResult {
    let status = match review.outcome {
        LegacyAiReviewOutcome::Completed if review.timed_out => "timeout",
        LegacyAiReviewOutcome::Completed if review.exit_code == Some(0) => "executed",
        LegacyAiReviewOutcome::Completed => "failed",
        LegacyAiReviewOutcome::Rejected | LegacyAiReviewOutcome::ApprovalDismissed => "rejected",
        LegacyAiReviewOutcome::ApprovalTimedOut | LegacyAiReviewOutcome::CommandTimedOut => {
            "timeout"
        }
        LegacyAiReviewOutcome::TargetChanged => "target-changed",
        LegacyAiReviewOutcome::Failed => "failed",
    };
    let reason = review.error.or_else(|| match status {
        "rejected" => Some("迁移前的审批记录未执行".to_string()),
        "timeout" => Some("迁移前的执行记录已超时".to_string()),
        "target-changed" => Some("执行前终端目标已变化".to_string()),
        _ => None,
    });
    AiToolCallResult {
        proposal_id: review.command_id,
        status: status.to_string(),
        exit_code: review.exit_code,
        stdout: review.output,
        stderr: None,
        duration_ms: None,
        reason,
        record_id: Some(review.id),
        requested_at: Some(review.requested_at),
        approved_at: review.approved_at,
        completed_at: Some(review.completed_at),
        timeout_ms: Some(review.timeout_ms),
        output_truncated: Some(review.output_truncated),
    }
}

fn attach_legacy_review(
    messages: &mut Vec<AiMessage>,
    command_locations: &HashMap<String, (usize, usize)>,
    review: LegacyAiReviewRecord,
) {
    let result = legacy_review_result(review.clone());
    if let Some((message_index, activity_index)) = command_locations.get(&review.command_id) {
        if let Some(activity) = messages
            .get_mut(*message_index)
            .and_then(|message| message.tool_activities.get_mut(*activity_index))
        {
            activity.result = Some(result);
            return;
        }
    }

    // A partially written or hand-edited legacy file may contain an old review
    // record without its command proposal. Preserve the audit result as a
    // standalone unified tool activity instead of dropping history.
    let proposal = AiToolCallProposal {
        id: review.command_id.clone(),
        tool_name: COPILOT_EXECUTE_REMOTE_COMMAND_TOOL.to_string(),
        command: review.command,
        risk: review.risk,
        target: review.target,
        explanation: None,
        approval_request_id: None,
    };
    if let Some(message) = messages
        .iter_mut()
        .rev()
        .find(|message| message.role == AiMessageRole::Assistant)
    {
        message.tool_activities.push(AiToolActivity {
            proposal,
            result: Some(result),
        });
    } else {
        messages.push(AiMessage {
            id: format!(
                "legacy-review-{}",
                result.record_id.as_deref().unwrap_or("result")
            ),
            role: AiMessageRole::Assistant,
            content: String::new(),
            created_at: result.completed_at.clone().unwrap_or_else(now_timestamp),
            context: None,
            tool_activities: vec![AiToolActivity {
                proposal,
                result: Some(result),
            }],
        });
    }
}

fn migrate_legacy_messages(messages: Vec<LegacyAiMessage>) -> (Vec<AiMessage>, bool) {
    let mut migrated = false;
    let mut converted = Vec::with_capacity(messages.len());
    let mut command_locations = HashMap::<String, (usize, usize)>::new();

    for legacy in messages {
        if matches!(legacy.role, LegacyAiMessageRole::Review) {
            migrated = true;
            if let Some(review) = legacy.review {
                attach_legacy_review(&mut converted, &command_locations, review);
            }
            continue;
        }

        let role = match legacy.role {
            LegacyAiMessageRole::User => AiMessageRole::User,
            LegacyAiMessageRole::Assistant => AiMessageRole::Assistant,
            LegacyAiMessageRole::Review => unreachable!(),
        };
        let message_index = converted.len();
        let mut tool_activities = legacy.tool_activities;
        if !legacy.commands.is_empty() {
            migrated = true;
            for command in legacy.commands {
                let activity_index = tool_activities.len();
                let proposal = legacy_tool_proposal(command);
                command_locations.insert(proposal.id.clone(), (message_index, activity_index));
                tool_activities.push(AiToolActivity {
                    proposal,
                    result: None,
                });
            }
        }
        if legacy.review.is_some() {
            migrated = true;
        }
        converted.push(AiMessage {
            id: legacy.id,
            role,
            content: legacy.content,
            created_at: legacy.created_at,
            context: legacy.context,
            tool_activities,
        });
        if let Some(review) = legacy.review {
            attach_legacy_review(&mut converted, &command_locations, review);
        }
    }

    (converted, migrated)
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

fn normalize_conversation_title(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(ai_error(
            "AI_CONVERSATION_INVALID_INPUT",
            "对话标题不能为空或包含控制字符",
        ));
    }
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > MAX_CONVERSATION_TITLE_LENGTH {
        return Err(ai_error(
            "AI_CONVERSATION_INVALID_INPUT",
            "对话标题超过长度限制",
        ));
    }
    Ok(compact)
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
