use serde_json::Value;

use crate::commands as shared;
use crate::web::WebServerState;
use crate::AppError;

use super::{optional_arg, required_arg, serialize};

pub(super) async fn dispatch(
    state: &WebServerState,
    command: &str,
    args: &Value,
) -> Option<Result<Value, AppError>> {
    let app = &state.app;
    let outcome = match command {
        "app_open_profile" => match required_arg(args, "profileId") {
            Ok(id) => serialize(shared::app_open_profile(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_open_profile_background" => match required_arg(args, "profileId") {
            Ok(id) => serialize(shared::app_open_profile_background(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_activate_tab" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_activate_tab(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_attach_background_session" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_attach_background_session(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_detach_session_to_background" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_detach_session_to_background(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_hide_session_to_background" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_hide_session_to_background(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_reconnect_tab" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_reconnect_tab(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_retry_monitoring" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let generation = required_arg(args, "generation")?;
                serialize(shared::app_retry_monitoring(app.clone(), tab_id, generation).await)
            }
            .await
        }
        "app_get_network_connections" => match required_arg(args, "tabId") {
            Ok(tab_id) => serialize(shared::app_get_network_connections(app.clone(), tab_id).await),
            Err(e) => Err(e),
        },
        "app_probe_topology_services" => match required_arg(args, "targets") {
            Ok(targets) => serialize(shared::app_probe_topology_services(targets).await),
            Err(e) => Err(e),
        },
        "app_set_monitoring_enabled" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let generation = required_arg(args, "generation")?;
                let enabled = required_arg(args, "enabled")?;
                serialize(
                    shared::app_set_monitoring_enabled(app.clone(), tab_id, generation, enabled)
                        .await,
                )
            }
            .await
        }
        "app_disconnect_tab" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_disconnect_tab(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_close_tab" => match required_arg(args, "tabId") {
            Ok(id) => serialize(shared::app_close_tab(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_split_tab" => {
            async {
                let source_tab_id = required_arg(args, "sourceTabId")?;
                let direction = required_arg(args, "direction")?;
                serialize(shared::app_split_tab(app.clone(), source_tab_id, direction).await)
            }
            .await
        }
        "app_close_pane" => {
            async {
                let root_tab_id = required_arg(args, "rootTabId")?;
                let pane_tab_id = required_arg(args, "paneTabId")?;
                serialize(shared::app_close_pane(app.clone(), root_tab_id, pane_tab_id).await)
            }
            .await
        }
        "app_set_active_pane" => {
            async {
                let root_tab_id = required_arg(args, "rootTabId")?;
                let pane_tab_id = required_arg(args, "paneTabId")?;
                serialize(shared::app_set_active_pane(app.clone(), root_tab_id, pane_tab_id).await)
            }
            .await
        }
        "app_set_pane_weights" => {
            async {
                let root_tab_id = required_arg(args, "rootTabId")?;
                let pane_path = required_arg(args, "panePath")?;
                let weights = required_arg(args, "weights")?;
                serialize(
                    shared::app_set_pane_weights(app.clone(), root_tab_id, pane_path, weights)
                        .await,
                )
            }
            .await
        }
        "app_open_local_terminal" => match optional_arg(args, "options") {
            Ok(options) => serialize(shared::app_open_local_terminal(app.clone(), options).await),
            Err(e) => Err(e),
        },
        "app_write_terminal" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let data = required_arg(args, "data")?;
                serialize(shared::app_write_terminal(app.clone(), tab_id, data).await)
            }
            .await
        }
        "app_resize_terminal" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let cols = required_arg(args, "cols")?;
                let rows = required_arg(args, "rows")?;
                let width = required_arg(args, "width")?;
                let height = required_arg(args, "height")?;
                serialize(
                    shared::app_resize_terminal(app.clone(), tab_id, cols, rows, width, height)
                        .await,
                )
            }
            .await
        }
        "app_open_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                serialize(shared::app_open_remote_path(app.clone(), tab_id, target_path).await)
            }
            .await
        }
        "app_list_remote_directory" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                serialize(shared::app_list_remote_directory(app.clone(), tab_id, target_path).await)
            }
            .await
        }
        "app_set_follow_shell_cwd" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let enabled = required_arg(args, "enabled")?;
                serialize(shared::app_set_follow_shell_cwd(app.clone(), tab_id, enabled).await)
            }
            .await
        }
        "app_execute_remote_command" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let command = required_arg(args, "command")?;
                let cwd = optional_arg(args, "cwd")?;
                let timeout_ms = optional_arg(args, "timeoutMs")?;
                let sudo_password = optional_arg(args, "sudoPassword")?;
                let su_password = optional_arg(args, "suPassword")?;
                let save_sudo_password = optional_arg(args, "saveSudoPassword")?;
                let save_su_password = optional_arg(args, "saveSuPassword")?;
                serialize(
                    shared::app_execute_remote_command(
                        app.clone(),
                        tab_id,
                        command,
                        cwd,
                        timeout_ms,
                        sudo_password,
                        su_password,
                        save_sudo_password,
                        save_su_password,
                    )
                    .await,
                )
            }
            .await
        }
        "app_read_remote_file" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let encoding = optional_arg(args, "encoding")?;
                serialize(
                    shared::app_read_remote_file(app.clone(), tab_id, target_path, encoding).await,
                )
            }
            .await
        }
        "app_write_remote_file" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let content = required_arg(args, "content")?;
                let encoding = optional_arg(args, "encoding")?;
                serialize(
                    shared::app_write_remote_file(
                        app.clone(),
                        tab_id,
                        target_path,
                        content,
                        encoding,
                    )
                    .await,
                )
            }
            .await
        }
        "app_create_remote_directory" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let parent_path = required_arg(args, "parentPath")?;
                let name = required_arg(args, "name")?;
                serialize(
                    shared::app_create_remote_directory(app.clone(), tab_id, parent_path, name)
                        .await,
                )
            }
            .await
        }
        "app_create_remote_file" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let parent_path = required_arg(args, "parentPath")?;
                let name = required_arg(args, "name")?;
                serialize(
                    shared::app_create_remote_file(app.clone(), tab_id, parent_path, name).await,
                )
            }
            .await
        }
        "app_copy_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let destination_path = required_arg(args, "destinationPath")?;
                let target_type = required_arg(args, "targetType")?;
                serialize(
                    shared::app_copy_remote_path(
                        app.clone(),
                        tab_id,
                        target_path,
                        destination_path,
                        target_type,
                    )
                    .await,
                )
            }
            .await
        }
        "app_move_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let destination_path = required_arg(args, "destinationPath")?;
                serialize(
                    shared::app_move_remote_path(
                        app.clone(),
                        tab_id,
                        target_path,
                        destination_path,
                    )
                    .await,
                )
            }
            .await
        }
        "app_rename_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let new_name = required_arg(args, "newName")?;
                serialize(
                    shared::app_rename_remote_path(app.clone(), tab_id, target_path, new_name)
                        .await,
                )
            }
            .await
        }
        "app_delete_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let target_type = required_arg(args, "targetType")?;
                let target_is_symlink = required_arg(args, "targetIsSymlink")?;
                serialize(
                    shared::app_delete_remote_path(
                        app.clone(),
                        tab_id,
                        target_path,
                        target_type,
                        target_is_symlink,
                    )
                    .await,
                )
            }
            .await
        }
        "app_change_remote_permissions" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let target_path = required_arg(args, "targetPath")?;
                let options = required_arg(args, "options")?;
                serialize(
                    shared::app_change_remote_permissions(
                        app.clone(),
                        tab_id,
                        target_path,
                        options,
                    )
                    .await,
                )
            }
            .await
        }
        "app_set_remote_file_access_mode" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let mode = required_arg(args, "mode")?;
                let options = optional_arg(args, "options")?;
                serialize(
                    shared::app_set_remote_file_access_mode(app.clone(), tab_id, mode, options)
                        .await,
                )
            }
            .await
        }
        "app_browse_transfer_directory" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let path = required_arg(args, "path")?;
                serialize(shared::app_browse_transfer_directory(app.clone(), tab_id, path).await)
            }
            .await
        }
        "app_copy_between_servers" => {
            async {
                let source_tab_id = required_arg(args, "sourceTabId")?;
                let target_tab_id = required_arg(args, "targetTabId")?;
                let source_path = required_arg(args, "sourcePath")?;
                let target_directory = required_arg(args, "targetDirectory")?;
                let replace_existing = required_arg(args, "replaceExisting")?;
                serialize(
                    shared::app_copy_between_servers(
                        app.clone(),
                        source_tab_id,
                        target_tab_id,
                        source_path,
                        target_directory,
                        replace_existing,
                    )
                    .await,
                )
            }
            .await
        }
        "app_queue_upload" => match required_arg(args, "fileNames") {
            Ok(file_names) => serialize(shared::app_queue_upload(app.clone(), file_names).await),
            Err(e) => Err(e),
        },
        "app_upload_file" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let local_path = required_arg(args, "localPath")?;
                let remote_directory = required_arg(args, "remoteDirectory")?;
                let options = optional_arg(args, "options")?;
                serialize(
                    shared::app_upload_file(
                        app.clone(),
                        tab_id,
                        local_path,
                        remote_directory,
                        options,
                    )
                    .await,
                )
            }
            .await
        }
        "app_download_file" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let remote_path = required_arg(args, "remotePath")?;
                let local_directory = required_arg(args, "localDirectory")?;
                let options = optional_arg(args, "options")?;
                serialize(
                    shared::app_download_file(
                        app.clone(),
                        tab_id,
                        remote_path,
                        local_directory,
                        options,
                    )
                    .await,
                )
            }
            .await
        }
        "app_download_remote_path" => {
            async {
                let tab_id = required_arg(args, "tabId")?;
                let remote_path = required_arg(args, "remotePath")?;
                let target_type = required_arg(args, "targetType")?;
                let local_directory = required_arg(args, "localDirectory")?;
                let options = optional_arg(args, "options")?;
                serialize(
                    shared::app_download_remote_path(
                        app.clone(),
                        tab_id,
                        remote_path,
                        target_type,
                        local_directory,
                        options,
                    )
                    .await,
                )
            }
            .await
        }
        "app_cancel_transfer" => match required_arg(args, "transferId") {
            Ok(id) => serialize(shared::app_cancel_transfer(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_pause_transfer" => match required_arg(args, "transferId") {
            Ok(id) => serialize(shared::app_pause_transfer(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_resume_transfer" => match required_arg(args, "transferId") {
            Ok(id) => serialize(shared::app_resume_transfer(app.clone(), id).await),
            Err(e) => Err(e),
        },
        "app_discard_transfer" => {
            async {
                let transfer_id = required_arg(args, "transferId")?;
                let force = optional_arg(args, "force")?;
                serialize(shared::app_discard_transfer(app.clone(), transfer_id, force).await)
            }
            .await
        }
        "app_clear_transfers" => match required_arg(args, "transferIds") {
            Ok(ids) => serialize(shared::app_clear_transfers(app.clone(), ids).await),
            Err(e) => Err(e),
        },
        _ => return None,
    };
    Some(outcome)
}
