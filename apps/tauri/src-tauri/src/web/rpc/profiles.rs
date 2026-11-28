use serde_json::Value;

use crate::commands as shared;
use crate::sessions::local_files;
use crate::web::WebServerState;
use crate::AppError;

use super::{optional_arg, required_arg, serialize};

pub(super) async fn dispatch(
    state: &WebServerState,
    command: &str,
    args: &Value,
    window_label: &str,
) -> Option<Result<Value, AppError>> {
    let app = &state.app;
    let outcome = match command {
        "app_resolve_ssh_interaction" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let response = required_arg(args, "response")?;
                serialize(
                    shared::app_resolve_ssh_interaction(app.clone(), request_id, response).await,
                )
            }
            .await
        }
        "app_resolve_sudo_password_prompt" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let cancelled = required_arg(args, "cancelled")?;
                let value = optional_arg(args, "value")?;
                let save = optional_arg(args, "save")?;
                serialize(
                    shared::app_resolve_sudo_password_prompt(
                        app.clone(),
                        request_id,
                        cancelled,
                        value,
                        save,
                    )
                    .await,
                )
            }
            .await
        }
        "app_set_sudo_password_renderer_ready" => {
            async {
                let registration_id = required_arg(args, "registrationId")?;
                let ready = required_arg(args, "ready")?;
                serialize(
                    shared::app_set_sudo_password_renderer_ready(
                        app.clone(),
                        window_label.to_string(),
                        registration_id,
                        ready,
                    )
                    .await,
                )
            }
            .await
        }
        "app_resolve_backup_password" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let cancelled = required_arg(args, "cancelled")?;
                let value = optional_arg(args, "value")?;
                serialize(
                    shared::app_resolve_backup_password(app.clone(), request_id, cancelled, value)
                        .await,
                )
            }
            .await
        }
        "app_set_backup_password_renderer_ready" => {
            async {
                let registration_id = required_arg(args, "registrationId")?;
                let ready = required_arg(args, "ready")?;
                serialize(
                    shared::app_set_backup_password_renderer_ready(
                        app.clone(),
                        window_label.to_string(),
                        registration_id,
                        ready,
                    )
                    .await,
                )
            }
            .await
        }
        "app_list_ssh_tunnels" => match required_arg(args, "tabId") {
            Ok(tab_id) => serialize(shared::app_list_ssh_tunnels(app.clone(), tab_id).await),
            Err(e) => Err(e),
        },
        "app_create_ssh_tunnel" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let rule = required_arg(args, "rule")?;
                serialize(shared::app_create_ssh_tunnel(app.clone(), tab_id, rule).await)
            }
            .await
        }
        "app_start_ssh_tunnel" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let rule_id = required_arg(args, "ruleId")?;
                serialize(shared::app_start_ssh_tunnel(app.clone(), tab_id, rule_id).await)
            }
            .await
        }
        "app_stop_ssh_tunnel" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let rule_id = required_arg(args, "ruleId")?;
                serialize(shared::app_stop_ssh_tunnel(app.clone(), tab_id, rule_id).await)
            }
            .await
        }
        "app_delete_ssh_tunnel" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let rule_id = required_arg(args, "ruleId")?;
                serialize(shared::app_delete_ssh_tunnel(app.clone(), tab_id, rule_id).await)
            }
            .await
        }
        "app_list_tunnel_profiles" => {
            serialize(shared::app_list_tunnel_profiles(app.clone()).await)
        }
        "app_save_tunnel_profile" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_save_tunnel_profile(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_delete_tunnel_profile" => match required_arg(args, "id") {
            Ok(id) => serialize(shared::app_delete_tunnel_profile(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_list_proxy_profiles" => serialize(shared::app_list_proxy_profiles(app.clone()).await),
        "app_get_ssh_route_policy" => serialize(shared::app_get_ssh_route_policy(app.clone())),
        "app_save_ssh_route_policy" => {
            async {
                let expected = required_arg(args, "expected")?;
                let policy = required_arg(args, "policy")?;
                serialize(shared::app_save_ssh_route_policy(app.clone(), expected, policy).await)
            }
            .await
        }
        "app_probe_ssh_route_policy" => match required_arg(args, "profileId") {
            Ok(profile_id) => {
                serialize(shared::app_probe_ssh_route_policy(app.clone(), profile_id).await)
            }
            Err(e) => Err(e),
        },
        "app_save_proxy_profile" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_save_proxy_profile(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_delete_proxy_profile" => match required_arg(args, "id") {
            Ok(id) => serialize(shared::app_delete_proxy_profile(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_test_proxy_profile" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_test_proxy_profile(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_create_profile" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_create_profile(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_update_profile" => {
            async {
                let profile_id = required_arg(args, "profileId")?;
                let input = required_arg(args, "input")?;
                serialize(shared::app_update_profile(app.clone(), profile_id, input).await)
            }
            .await
        }
        "app_assign_ssh_proxy" => match required_arg(args, "input") {
            Ok(input) => serialize(shared::app_assign_ssh_proxy(app.clone(), input).await),
            Err(e) => Err(e),
        },
        "app_list_ssh_network_interfaces" => serialize(shared::app_list_ssh_network_interfaces()),
        "app_set_profile_file_pane_mode" => {
            async {
                let profile_id = required_arg(args, "profileId")?;
                let mode = required_arg(args, "mode")?;
                serialize(
                    shared::app_set_profile_file_pane_mode(app.clone(), profile_id, mode).await,
                )
            }
            .await
        }
        "app_set_profile_default_directory" => {
            async {
                let profile_id = required_arg(args, "profileId")?;
                let pane = required_arg(args, "pane")?;
                let path = required_arg(args, "path")?;
                serialize(
                    shared::app_set_profile_default_directory(app.clone(), profile_id, pane, path)
                        .await,
                )
            }
            .await
        }
        "app_clear_trusted_host_fingerprint" => match required_arg(args, "profileId") {
            Ok(profile_id) => {
                serialize(shared::app_clear_trusted_host_fingerprint(app.clone(), profile_id).await)
            }
            Err(e) => Err(e),
        },
        "app_test_connection" => {
            async {
                let profile_id = optional_arg(args, "profileId")?;
                let input = required_arg(args, "input")?;
                serialize(
                    shared::app_test_connection(
                        app.clone(),
                        window_label.to_string(),
                        profile_id,
                        input,
                    )
                    .await,
                )
            }
            .await
        }
        "app_delete_profile" => match required_arg(args, "profileId") {
            Ok(profile_id) => serialize(shared::app_delete_profile(app.clone(), profile_id).await),
            Err(e) => Err(e),
        },
        "app_update_folder" => {
            async {
                let folder_id = required_arg(args, "folderId")?;
                let updates = required_arg(args, "updates")?;
                serialize(shared::app_update_folder(app.clone(), folder_id, updates).await)
            }
            .await
        }
        "app_delete_folder" => match required_arg(args, "folderId") {
            Ok(folder_id) => serialize(shared::app_delete_folder(app.clone(), folder_id).await),
            Err(e) => Err(e),
        },
        "app_update_entity_order" => {
            async {
                let id = required_arg(args, "id")?;
                let new_parent_id = optional_arg(args, "newParentId")?;
                let new_order = required_arg(args, "newOrder")?;
                serialize(
                    shared::app_update_entity_order(app.clone(), id, new_parent_id, new_order)
                        .await,
                )
            }
            .await
        }
        "app_update_command_folder" => {
            async {
                let folder_id = required_arg(args, "folderId")?;
                let updates = required_arg(args, "updates")?;
                serialize(shared::app_update_command_folder(app.clone(), folder_id, updates).await)
            }
            .await
        }
        "app_delete_command_folder" => match required_arg(args, "folderId") {
            Ok(folder_id) => {
                serialize(shared::app_delete_command_folder(app.clone(), folder_id).await)
            }
            Err(e) => Err(e),
        },
        "app_update_command_order" => {
            async {
                let id = required_arg(args, "id")?;
                let new_parent_id = optional_arg(args, "newParentId")?;
                let new_order = required_arg(args, "newOrder")?;
                serialize(
                    shared::app_update_command_order(app.clone(), id, new_parent_id, new_order)
                        .await,
                )
            }
            .await
        }
        "app_update_command_template" => {
            async {
                let command_id = required_arg(args, "commandId")?;
                let input = required_arg(args, "input")?;
                serialize(shared::app_update_command_template(app.clone(), command_id, input).await)
            }
            .await
        }
        "app_delete_command_template" => match required_arg(args, "commandId") {
            Ok(command_id) => {
                serialize(shared::app_delete_command_template(app.clone(), command_id).await)
            }
            Err(e) => Err(e),
        },
        "app_execute_command_template" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let command_id = required_arg(args, "commandId")?;
                let cmd_args = optional_arg(args, "args")?;
                let options = optional_arg(args, "options")?;
                serialize(
                    shared::app_execute_command_template(
                        app.clone(),
                        tab_id,
                        command_id,
                        cmd_args,
                        options,
                    )
                    .await,
                )
            }
            .await
        }
        "app_run_command_batch" => {
            async {
                let command_id = required_arg(args, "commandId")?;
                let expected_template = required_arg(args, "expectedTemplate")?;
                let batch_args = required_arg(args, "args")?;
                let targets = required_arg(args, "targets")?;
                serialize(
                    shared::app_run_command_batch(
                        app.clone(),
                        window_label.to_string(),
                        command_id,
                        expected_template,
                        batch_args,
                        targets,
                    )
                    .await,
                )
            }
            .await
        }
        "app_list_command_batches" => serialize(shared::app_list_command_batches(app.clone())),
        "app_resolve_mcp_approval" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let approved = required_arg(args, "approved")?;
                serialize(shared::app_resolve_mcp_approval(app.clone(), request_id, approved).await)
            }
            .await
        }
        "app_resolve_action_approval" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let approved = required_arg(args, "approved")?;
                serialize(
                    shared::app_resolve_action_approval(app.clone(), request_id, approved).await,
                )
            }
            .await
        }
        "app_execute_ai_terminal_handoff" => {
            async {
                let request_id = required_arg(args, "requestId")?;
                let tab_id = required_arg(args, "tabId")?;
                let command = required_arg(args, "command")?;
                serialize(
                    shared::app_execute_ai_terminal_handoff(
                        app.clone(),
                        request_id,
                        tab_id,
                        command,
                    )
                    .await,
                )
            }
            .await
        }
        "app_list_local_directory" => match optional_arg(args, "dirPath") {
            Ok(dir_path) => serialize(local_files::app_list_local_directory(dir_path)),
            Err(e) => Err(e),
        },
        "app_connect_local_network_share" => {
            async {
                let path = required_arg(args, "path")?;
                let username = required_arg(args, "username")?;
                let password = required_arg(args, "password")?;
                let share = optional_arg(args, "share")?;
                serialize(
                    local_files::app_connect_local_network_share(path, username, password, share)
                        .await,
                )
            }
            .await
        }
        "app_read_local_file" => (|| {
            let file_path = required_arg(args, "filePath")?;
            let encoding = optional_arg(args, "encoding")?;
            serialize(local_files::app_read_local_file(file_path, encoding))
        })(),
        "app_write_local_file" => (|| {
            let file_path = required_arg(args, "filePath")?;
            let content = required_arg(args, "content")?;
            let encoding = optional_arg(args, "encoding")?;
            serialize(local_files::app_write_local_file(
                file_path, content, encoding,
            ))
        })(),
        "app_create_local_directory" => (|| {
            let dir_path = required_arg(args, "dirPath")?;
            let name = required_arg(args, "name")?;
            serialize(local_files::app_create_local_directory(dir_path, name))
        })(),
        "app_create_local_file" => (|| {
            let dir_path = required_arg(args, "dirPath")?;
            let name = required_arg(args, "name")?;
            serialize(local_files::app_create_local_file(dir_path, name))
        })(),
        "app_copy_local_path" => (|| {
            let source_path = required_arg(args, "sourcePath")?;
            let destination_path = required_arg(args, "destinationPath")?;
            serialize(local_files::app_copy_local_path(
                source_path,
                destination_path,
            ))
        })(),
        "app_move_local_path" => (|| {
            let source_path = required_arg(args, "sourcePath")?;
            let destination_path = required_arg(args, "destinationPath")?;
            serialize(local_files::app_move_local_path(
                source_path,
                destination_path,
            ))
        })(),
        "app_rename_local_path" => (|| {
            let target_path = required_arg(args, "targetPath")?;
            let new_name = required_arg(args, "newName")?;
            serialize(local_files::app_rename_local_path(target_path, new_name))
        })(),
        "app_delete_local_path" => match required_arg(args, "targetPath") {
            Ok(target_path) => serialize(local_files::app_delete_local_path(target_path)),
            Err(e) => Err(e),
        },
        "app_change_local_permissions" => (|| {
            let target_path = required_arg(args, "targetPath")?;
            let options = required_arg(args, "options")?;
            serialize(local_files::app_change_local_permissions(
                target_path,
                options,
            ))
        })(),
        _ => return None,
    };
    Some(outcome)
}
