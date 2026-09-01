// Bridge dispatch, access policy, approval policy, and tool risk rules.
async fn dispatch_bridge_request(
    app: &AppHandle,
    request: BridgeRequest,
    progress_sender: Option<mpsc::UnboundedSender<BridgeProgress>>,
) -> Result<Value, String> {
    let progress_token = request.progress_token.clone();
    let policy = enforce_mcp_access_policy(app, &request).await?;
    if should_request_mcp_approval(&policy, &request) {
        if let Some(progress_sender) = progress_sender.as_ref() {
            let _ = progress_sender.send(BridgeProgress::action_approval_waiting(
                &request.action,
                progress_token.clone(),
            ));
        }
        request_mcp_approval(app, request.source, &request.action, &request.params).await?;
    }

    match request.action.as_str() {
        "list_connections" => list_connections(app, &request.params).await,
        "get_session_context" => get_session_context(app, &request.params).await,
        "get_command_templates" => get_command_templates(app, &request.params).await,
        "list_remote_directory" => list_remote_directory(app, &request.params).await,
        "read_remote_file" => read_remote_file(app, &request.params).await,
        "list_transfers" => list_transfers(app, &request.params).await,
        "wait_for_transfer" => wait_for_transfer(app, &request.params).await,
        "wait_for_connection" => {
            wait_for_connection(app, &request.params, progress_sender, progress_token).await
        }
        "list_ssh_tunnels" => list_ssh_tunnels(app, &request.params).await,
        "open_connection" => {
            open_connection(
                app,
                &request.params,
                request.source,
                progress_sender,
                progress_token,
            )
            .await
        }
        "activate_session" => activate_session(app, &request.params).await,
        "reconnect_session" => reconnect_session(app, &request.params).await,
        "disconnect_session" => disconnect_session(app, &request.params).await,
        "close_session" => close_session(app, &request.params).await,
        "execute_remote_command" => {
            execute_remote_command(app, &request.params, progress_sender, progress_token).await
        }
        "execute_visible_command" => execute_visible_command(app, &request.params).await,
        "execute_command_template" => execute_command_template(app, &request.params).await,
        "write_remote_file" => write_remote_file(app, &request.params).await,
        "create_remote_directory" => create_remote_directory(app, &request.params).await,
        "create_remote_file" => create_remote_file(app, &request.params).await,
        "copy_remote_path" => copy_remote_path(app, &request.params).await,
        "move_remote_path" => move_remote_path(app, &request.params).await,
        "rename_remote_path" => rename_remote_path(app, &request.params).await,
        "delete_remote_path" => delete_remote_path(app, &request.params).await,
        "change_remote_permissions" => change_remote_permissions(app, &request.params).await,
        "set_remote_file_access_mode" => set_remote_file_access_mode(app, &request.params).await,
        "upload_file" => upload_file(app, &request.params).await,
        "download_file" => download_file(app, &request.params).await,
        "download_remote_directory" => download_remote_directory(app, &request.params).await,
        "pause_transfer" => transfer_action(app, &request.params, "pause").await,
        "resume_transfer" => transfer_action(app, &request.params, "resume").await,
        "discard_transfer" => transfer_action(app, &request.params, "discard").await,
        "clear_transfers" => clear_transfers(app, &request.params).await,
        "create_ssh_tunnel" => create_ssh_tunnel(app, &request.params).await,
        "start_ssh_tunnel" => tunnel_action(app, &request.params, "start").await,
        "stop_ssh_tunnel" => tunnel_action(app, &request.params, "stop").await,
        "delete_ssh_tunnel" => tunnel_action(app, &request.params, "delete").await,
        _ => Err("Unsupported FileTerm MCP action".to_string()),
    }
}

/// External Agents never get a wider capability than the policy selected in
/// FileTerm settings. This check belongs on the desktop bridge rather than in
/// the stdio MCP child process so MCP, the explicit CLI and future local
/// clients share the same decision point.
async fn enforce_mcp_access_policy(
    app: &AppHandle,
    request: &BridgeRequest,
) -> Result<McpAccessPolicy, String> {
    let policy = mcp_access_policy(app)?;
    if policy.operation_policy == "read-only"
        && !action_is_read_only(&request.action, &request.params)
    {
        return Err(format!(
            "{MCP_POLICY_READ_ONLY}: FileTerm is configured to allow only read-only external operations"
        ));
    }

    match policy.connection_scope.as_str() {
        "all-saved-connections" => {}
        "selected-connections" => enforce_selected_connection_scope(app, request, &policy).await?,
        _ => {
            return Err(format!(
                "{MCP_SCOPE_DENIED}: invalid saved connection scope"
            ))
        }
    }
    Ok(policy)
}

fn mcp_access_policy(app: &AppHandle) -> Result<McpAccessPolicy, String> {
    let preferences =
        crate::commands::app_get_ui_preferences(app.clone()).map_err(public_app_error)?;
    Ok(McpAccessPolicy {
        connection_scope: preferences.mcp_agent.connection_scope,
        operation_policy: preferences.mcp_agent.operation_policy,
        allowed_profile_ids: preferences
            .mcp_agent
            .allowed_profile_ids
            .into_iter()
            .collect(),
    })
}

fn should_request_mcp_approval(policy: &McpAccessPolicy, request: &BridgeRequest) -> bool {
    matches!(
        policy.operation_policy.as_str(),
        "basic-safe-operations" | "approved-operations"
    ) && request.requires_approval
        && action_requires_approval(&request.action, &request.params)
}

async fn enforce_selected_connection_scope(
    app: &AppHandle,
    request: &BridgeRequest,
    policy: &McpAccessPolicy,
) -> Result<(), String> {
    if matches!(
        request.action.as_str(),
        "get_command_templates"
            | "list_transfers"
            | "wait_for_transfer"
            | "pause_transfer"
            | "resume_transfer"
            | "discard_transfer"
            | "clear_transfers"
    ) {
        return Ok(());
    }
    if request.action == "list_connections" {
        return Ok(());
    }
    if request.action == "open_connection" {
        let requested_profile = required_string(&request.params, "profile_id", 256)?;
        return policy
            .allowed_profile_ids
            .contains(&requested_profile)
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "{MCP_SCOPE_DENIED}: this Agent is limited to its selected saved connections"
                )
            });
    }
    if request.action == "wait_for_connection" {
        return enforce_connection_operation_scope(app, request, &policy.allowed_profile_ids).await;
    }
    if request.action == "get_session_context" {
        let requested_profile = optional_string(&request.params, "profile_id", 256)?;
        return requested_profile
            .as_deref()
            .is_none_or(|profile_id| policy.allowed_profile_ids.contains(profile_id))
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "{MCP_SCOPE_DENIED}: this Agent is limited to its selected saved connections"
                )
            });
    }

    let tab_id = required_string(&request.params, "tab_id", 256)?;
    let snapshot = crate::commands::get_workspace_snapshot(app.clone())
        .await
        .map_err(public_app_error)?;
    let profile_id = snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .and_then(|tabs| {
            tabs.iter()
                .find(|tab| tab.get("id").and_then(Value::as_str) == Some(tab_id.as_str()))
        })
        .and_then(|tab| tab.get("profileId"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{MCP_SCOPE_DENIED}: requested session was not found"))?;
    policy
        .allowed_profile_ids
        .contains(profile_id)
        .then_some(())
        .ok_or_else(|| {
            format!("{MCP_SCOPE_DENIED}: this Agent is limited to its selected saved connections")
        })
}

async fn enforce_connection_operation_scope(
    app: &AppHandle,
    request: &BridgeRequest,
    allowed_profile_ids: &HashSet<String>,
) -> Result<(), String> {
    let operation_id = required_string(&request.params, "operation_id", 256)?;
    let info = app
        .state::<crate::services::workspace::WorkspaceState>()
        .connection_operations
        .info(&operation_id)
        .await
        .map_err(|error| format!("{MCP_SCOPE_DENIED}: {error}"))?;
    if !allowed_profile_ids.contains(&info.profile_id) {
        return Err(format!(
            "{MCP_SCOPE_DENIED}: this Agent is limited to its selected saved connections"
        ));
    }
    Ok(())
}

fn session_tab_ids_for_profile(snapshot: &Value, profile_id: &str) -> Vec<String> {
    snapshot
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|tab| tab.get("profileId").and_then(Value::as_str) == Some(profile_id))
        .filter_map(|tab| tab.get("id").and_then(Value::as_str).map(ToOwned::to_owned))
        .collect()
}

async fn mcp_visibility(app: &AppHandle) -> Result<McpVisibility, String> {
    let policy = mcp_access_policy(app)?;
    match policy.connection_scope.as_str() {
        "all-saved-connections" => Ok(McpVisibility::all_saved_connections()),
        "selected-connections" => {
            let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                .await
                .map_err(public_app_error)?;
            let existing_profile_ids = snapshot
                .get("profiles")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|profile| profile.get("id").and_then(Value::as_str))
                .collect::<HashSet<_>>();
            let profile_ids = policy
                .allowed_profile_ids
                .iter()
                .filter(|profile_id| existing_profile_ids.contains(profile_id.as_str()))
                .cloned()
                .collect::<HashSet<_>>();
            let tab_ids = profile_ids
                .iter()
                .flat_map(|profile_id| session_tab_ids_for_profile(&snapshot, profile_id))
                .collect::<HashSet<_>>();
            Ok(McpVisibility {
                scope: McpVisibilityScope::SelectedConnections,
                profile_ids,
                tab_ids,
            })
        }
        _ => Err(format!(
            "{MCP_SCOPE_DENIED}: invalid saved connection scope"
        )),
    }
}

fn action_requires_approval(action: &str, params: &Value) -> bool {
    match action {
        // Basic observation and workspace-context actions are automatic in
        // the middle policy.
        action if action_is_read_only(action, params) => false,
        // Ordinary safe remote commands use the same local classifier as the
        // built-in Copilot. Mutating, destructive, privileged, and unknown
        // commands return to the FileTerm approval dialog.
        "execute_remote_command" => params
            .get("command")
            .and_then(Value::as_str)
            .map(|command| !is_basic_safe_command(command))
            .unwrap_or(true),
        // A saved template is rendered later by the command-template route;
        // keep it approval-gated because its final command is not available at
        // this policy boundary and its positional arguments may change it.
        // Unknown/future actions also stay approval-gated by default.
        _ => true,
    }
}

fn action_is_read_only(action: &str, _params: &Value) -> bool {
    matches!(
        action,
        "list_connections"
            | "get_session_context"
            | "get_command_templates"
            | "list_remote_directory"
            | "read_remote_file"
            | "list_transfers"
            | "wait_for_transfer"
            | "wait_for_connection"
            | "list_ssh_tunnels"
            | "activate_session"
    )
}

fn action_approval_source(source: WorkspaceSessionSource) -> ActionApprovalSource {
    match source {
        WorkspaceSessionSource::Cli => ActionApprovalSource::Cli,
        WorkspaceSessionSource::Mcp => ActionApprovalSource::Mcp,
    }
}

async fn request_mcp_approval(
    app: &AppHandle,
    source: WorkspaceSessionSource,
    action: &str,
    params: &Value,
) -> Result<(), String> {
    let approval_source = action_approval_source(source);
    let details = approval_details(app, action, params).await?;
    let decision = request_action_approval(app, approval_source, action, details)
        .await
        .map_err(public_app_error)?;
    match decision {
        ActionApprovalDecision::Approved => Ok(()),
        decision => Err(decision.rejection_message(approval_source).to_string()),
    }
}

async fn approval_details(
    app: &AppHandle,
    action: &str,
    params: &Value,
) -> Result<ActionApprovalDetails, String> {
    let tab_id = optional_string(params, "tab_id", 256)?;
    let target = match action {
        "open_connection" => optional_string(params, "profile_id", 256)?,
        "write_remote_file"
        | "delete_remote_path"
        | "change_remote_permissions"
        | "set_remote_file_access_mode" => optional_string(params, "path", 4_096)?,
        "copy_remote_path" | "move_remote_path" => {
            optional_string(params, "destination_path", 4_096)?
        }
        "rename_remote_path" => optional_string(params, "target_path", 4_096)?,
        "upload_file" => optional_string(params, "local_path", 4_096)?,
        "download_file" | "download_remote_directory" => {
            optional_string(params, "remote_path", 4_096)?
        }
        "pause_transfer" | "resume_transfer" | "discard_transfer" => {
            optional_string(params, "transfer_id", 256)?
        }
        "clear_transfers" => params.get("transfer_ids").map(|value| {
            truncate_text(
                &serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
                4_096,
            )
        }),
        "start_ssh_tunnel" | "stop_ssh_tunnel" | "delete_ssh_tunnel" => {
            optional_string(params, "rule_id", 256)?
        }
        "create_remote_directory" | "create_remote_file" => Some(format!(
            "父目录：{}\n名称：{}",
            required_string(params, "parent_path", 4_096)?,
            required_string(params, "name", 512)?
        )),
        "create_ssh_tunnel" => params
            .get("rule")
            .and_then(Value::as_object)
            .and_then(|rule| rule.get("name"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        _ => tab_id.clone(),
    };
    let details = match action {
        "execute_remote_command" | "execute_visible_command" => Some(truncate_text(
            &required_text(params, "command", 64 * 1024)?,
            4 * 1024,
        )),
        "execute_command_template" => {
            let command_id = required_string(params, "command_id", 256)?;
            let snapshot = crate::commands::get_workspace_snapshot(app.clone())
                .await
                .map_err(public_app_error)?;
            let template = snapshot
                .get("commandTemplates")
                .and_then(Value::as_array)
                .and_then(|templates| {
                    templates.iter().find(|template| {
                        template.get("id").and_then(Value::as_str) == Some(command_id.as_str())
                    })
                });
            let template_text = template
                .and_then(|template| template.get("command"))
                .and_then(Value::as_str)
                .map(|command| truncate_text(command, 4 * 1024))
                .unwrap_or_else(|| "未找到命令模板内容".to_string());
            Some(format!(
                "命令模板：{}\n命令：{}\n参数：{}",
                command_id,
                template_text,
                serde_json::to_string(params.get("args").unwrap_or(&Value::Null))
                    .unwrap_or_else(|_| "null".to_string())
            ))
        }
        "write_remote_file" => {
            let content = required_text(params, "content", MCP_MAX_FILE_CONTENT_BYTES)?;
            Some(format!(
                "写入 {} 字节{}",
                content.len(),
                if content.is_empty() {
                    String::new()
                } else {
                    format!("\n内容预览：{}", truncate_text(&content, 1_000))
                }
            ))
        }
        "upload_file" => Some(format!(
            "本地源：{}\n远端目录：{}",
            required_string(params, "local_path", 4_096)?,
            required_string(params, "remote_directory", 4_096)?
        )),
        "download_file" | "download_remote_directory" => Some(format!(
            "远端源：{}\n本地目录：{}",
            required_string(params, "remote_path", 4_096)?,
            required_string(params, "local_directory", 4_096)?
        )),
        "copy_remote_path" | "move_remote_path" => Some(format!(
            "源路径：{}\n目标路径：{}",
            required_string(params, "target_path", 4_096)?,
            required_string(params, "destination_path", 4_096)?
        )),
        "rename_remote_path" => Some(format!(
            "原路径：{}\n新名称：{}",
            required_string(params, "target_path", 4_096)?,
            required_string(params, "new_name", 512)?
        )),
        "change_remote_permissions" => Some(format!(
            "模式：{}\n递归：{}\n应用范围：{}",
            required_string(params, "mode", 4)?,
            optional_bool(params, "recursive")?.unwrap_or(false),
            optional_string(params, "apply_to", 32)?.unwrap_or_else(|| "all".to_string())
        )),
        "set_remote_file_access_mode" => Some(format!(
            "访问模式：{}",
            required_string(params, "mode", 16)?
        )),
        "clear_transfers" => Some(format!(
            "传输任务：{}",
            serde_json::to_string(params.get("transfer_ids").unwrap_or(&Value::Null))
                .unwrap_or_else(|_| "null".to_string())
        )),
        "create_ssh_tunnel" => Some(format!(
            "规则：{}",
            truncate_text(
                &serde_json::to_string(params.get("rule").unwrap_or(&Value::Null))
                    .unwrap_or_else(|_| "null".to_string()),
                4 * 1024
            )
        )),
        _ => None,
    };
    let summary = match action {
        "open_connection" => "打开 FileTerm 连接".to_string(),
        "reconnect_session" => "重新连接 FileTerm 会话".to_string(),
        "disconnect_session" => "断开 FileTerm 会话".to_string(),
        "close_session" => "关闭 FileTerm 标签页".to_string(),
        "execute_remote_command" => "在远程 SSH 主机后台执行命令".to_string(),
        "execute_visible_command" => "在可见 FileTerm 终端执行命令".to_string(),
        "execute_command_template" => "执行 FileTerm 命令模板".to_string(),
        "write_remote_file" => "写入远程文件".to_string(),
        "create_remote_directory" => "创建远程目录".to_string(),
        "create_remote_file" => "创建远程文件".to_string(),
        "copy_remote_path" => "复制远程文件或目录".to_string(),
        "move_remote_path" => "移动远程文件或目录".to_string(),
        "rename_remote_path" => "重命名远程文件或目录".to_string(),
        "delete_remote_path" => "删除远程文件或目录".to_string(),
        "change_remote_permissions" => "修改远程文件权限".to_string(),
        "set_remote_file_access_mode" => "切换远程文件访问身份".to_string(),
        "upload_file" => "上传本地文件或目录".to_string(),
        "download_file" => "下载远程文件".to_string(),
        "download_remote_directory" => "下载远程目录".to_string(),
        "pause_transfer" => "暂停传输任务".to_string(),
        "resume_transfer" => "继续传输任务".to_string(),
        "discard_transfer" => "丢弃传输任务断点".to_string(),
        "clear_transfers" => "清理传输历史".to_string(),
        "create_ssh_tunnel" => "创建 SSH 隧道".to_string(),
        "start_ssh_tunnel" => "启动 SSH 隧道".to_string(),
        "stop_ssh_tunnel" => "停止 SSH 隧道".to_string(),
        "delete_ssh_tunnel" => "删除 SSH 隧道".to_string(),
        _ => format!("外部客户端请求未识别的 FileTerm 操作：{action}"),
    };
    let details = details.or_else(|| {
        (!matches!(
            action,
            "open_connection"
                | "reconnect_session"
                | "disconnect_session"
                | "close_session"
                | "execute_remote_command"
                | "execute_visible_command"
                | "execute_command_template"
                | "write_remote_file"
                | "create_remote_directory"
                | "create_remote_file"
                | "copy_remote_path"
                | "move_remote_path"
                | "rename_remote_path"
                | "delete_remote_path"
                | "change_remote_permissions"
                | "set_remote_file_access_mode"
                | "upload_file"
                | "download_file"
                | "download_remote_directory"
                | "pause_transfer"
                | "resume_transfer"
                | "discard_transfer"
                | "clear_transfers"
                | "create_ssh_tunnel"
                | "start_ssh_tunnel"
                | "stop_ssh_tunnel"
                | "delete_ssh_tunnel"
        ))
        .then(|| {
            format!(
                "操作：{}\n参数：{}",
                action,
                truncate_text(
                    &serde_json::to_string(params).unwrap_or_else(|_| "null".to_string()),
                    4 * 1024
                )
            )
        })
    });
    Ok(ActionApprovalDetails {
        title: "FileTerm 外部操作需要确认".to_string(),
        summary,
        target: target.or(tab_id),
        details,
        destructive: matches!(
            action,
            "write_remote_file"
                | "delete_remote_path"
                | "change_remote_permissions"
                | "set_remote_file_access_mode"
                | "discard_transfer"
                | "clear_transfers"
                | "delete_ssh_tunnel"
        ),
        requires_risk_acknowledgement: false,
    })
}
