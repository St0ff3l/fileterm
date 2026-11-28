use serde_json::Value;

use crate::commands as shared;
use crate::runtime::StreamChannel;
use crate::web::WebServerState;
use crate::AppError;

use super::{optional_arg, required_arg, serialize, RpcRequest};

pub(super) async fn dispatch(
    state: &WebServerState,
    command: &str,
    args: &Value,
    window_label: &str,
    request: &RpcRequest,
) -> Option<Result<Value, AppError>> {
    let app = &state.app;
    let outcome = match command {
        "app_get_platform" => Ok(Value::String("web".to_string())),
        "app_get_linux_window_corner_style" => Ok(Value::String(
            shared::app_get_linux_window_corner_style().to_string(),
        )),
        "app_get_mcp_agent_setup" => serialize(shared::app_get_mcp_agent_setup()),
        "app_read_mcp_call_log" => serialize(
            shared::app_read_mcp_call_log(
                app.clone(),
                match optional_arg(args, "limit") {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                },
                match optional_arg(args, "offset") {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                },
                match optional_arg(args, "before") {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                },
            )
            .await,
        ),
        "app_get_arch" => Ok(Value::String(shared::app_get_arch())),
        "app_get_runtime_version" => {
            Ok(Value::String(format!("web-{}", env!("CARGO_PKG_VERSION"))))
        }
        "app_open_external_url" => match required_arg::<String>(args, "url") {
            Ok(url) => serialize(shared::app_open_external_url(app.clone(), url).await),
            Err(e) => Err(e),
        },
        "app_get_update_status" => serialize(shared::app_get_update_status(app.clone()).await),
        "app_check_for_updates" => serialize(shared::app_check_for_updates(app.clone()).await),
        "app_download_update" => serialize(shared::app_download_update(app.clone()).await),
        "app_install_update" => serialize(shared::app_install_update(app.clone()).await),
        "app_write_diagnostic_log" => (|| {
            let level = required_arg(args, "level")?;
            let scope = required_arg(args, "scope")?;
            let message = required_arg(args, "message")?;
            serialize(shared::app_write_diagnostic_log(
                app.clone(),
                level,
                scope,
                message,
            ))
        })(),
        "app_list_serial_ports" => serialize(shared::app_list_serial_ports().await),
        "app_serial_control" => {
            let result = async {
                let tab_id = required_arg(args, "tabId")?;
                let action = required_arg(args, "action")?;
                let value = optional_arg(args, "value")?;
                let duration_ms = optional_arg(args, "durationMs")?;
                serialize(
                    shared::app_serial_control(app.clone(), tab_id, action, value, duration_ms)
                        .await,
                )
            }
            .await;
            result
        }
        "app_serial_transfer" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let direction = required_arg(args, "direction")?;
                let mode = required_arg(args, "mode")?;
                let local_path = required_arg(args, "localPath")?;
                let file_name = optional_arg(args, "fileName")?;
                let local_paths = optional_arg(args, "localPaths")?;
                let xmodem_preserve_padding = optional_arg(args, "xmodemPreservePadding")?;
                serialize(
                    shared::app_serial_transfer(
                        app.clone(),
                        tab_id,
                        direction,
                        mode,
                        local_path,
                        file_name,
                        local_paths,
                        xmodem_preserve_padding,
                    )
                    .await,
                )
            }
            .await
        }
        "app_serial_cancel_transfer" => match required_arg::<String>(args, "tabId") {
            Ok(tab_id) => serialize(shared::app_serial_cancel_transfer(app.clone(), tab_id).await),
            Err(e) => Err(e),
        },
        "app_get_ui_preferences" => serialize(shared::app_get_ui_preferences(app.clone())),
        "app_set_ui_preferences" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_ui_preferences(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_get_security_settings" => serialize(shared::app_get_security_settings(app.clone())),
        "app_set_security_settings" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_security_settings(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_reset_security_backup_password" => {
            serialize(shared::app_reset_security_backup_password(app.clone()))
        }
        "app_verify_security_password" => match required_arg(args, "password") {
            Ok(password) => serialize(shared::app_verify_security_password(app.clone(), password)),
            Err(e) => Err(e),
        },
        "app_list_local_terminal_shells" => serialize(Ok(shared::app_list_local_terminal_shells())),
        "app_list_ai_providers" => serialize(shared::app_list_ai_providers(app.clone())),
        "app_list_ai_models" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_list_ai_models(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_save_ai_provider" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_save_ai_provider(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_delete_ai_provider" => match required_arg(args, "providerId") {
            Ok(provider_id) => serialize(shared::app_delete_ai_provider(app.clone(), provider_id)),
            Err(e) => Err(e),
        },
        "app_test_ai_provider" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_test_ai_provider(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_list_ai_conversations" => serialize(shared::app_list_ai_conversations(app.clone())),
        "app_get_ai_conversation" => match required_arg(args, "conversationId") {
            Ok(conversation_id) => serialize(shared::app_get_ai_conversation(
                app.clone(),
                conversation_id,
            )),
            Err(e) => Err(e),
        },
        "app_create_ai_conversation" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_create_ai_conversation(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_rename_ai_conversation" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_rename_ai_conversation(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_summarize_ai_conversation_title" => match required_arg(args, "input") {
            Ok(input) => {
                serialize(shared::app_summarize_ai_conversation_title(app.clone(), input).await)
            }
            Err(e) => Err(e),
        },
        "app_delete_ai_message" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_delete_ai_message(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_delete_ai_conversation" => match required_arg(args, "conversationId") {
            Ok(id) => serialize(shared::app_delete_ai_conversation(app.clone(), id)),
            Err(e) => Err(e),
        },
        "app_get_ai_copilot_mode_state" => serialize(shared::app_get_ai_copilot_mode_state(
            window_label.to_string(),
        )),
        "app_set_ai_copilot_mode" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_ai_copilot_mode(
                window_label.to_string(),
                input,
            )),
            Err(e) => Err(e),
        },
        "app_set_ai_context_attach" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_ai_context_attach(
                window_label.to_string(),
                input,
            )),
            Err(e) => Err(e),
        },
        "app_set_ai_dangerous_command_restrictions" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_ai_dangerous_command_restrictions(
                window_label.to_string(),
                input,
            )),
            Err(e) => Err(e),
        },
        "app_create_ai_context_preview" => match required_arg(args, "input") {
            Ok(input) => serialize(
                shared::app_create_ai_context_preview(app.clone(), window_label.to_string(), input)
                    .await,
            ),
            Err(e) => Err(e),
        },
        "app_start_ai_chat" => match build_ai_stream_channel(state, request) {
            Ok(channel) => match required_arg(args, "input") {
                Ok(input) => serialize(
                    shared::app_start_ai_chat(
                        app.clone(),
                        window_label.to_string(),
                        input,
                        channel,
                    )
                    .await,
                ),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        },
        "app_retry_ai_chat" => match build_ai_stream_channel(state, request) {
            Ok(channel) => match required_arg(args, "input") {
                Ok(input) => serialize(
                    shared::app_retry_ai_chat(
                        app.clone(),
                        window_label.to_string(),
                        input,
                        channel,
                    )
                    .await,
                ),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        },
        "app_cancel_ai_chat" => match required_arg(args, "requestId") {
            Ok(id) => serialize(shared::app_cancel_ai_chat(id)),
            Err(e) => Err(e),
        },
        "app_get_ui_state_item" => match required_arg(args, "key") {
            Ok(key) => serialize(shared::app_get_ui_state_item(app.clone(), key)),
            Err(e) => Err(e),
        },
        "app_set_ui_state_item" => (|| {
            let key = required_arg(args, "key")?;
            let value = required_arg(args, "value")?;
            serialize(shared::app_set_ui_state_item(app.clone(), key, value))
        })(),
        "app_remove_ui_state_item" => match required_arg(args, "key") {
            Ok(key) => serialize(shared::app_remove_ui_state_item(app.clone(), key)),
            Err(e) => Err(e),
        },
        "app_get_terminal_command_history" => match required_arg(args, "profileId") {
            Ok(profile_id) => serialize(shared::app_get_terminal_command_history(
                app.clone(),
                profile_id,
            )),
            Err(e) => Err(e),
        },
        "app_set_terminal_command_history" => (|| {
            let profile_id = required_arg(args, "profileId")?;
            let entries = required_arg(args, "entries")?;
            serialize(shared::app_set_terminal_command_history(
                app.clone(),
                profile_id,
                entries,
            ))
        })(),
        "app_get_path_bookmarks" => match required_arg(args, "scope") {
            Ok(scope) => serialize(shared::app_get_path_bookmarks(app.clone(), scope)),
            Err(e) => Err(e),
        },
        "app_update_path_bookmark" => (|| {
            let scope = required_arg(args, "scope")?;
            let action = required_arg(args, "action")?;
            let bookmark = required_arg(args, "bookmark")?;
            serialize(shared::app_update_path_bookmark(
                app.clone(),
                scope,
                action,
                bookmark,
            ))
        })(),
        "app_get_command_send_preferences" => {
            serialize(shared::app_get_command_send_preferences(app.clone()))
        }
        "app_set_command_send_preferences" => match required_arg(args, "preferences") {
            Ok(prefs) => serialize(shared::app_set_command_send_preferences(app.clone(), prefs)),
            Err(e) => Err(e),
        },
        "app_get_snapshot" => serialize(shared::app_get_snapshot(app.clone()).await),
        "app_get_connection_library" => {
            serialize(shared::app_get_connection_library(app.clone()).await)
        }
        "app_list_imported_fonts" => serialize(shared::app_list_imported_fonts(app.clone())),
        "app_get_imported_font_data" => match required_arg(args, "fontId") {
            Ok(font_id) => serialize(shared::app_get_imported_font_data(app.clone(), font_id)),
            Err(e) => Err(e),
        },
        "app_delete_imported_font" => match required_arg(args, "fontId") {
            Ok(font_id) => serialize(shared::app_delete_imported_font(app.clone(), font_id)),
            Err(e) => Err(e),
        },
        "app_list_ssh_keys" => serialize(shared::app_list_ssh_keys(app.clone())),
        "app_list_ssh_config_hosts" => serialize(shared::app_list_ssh_config_hosts(app.clone())),
        "app_import_ssh_key" => match optional_arg(args, "input") {
            Ok(input) => serialize(shared::app_import_ssh_key(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_update_ssh_key_note" => (|| {
            let key_id = required_arg(args, "keyId")?;
            let note = required_arg(args, "note")?;
            serialize(shared::app_update_ssh_key_note(app.clone(), key_id, note))
        })(),
        "app_delete_ssh_key" => match required_arg(args, "keyId") {
            Ok(key_id) => serialize(shared::app_delete_ssh_key(app.clone(), key_id)),
            Err(e) => Err(e),
        },
        "app_commit_connection_json_import" => {
            async {
                let plan_id = required_arg(args, "planId")?;
                let options = required_arg(args, "options")?;
                serialize(
                    shared::app_commit_connection_json_import(app.clone(), plan_id, options).await,
                )
            }
            .await
        }
        "app_get_webdav_sync_config" => serialize(shared::app_get_webdav_sync_config(app.clone())),
        "app_set_webdav_sync_config" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_webdav_sync_config(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_test_webdav_sync" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_test_webdav_sync(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_upload_webdav_sync" => match optional_arg(args, "mode") {
            Ok(mode) => serialize(shared::app_upload_webdav_sync(app.clone(), mode).await),
            Err(e) => Err(e),
        },
        "app_download_webdav_sync" => match optional_arg(args, "mode") {
            Ok(mode) => serialize(shared::app_download_webdav_sync(app.clone(), mode).await),
            Err(e) => Err(e),
        },
        "app_get_s3_backup_config" => serialize(shared::app_get_s3_backup_config(app.clone())),
        "app_set_s3_backup_config" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_set_s3_backup_config(app.clone(), input)),
            Err(e) => Err(e),
        },
        "app_test_s3_backup" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_test_s3_backup(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_upload_s3_backup" => match optional_arg(args, "mode") {
            Ok(mode) => serialize(shared::app_upload_s3_backup(app.clone(), mode).await),
            Err(e) => Err(e),
        },
        "app_download_s3_backup" => match optional_arg(args, "mode") {
            Ok(mode) => serialize(shared::app_download_s3_backup(app.clone(), mode).await),
            Err(e) => Err(e),
        },
        "app_workspace_mutation" => {
            async {
                let operation = required_arg(args, "operation")?;
                let payload = required_arg(args, "payload")?;
                serialize(shared::app_workspace_mutation(app.clone(), operation, payload).await)
            }
            .await
        }
        _ => return None,
    };
    Some(outcome)
}

fn build_ai_stream_channel(
    state: &WebServerState,
    request: &RpcRequest,
) -> Result<StreamChannel<crate::services::ai::AiStreamEvent>, AppError> {
    let client_id = request
        .client_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::Command("AI 流式请求缺少 WebSocket clientId".to_string()))?
        .to_string();
    let channel_id = request
        .channel_id
        .ok_or_else(|| AppError::Command("AI 流式请求缺少 channelId".to_string()))?;
    let runtime = std::sync::Arc::clone(&state.runtime);
    Ok(StreamChannel::from_callback(channel_id, move |event| {
        let payload = serde_json::to_value(event).map_err(|error| error.to_string())?;
        runtime.send_channel_event(&client_id, channel_id, payload)
    }))
}
