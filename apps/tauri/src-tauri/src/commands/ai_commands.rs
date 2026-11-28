// AI provider, conversation, mode, preview, and chat commands.
pub fn app_list_ai_providers(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    crate::services::ai::list_providers(&app)
}

pub async fn app_list_ai_models(
    app: AppHandle,
    input: crate::services::ai::ListAiModelsInput,
) -> Result<Vec<crate::services::ai::AiModelInfo>, AppError> {
    crate::services::ai::list_models(&app, input).await
}

pub fn app_save_ai_provider(
    app: AppHandle,
    input: crate::services::ai::SaveAiProviderInput,
) -> Result<crate::services::ai::AiProviderSummary, AppError> {
    crate::services::ai::save_provider(&app, input)
}

pub fn app_delete_ai_provider(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    crate::services::ai::delete_provider(&app, &provider_id)
}

pub async fn app_test_ai_provider(
    app: AppHandle,
    input: crate::services::ai::TestAiProviderInput,
) -> Result<crate::services::ai::AiProviderTestResult, AppError> {
    crate::services::ai::test_provider(&app, input).await
}

pub fn app_list_ai_conversations(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiConversationSummary>, AppError> {
    crate::services::ai::list_conversations(&app)
}

pub fn app_get_ai_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::get_conversation(&app, &conversation_id)
}

pub fn app_create_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::CreateAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::create_conversation(&app, input)
}

pub fn app_rename_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::RenameAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::rename_conversation(&app, input)
}

pub async fn app_summarize_ai_conversation_title(
    app: AppHandle,
    input: crate::services::ai::SummarizeAiConversationTitleInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::summarize_conversation_title(&app, input).await
}

pub fn app_delete_ai_message(
    app: AppHandle,
    input: crate::services::ai::DeleteAiMessageInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    crate::services::ai::delete_message(&app, input)
}

pub fn app_delete_ai_conversation(app: AppHandle, conversation_id: String) -> Result<(), AppError> {
    crate::services::ai::delete_conversation(&app, &conversation_id)
}

pub fn app_get_ai_copilot_mode_state(
    window_label: String,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::get_copilot_mode_state(&window_label)
}

pub fn app_set_ai_copilot_mode(
    window_label: String,
    input: crate::services::ai::SetAiCopilotModeInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_copilot_mode(&window_label, input)
}

pub fn app_set_ai_context_attach(
    window_label: String,
    input: crate::services::ai::SetAiContextAttachInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_context_attach(&window_label, input)
}

pub fn app_set_ai_dangerous_command_restrictions(
    window_label: String,
    input: crate::services::ai::SetAiDangerousCommandRestrictionsInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    crate::services::ai::set_dangerous_command_restrictions(&window_label, input)
}

pub async fn app_create_ai_context_preview(
    app: AppHandle,
    window_label: String,
    input: crate::services::ai::CreateAiContextPreviewInput,
) -> Result<crate::services::ai::AiContextPreview, AppError> {
    crate::services::ai::create_context_preview(&app, &window_label, input).await
}

pub async fn app_start_ai_chat(
    app: AppHandle,
    window_label: String,
    input: crate::services::ai::StartAiChatInput,
    channel: StreamChannel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    crate::services::ai::start_chat(&app, &window_label, input, channel).await
}

pub async fn app_retry_ai_chat(
    app: AppHandle,
    window_label: String,
    input: crate::services::ai::RetryAiChatInput,
    channel: StreamChannel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    crate::services::ai::retry_chat(&app, &window_label, input, channel).await
}

pub fn app_cancel_ai_chat(request_id: String) -> Result<(), AppError> {
    crate::services::ai::cancel_chat(&request_id)
}
