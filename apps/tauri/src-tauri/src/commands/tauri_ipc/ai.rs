use tauri::ipc::Channel;
use tauri::{AppHandle, WebviewWindow};

use crate::commands as shared;
use crate::runtime::StreamChannel;
use crate::AppError;

#[tauri::command]
pub fn app_list_ai_providers(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    shared::app_list_ai_providers(crate::runtime_app(&app))
}

#[tauri::command]
pub async fn app_list_ai_models(
    app: AppHandle,
    input: crate::services::ai::ListAiModelsInput,
) -> Result<Vec<crate::services::ai::AiModelInfo>, AppError> {
    shared::app_list_ai_models(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub fn app_save_ai_provider(
    app: AppHandle,
    input: crate::services::ai::SaveAiProviderInput,
) -> Result<crate::services::ai::AiProviderSummary, AppError> {
    shared::app_save_ai_provider(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_delete_ai_provider(
    app: AppHandle,
    provider_id: String,
) -> Result<Vec<crate::services::ai::AiProviderSummary>, AppError> {
    shared::app_delete_ai_provider(crate::runtime_app(&app), provider_id)
}

#[tauri::command]
pub async fn app_test_ai_provider(
    app: AppHandle,
    input: crate::services::ai::TestAiProviderInput,
) -> Result<crate::services::ai::AiProviderTestResult, AppError> {
    shared::app_test_ai_provider(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub fn app_list_ai_conversations(
    app: AppHandle,
) -> Result<Vec<crate::services::ai::AiConversationSummary>, AppError> {
    shared::app_list_ai_conversations(crate::runtime_app(&app))
}

#[tauri::command]
pub fn app_get_ai_conversation(
    app: AppHandle,
    conversation_id: String,
) -> Result<crate::services::ai::AiConversation, AppError> {
    shared::app_get_ai_conversation(crate::runtime_app(&app), conversation_id)
}

#[tauri::command]
pub fn app_create_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::CreateAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    shared::app_create_ai_conversation(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_rename_ai_conversation(
    app: AppHandle,
    input: crate::services::ai::RenameAiConversationInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    shared::app_rename_ai_conversation(crate::runtime_app(&app), input)
}

#[tauri::command]
pub async fn app_summarize_ai_conversation_title(
    app: AppHandle,
    input: crate::services::ai::SummarizeAiConversationTitleInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    shared::app_summarize_ai_conversation_title(crate::runtime_app(&app), input).await
}

#[tauri::command]
pub fn app_delete_ai_message(
    app: AppHandle,
    input: crate::services::ai::DeleteAiMessageInput,
) -> Result<crate::services::ai::AiConversation, AppError> {
    shared::app_delete_ai_message(crate::runtime_app(&app), input)
}

#[tauri::command]
pub fn app_delete_ai_conversation(app: AppHandle, conversation_id: String) -> Result<(), AppError> {
    shared::app_delete_ai_conversation(crate::runtime_app(&app), conversation_id)
}

#[tauri::command]
pub fn app_get_ai_copilot_mode_state(
    window: WebviewWindow,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    shared::app_get_ai_copilot_mode_state(window.label().to_string())
}

#[tauri::command]
pub fn app_set_ai_copilot_mode(
    window: WebviewWindow,
    input: crate::services::ai::SetAiCopilotModeInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    shared::app_set_ai_copilot_mode(window.label().to_string(), input)
}

#[tauri::command]
pub fn app_set_ai_context_attach(
    window: WebviewWindow,
    input: crate::services::ai::SetAiContextAttachInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    shared::app_set_ai_context_attach(window.label().to_string(), input)
}

#[tauri::command]
pub fn app_set_ai_dangerous_command_restrictions(
    window: WebviewWindow,
    input: crate::services::ai::SetAiDangerousCommandRestrictionsInput,
) -> Result<crate::services::ai::AiCopilotModeState, AppError> {
    shared::app_set_ai_dangerous_command_restrictions(window.label().to_string(), input)
}

#[tauri::command]
pub async fn app_create_ai_context_preview(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::CreateAiContextPreviewInput,
) -> Result<crate::services::ai::AiContextPreview, AppError> {
    shared::app_create_ai_context_preview(
        crate::runtime_app(&app),
        window.label().to_string(),
        input,
    )
    .await
}

#[tauri::command]
pub async fn app_start_ai_chat(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::StartAiChatInput,
    channel: Channel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    shared::app_start_ai_chat(
        crate::runtime_app(&app),
        window.label().to_string(),
        input,
        StreamChannel::from_tauri(channel),
    )
    .await
}

#[tauri::command]
pub async fn app_retry_ai_chat(
    app: AppHandle,
    window: WebviewWindow,
    input: crate::services::ai::RetryAiChatInput,
    channel: Channel<crate::services::ai::AiStreamEvent>,
) -> Result<crate::services::ai::AiChatRequest, AppError> {
    shared::app_retry_ai_chat(
        crate::runtime_app(&app),
        window.label().to_string(),
        input,
        StreamChannel::from_tauri(channel),
    )
    .await
}

#[tauri::command]
pub fn app_cancel_ai_chat(request_id: String) -> Result<(), AppError> {
    shared::app_cancel_ai_chat(request_id)
}
