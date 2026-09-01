// MCP bridge, CLI, policy, and JSON-RPC tests.
#[cfg(test)]
mod tests {
    use super::{
        action_approval_source, action_is_read_only, bridge_request_timeout, compact_session,
        handle_jsonrpc_request, initialize_result, mcp_cancel_request_id, mcp_error_code,
        mcp_error_is_retryable,
        optional_string, pagination, requested_execution_mode, should_request_mcp_approval,
        tool_definitions, tool_error_result, validate_tool_arguments, write_mcp_progress,
        ActionApprovalSource, BridgeProgress, BridgeRequest, McpAccessPolicy, McpVisibility,
        McpVisibilityScope, EXECUTION_MODE_BACKGROUND, EXECUTION_MODE_VISIBLE_TERMINAL,
        MCP_BRIDGE_TIMEOUT, MCP_CONNECTION_WAIT_TIMEOUT, MCP_JSONRPC_PROTOCOL_VERSION,
        BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED, DEFAULT_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS,
        FILETERM_REMOTE_COMMAND_LIMIT, FILETERM_REMOTE_COMMAND_NOT_FOUND,
        FILETERM_REMOTE_COMMAND_SCOPE_MISMATCH, MAX_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS,
        NETWORK_DEVICE_COMMAND_INVALID, NETWORK_DEVICE_CWD_UNSUPPORTED,
        NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED, SUDO_PASSWORD_CANCELLED, SUDO_PASSWORD_NEEDED,
        VISIBLE_TERMINAL_COMMAND_INVALID, VISIBLE_TERMINAL_SESSION_NOT_ACTIVE,
    };
    use super::{
        cli_bridge_request, cli_exec_action, cli_jsonl_bridge_request, cli_jsonl_request_key,
        decode_cli_secret_bytes, parse_cli_options_with_flags, validate_cli_jsonl_cancel_params,
        validate_cli_jsonl_request, CliJsonlRequest, CliJsonlRequestControls,
    };
    use crate::services::workspace::WorkspaceSessionSource;
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::time::Duration;

    #[test]
    fn cli_and_mcp_bridge_requests_keep_distinct_session_sources() {
        let cli = cli_bridge_request("list_connections", json!({}));
        assert_eq!(cli.source, WorkspaceSessionSource::Cli);

        let cli_jsonl_input = CliJsonlRequest {
            id: json!("request-1"),
            action: "list_connections".to_string(),
            params: json!({}),
            requires_approval: true,
            progress_token: None,
        };
        let cli_jsonl = cli_jsonl_bridge_request(&cli_jsonl_input);
        assert_eq!(cli_jsonl.source, WorkspaceSessionSource::Cli);

        let mcp = BridgeRequest {
            action: "list_connections".to_string(),
            params: json!({}),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: true,
            progress_token: None,
        };
        assert_eq!(mcp.source, WorkspaceSessionSource::Mcp);
    }

    #[test]
    fn cli_and_mcp_approval_requests_keep_distinct_sources() {
        assert_eq!(
            action_approval_source(WorkspaceSessionSource::Cli),
            ActionApprovalSource::Cli
        );
        assert_eq!(
            action_approval_source(WorkspaceSessionSource::Mcp),
            ActionApprovalSource::Mcp
        );
    }

    #[test]
    fn compact_session_exposes_cli_or_mcp_source_without_leaking_gui_defaults() {
        let cli = compact_session(
            &json!({ "source": "cli", "isBackground": true }),
            &json!({ "connected": true }),
            "tab-cli",
        );
        assert_eq!(cli["source"], "cli");

        let gui = compact_session(
            &json!({ "isBackground": false }),
            &json!({ "connected": true }),
            "tab-gui",
        );
        assert!(gui["source"].is_null());
    }

    #[test]
    fn cli_jsonl_requests_require_ids_and_object_params() {
        let valid = serde_json::from_value::<CliJsonlRequest>(json!({
            "id": "request-1",
            "action": "list_connections"
        }))
        .unwrap();
        assert!(validate_cli_jsonl_request(&valid).is_ok());

        let missing_id = serde_json::from_value::<CliJsonlRequest>(json!({
            "id": null,
            "action": "list_connections"
        }))
        .unwrap();
        assert!(validate_cli_jsonl_request(&missing_id).is_err());

        let invalid_params = serde_json::from_value::<CliJsonlRequest>(json!({
            "id": "request-2",
            "action": "list_connections",
            "params": []
        }))
        .unwrap();
        assert!(validate_cli_jsonl_request(&invalid_params).is_err());
    }

    #[test]
    fn cli_jsonl_requests_cannot_disable_desktop_approval() {
        let request = serde_json::from_value::<CliJsonlRequest>(json!({
            "id": "request-1",
            "action": "write_remote_file",
            "params": {},
            "requiresApproval": false
        }))
        .unwrap();
        assert!(validate_cli_jsonl_request(&request).is_ok());
        assert!(cli_jsonl_bridge_request(&request).requires_approval);
    }

    #[test]
    fn direct_cli_requests_use_the_shared_approval_gate() {
        assert!(cli_bridge_request("execute_remote_command", json!({})).requires_approval);
        assert!(cli_bridge_request("list_connections", json!({})).requires_approval);
    }

    #[test]
    fn cli_jsonl_request_cancellation_is_single_use_and_id_scoped() {
        let controls = CliJsonlRequestControls::default();
        let request_id = json!("request-1");
        let cancellation = controls.register(&request_id).unwrap();
        assert!(!cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(controls.cancel(&request_id).unwrap());
        assert!(cancellation.load(std::sync::atomic::Ordering::Acquire));
        assert!(!controls.cancel(&json!("request-2")).unwrap());
        controls.remove(&request_id);
        assert!(!controls.cancel(&request_id).unwrap());
        assert!(controls.register(&request_id).is_ok());
    }

    #[test]
    fn cli_jsonl_cancel_requests_validate_target_ids() {
        assert_eq!(
            validate_cli_jsonl_cancel_params(&json!({ "request_id": 7 })).unwrap(),
            json!(7)
        );
        assert!(validate_cli_jsonl_cancel_params(&json!({})).is_err());
        assert!(validate_cli_jsonl_cancel_params(&json!({
            "request_id": "request-1",
            "extra": true
        }))
        .is_err());
        assert!(validate_cli_jsonl_cancel_params(&json!({ "request_id": true })).is_err());
        assert!(cli_jsonl_request_key(&Value::Null).is_err());
    }

    #[test]
    fn cli_password_stdin_flags_are_valueless_and_bounded() {
        let arguments = vec![
            "--tab-id".to_string(),
            "tab-1".to_string(),
            "--command".to_string(),
            "sudo id".to_string(),
            "--sudo-password-stdin".to_string(),
            "--save-sudo-password".to_string(),
            "true".to_string(),
        ];
        let (values, flags) = parse_cli_options_with_flags(
            &arguments,
            &[
                "tab-id",
                "command",
                "save-sudo-password",
                "sudo-password",
                "sudo-password-stdin",
            ],
            &["sudo-password-stdin"],
        )
        .unwrap();
        assert_eq!(values.get("tab-id"), Some(&"tab-1".to_string()));
        assert_eq!(values.get("save-sudo-password"), Some(&"true".to_string()));
        assert!(flags.contains("sudo-password-stdin"));

        assert_eq!(
            decode_cli_secret_bytes("--sudo-password-stdin", b"  secret  \r".to_vec(), true)
                .unwrap(),
            "  secret  "
        );
        assert!(decode_cli_secret_bytes("--sudo-password-stdin", Vec::new(), true).is_err());
        assert!(
            decode_cli_secret_bytes("--sudo-password-stdin", vec![b'x'; 4 * 1024 + 1], false)
                .is_err()
        );
    }

    #[test]
    fn cli_password_argv_and_stdin_sources_cannot_be_combined() {
        let arguments = vec![
            "--tab-id".to_string(),
            "tab-1".to_string(),
            "--command".to_string(),
            "sudo id".to_string(),
            "--sudo-password".to_string(),
            "secret".to_string(),
            "--sudo-password-stdin".to_string(),
        ];
        let error = cli_exec_action(&arguments).unwrap_err();
        assert!(error.contains("either --sudo-password or --sudo-password-stdin"));
        assert!(!error.contains("secret"));
    }

    #[test]
    fn tools_are_prefixed_and_have_strict_schemas() {
        for tool in tool_definitions() {
            assert!(tool["name"].as_str().unwrap().starts_with("fileterm_"));
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            assert_eq!(tool["outputSchema"]["type"], "object");
        }
        let read_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_read_remote_file")
            .unwrap();
        assert_eq!(read_tool["annotations"]["readOnlyHint"], true);
        let write_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_write_remote_file")
            .unwrap();
        assert_eq!(write_tool["annotations"]["readOnlyHint"], false);
        assert_eq!(write_tool["annotations"]["destructiveHint"], true);
        let remote_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_execute_remote_command")
            .unwrap();
        assert!(remote_tool["description"]
            .as_str()
            .unwrap()
            .contains("REMOTE_INTERACTIVE_INPUT_REQUIRED"));
        assert!(remote_tool["description"]
            .as_str()
            .unwrap()
            .contains("progress/log notification"));
        assert!(remote_tool["description"]
            .as_str()
            .unwrap()
            .contains(NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED));
        assert!(remote_tool["description"]
            .as_str()
            .unwrap()
            .contains("never writes to the visible terminal"));
        assert_eq!(
            remote_tool["outputSchema"]["required"],
            json!(["tabId", "executionMode", "result"])
        );
        assert!(
            remote_tool["outputSchema"]["properties"]["result"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "inputRequired")
        );
        assert!(
            remote_tool["outputSchema"]["properties"]["result"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "rawTerminal")
        );
        assert_eq!(
            remote_tool["outputSchema"]["properties"]["result"]["properties"]["rawTerminal"],
            json!({ "type": "boolean" })
        );
        assert_eq!(
            remote_tool["outputSchema"]["properties"]["result"]["properties"]["inputKind"],
            json!({ "type": "string", "enum": ["secret", "text"] })
        );

        let open_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_open_connection")
            .unwrap();
        assert_eq!(
            open_tool["inputSchema"]["required"],
            json!(["profile_id", "execution_mode"])
        );
        assert_eq!(
            open_tool["inputSchema"]["properties"]["execution_mode"]["enum"],
            json!(["background", "visible-terminal"])
        );
        assert!(open_tool["description"]
            .as_str()
            .unwrap()
            .contains("non-active"));

        let visible_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_execute_visible_command")
            .unwrap();
        assert_eq!(
            visible_tool["inputSchema"]["required"],
            json!(["tab_id", "command"])
        );
        assert_eq!(
            visible_tool["outputSchema"]["required"],
            json!(["tabId", "executionMode", "accepted", "result"])
        );
        assert!(visible_tool["description"]
            .as_str()
            .unwrap()
            .contains("already-active visible SSH terminal"));

        let command_list_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_list_remote_commands")
            .unwrap();
        assert_eq!(
            command_list_tool["inputSchema"]["required"],
            json!(["tab_id"])
        );
        assert_eq!(
            command_list_tool["outputSchema"]["required"],
            json!(["total", "count", "offset", "items", "hasMore", "nextOffset"])
        );
        assert_eq!(command_list_tool["annotations"]["readOnlyHint"], true);

        let transfer_wait_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_wait_for_transfer")
            .unwrap();
        assert_eq!(
            transfer_wait_tool["inputSchema"]["required"],
            json!(["transfer_id"])
        );
        assert_eq!(
            transfer_wait_tool["outputSchema"]["required"],
            json!(["transferId", "transfer", "timedOut"])
        );

        let start_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_start_remote_command")
            .unwrap();
        assert!(start_tool["description"]
            .as_str()
            .unwrap()
            .contains("never automatically rerun"));
        assert_eq!(
            start_tool["inputSchema"]["properties"]["timeout_ms"]["default"],
            DEFAULT_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS
        );
        assert_eq!(
            start_tool["inputSchema"]["properties"]["timeout_ms"]["maximum"],
            MAX_BACKGROUND_REMOTE_EXEC_TIMEOUT_MS
        );
        assert!(!start_tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .contains_key("save_sudo_password"));

        let background_read_tool = tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_read_remote_command")
            .unwrap();
        assert_eq!(background_read_tool["annotations"]["readOnlyHint"], true);
        assert_eq!(
            background_read_tool["inputSchema"]["properties"]["wait_ms"]["maximum"],
            30_000
        );

        for command_tool in [
            "fileterm_terminate_remote_command",
            "fileterm_close_remote_command",
        ] {
            let tool = tool_definitions()
                .into_iter()
                .find(|tool| tool["name"] == command_tool)
                .unwrap();
            assert_eq!(
                tool["inputSchema"]["required"],
                json!(["tab_id", "command_id"])
            );
        }
        assert!(tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_terminate_remote_command")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .contains("same SSH channel"));
        assert!(tool_definitions()
            .into_iter()
            .find(|tool| tool["name"] == "fileterm_close_remote_command")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .contains("retained output"));
    }

    #[test]
    fn pagination_enforces_a_bounded_positive_limit() {
        assert_eq!(pagination(&json!({})).unwrap(), (20, 0));
        assert!(pagination(&json!({ "limit": 0 })).is_err());
        assert!(pagination(&json!({ "limit": 101 })).is_err());
    }

    #[test]
    fn selected_visibility_is_limited_to_allowed_profiles_and_tabs() {
        let visibility = McpVisibility {
            scope: McpVisibilityScope::SelectedConnections,
            profile_ids: HashSet::from(["profile-1".to_string()]),
            tab_ids: HashSet::from(["tab-1".to_string()]),
        };
        assert!(visibility.allows_profile(Some("profile-1")));
        assert!(!visibility.allows_profile(Some("profile-2")));
        assert!(visibility.allows_tab(Some("tab-1")));
        assert!(!visibility.allows_tab(Some("tab-2")));
        assert!(visibility.allows_transfer_value(&json!({ "tabId": "tab-1" })));
        assert!(!visibility.allows_transfer_value(&json!({ "tabId": "tab-2" })));
    }

    #[test]
    fn basic_safe_operations_gate_side_effects_but_allow_observations() {
        let request = BridgeRequest {
            action: "write_remote_file".to_string(),
            params: json!({}),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: true,
            progress_token: None,
        };
        let full_access = McpAccessPolicy {
            connection_scope: "selected-connections".to_string(),
            operation_policy: "full-access".to_string(),
            allowed_profile_ids: HashSet::new(),
        };
        let basic_safe_operations = McpAccessPolicy {
            operation_policy: "basic-safe-operations".to_string(),
            ..full_access.clone()
        };
        let observation = BridgeRequest {
            action: "list_remote_directory".to_string(),
            ..request.clone()
        };
        let ordinary_command = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({ "command": "uname -a" }),
            ..request.clone()
        };
        let background_safe_command = BridgeRequest {
            action: "start_remote_command".to_string(),
            params: json!({ "command": "uname -a" }),
            ..request.clone()
        };
        let background_deployment = BridgeRequest {
            action: "start_remote_command".to_string(),
            params: json!({ "command": "docker compose up -d" }),
            ..request.clone()
        };
        let background_read = BridgeRequest {
            action: "read_remote_command".to_string(),
            params: json!({ "tab_id": "tab-1", "command_id": "command-1" }),
            ..request.clone()
        };
        let privileged_command = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({ "command": "sudo id" }),
            ..request.clone()
        };
        let destructive_command = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({ "command": "rm -rf /tmp/fileterm" }),
            ..request.clone()
        };
        let restart_command = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({ "command": "reboot" }),
            ..request.clone()
        };
        let unknown = BridgeRequest {
            action: "future_action".to_string(),
            ..request.clone()
        };
        assert!(!should_request_mcp_approval(&full_access, &request));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &request
        ));
        assert!(!should_request_mcp_approval(
            &basic_safe_operations,
            &observation
        ));
        assert!(!should_request_mcp_approval(
            &basic_safe_operations,
            &ordinary_command
        ));
        assert!(!should_request_mcp_approval(
            &basic_safe_operations,
            &background_safe_command
        ));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &background_deployment
        ));
        assert!(action_is_read_only(
            &background_read.action,
            &background_read.params
        ));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &privileged_command
        ));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &destructive_command
        ));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &restart_command
        ));
        assert!(!action_is_read_only(
            &ordinary_command.action,
            &ordinary_command.params
        ));
        assert!(should_request_mcp_approval(
            &basic_safe_operations,
            &unknown
        ));
        assert!(should_request_mcp_approval(
            &McpAccessPolicy {
                operation_policy: "approved-operations".to_string(),
                ..full_access
            },
            &request
        ));
    }

    #[test]
    fn string_parameters_reject_empty_and_oversized_values() {
        assert!(optional_string(&json!({ "tab_id": "" }), "tab_id", 10).is_err());
        assert!(optional_string(&json!({ "tab_id": "01234567890" }), "tab_id", 10).is_err());
    }

    #[test]
    fn tools_list_is_returned_over_json_rpc() {
        let response = handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        assert!(response["result"]["tools"].as_array().unwrap().len() >= 20);
    }

    #[test]
    fn initialize_negotiates_the_supported_protocol_version() {
        let response = handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": { "protocolVersion": "2099-01-01" }
        }))
        .unwrap();
        assert_eq!(
            response["result"]["protocolVersion"],
            MCP_JSONRPC_PROTOCOL_VERSION
        );
    }

    #[test]
    fn initialize_instructions_describe_credential_and_generic_input_paths() {
        let result = initialize_result(&json!({})).expect("initialize result should be valid");
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.contains("REMOTE_INTERACTIVE_INPUT_REQUIRED"));
        assert!(instructions.contains("visible SSH terminal"));
        assert!(instructions.contains("ask the user"));
        assert!(instructions.contains("sudo/su"));
        assert!(instructions.contains("progress/log notification"));
        assert!(instructions.contains("fileterm_execute_visible_command"));
        assert!(instructions.contains("non-active"));
        assert!(instructions.contains("execution_mode"));
        assert!(instructions.contains("fileterm_start_remote_command"));
        assert!(instructions.contains("increasing offset"));
    }

    #[test]
    fn notifications_produce_no_stdio_response() {
        assert!(handle_jsonrpc_request(json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }))
        .is_none());
    }

    #[test]
    fn mcp_cancellation_notification_extracts_the_target_request_id() {
        assert_eq!(
            mcp_cancel_request_id(&json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": { "requestId": "exec-1" }
            })),
            Some(json!("exec-1"))
        );
        assert_eq!(
            mcp_cancel_request_id(&json!({
                "method": "notifications/cancelled",
                "params": { "request_id": 7 }
            })),
            Some(json!(7))
        );
        assert!(mcp_cancel_request_id(&json!({
            "method": "notifications/message",
            "params": { "requestId": "exec-1" }
        }))
        .is_none());
    }

    #[test]
    fn tool_arguments_reject_unknown_or_non_object_fields() {
        assert!(
            validate_tool_arguments("fileterm_list_connections", &json!({ "secret": true }))
                .is_err()
        );
        assert!(validate_tool_arguments("fileterm_get_session_context", &json!("bad")).is_err());
        assert!(validate_tool_arguments(
            "fileterm_execute_unsupported_legacy_tool",
            &json!({ "command": "sudo id" })
        )
        .is_err());
        assert!(validate_tool_arguments(
            "fileterm_wait_for_transfer",
            &json!({
                "transfer_id": "transfer-1",
                "timeout_ms": 30_000
            })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_wait_for_connection",
            &json!({
                "operation_id": "connection-1",
                "timeout_ms": 30_000
            })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_open_connection",
            &json!({ "profile_id": "profile-1" })
        )
        .is_err());
        assert!(validate_tool_arguments(
            "fileterm_open_connection",
            &json!({
                "profile_id": "profile-1",
                "execution_mode": EXECUTION_MODE_BACKGROUND
            })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_execute_visible_command",
            &json!({ "tab_id": "tab-1", "command": "uname -a" })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_start_remote_command",
            &json!({ "tab_id": "tab-1", "command": "docker compose up -d" })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_start_remote_command",
            &json!({
                "tab_id": "tab-1",
                "command": "docker compose up -d",
                "save_sudo_password": true
            })
        )
        .is_err());
        assert!(validate_tool_arguments(
            "fileterm_read_remote_command",
            &json!({
                "tab_id": "tab-1",
                "command_id": "command-1",
                "offset": 128,
                "wait_ms": 30_000
            })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_close_remote_command",
            &json!({ "tab_id": "tab-1", "command_id": "command-1" })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_list_remote_commands",
            &json!({ "tab_id": "tab-1", "limit": 10, "offset": 0 })
        )
        .is_ok());
        assert!(validate_tool_arguments(
            "fileterm_delete_remote_path",
            &json!({
                "tab_id": "tab-1",
                "target_path": "/tmp/link",
                "target_type": "file",
                "target_is_symlink": true
            })
        )
        .is_ok());
    }

    #[test]
    fn execution_mode_is_explicit_and_strict() {
        assert_eq!(
            requested_execution_mode(&json!({})).unwrap(),
            EXECUTION_MODE_BACKGROUND
        );
        assert_eq!(
            requested_execution_mode(&json!({
                "execution_mode": EXECUTION_MODE_VISIBLE_TERMINAL
            }))
            .unwrap(),
            EXECUTION_MODE_VISIBLE_TERMINAL
        );
        assert!(requested_execution_mode(&json!({
            "execution_mode": "auto"
        }))
        .is_err());
    }

    #[test]
    fn tool_errors_include_stable_codes_and_retry_semantics() {
        let unavailable = tool_error_result(
            "REMOTE_INTERACTIVE_INPUT_REQUIRED: finish the operation in the visible SSH terminal"
                .to_string(),
        );
        assert_eq!(
            unavailable["structuredContent"]["error"]["code"],
            "REMOTE_INTERACTIVE_INPUT_REQUIRED"
        );
        assert_eq!(unavailable["structuredContent"]["error"]["retryable"], true);

        let rejected =
            tool_error_result("FileTerm external operation was rejected by the user".to_string());
        assert_eq!(
            rejected["structuredContent"]["error"]["code"],
            "FILETERM_OPERATION_REJECTED"
        );
        assert_eq!(rejected["structuredContent"]["error"]["retryable"], false);
    }

    #[test]
    fn cli_exec_keeps_the_bridge_open_for_a_foreground_password_prompt() {
        let request = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({}),
            source: WorkspaceSessionSource::Cli,
            requires_approval: true,
            progress_token: None,
        };
        assert!(bridge_request_timeout(&request) > MCP_BRIDGE_TIMEOUT);
    }

    #[test]
    fn ordinary_cli_exec_keeps_the_bridge_open_for_bounded_execution() {
        let request = BridgeRequest {
            action: "execute_remote_command".to_string(),
            params: json!({ "command": "uname -a" }),
            source: WorkspaceSessionSource::Cli,
            requires_approval: true,
            progress_token: None,
        };
        assert!(bridge_request_timeout(&request) > MCP_BRIDGE_TIMEOUT);
    }

    #[test]
    fn background_command_reads_keep_the_bridge_open_for_long_polling() {
        let request = BridgeRequest {
            action: "read_remote_command".to_string(),
            params: json!({
                "tab_id": "tab-1",
                "command_id": "command-1",
                "wait_ms": 30_000
            }),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: false,
            progress_token: None,
        };
        assert!(bridge_request_timeout(&request) > Duration::from_secs(30));
    }

    #[test]
    fn background_command_start_keeps_the_bridge_open_for_channel_setup() {
        let request = BridgeRequest {
            action: "start_remote_command".to_string(),
            params: json!({
                "tab_id": "tab-1",
                "command": "docker compose up -d"
            }),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: true,
            progress_token: None,
        };
        assert!(bridge_request_timeout(&request) > Duration::from_secs(35));
    }

    #[test]
    fn opening_and_waiting_for_a_connection_have_bounded_foreground_timeouts() {
        let open = BridgeRequest {
            action: "open_connection".to_string(),
            params: json!({ "profile_id": "profile-1" }),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: false,
            progress_token: None,
        };
        let wait = BridgeRequest {
            action: "wait_for_connection".to_string(),
            params: json!({ "operation_id": "connection-1" }),
            source: WorkspaceSessionSource::Mcp,
            requires_approval: false,
            progress_token: None,
        };
        assert!(bridge_request_timeout(&open) > MCP_CONNECTION_WAIT_TIMEOUT);
        assert_eq!(bridge_request_timeout(&wait), MCP_CONNECTION_WAIT_TIMEOUT);
    }

    #[test]
    fn privileged_prompt_progress_uses_mcp_notifications_without_secrets() {
        let progress = BridgeProgress::privileged_password_prompt(
            SUDO_PASSWORD_NEEDED,
            Some(json!("progress-1")),
        );
        let mut output = Vec::new();
        write_mcp_progress(&mut output, &progress).expect("progress notification should encode");
        let notification: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(notification["method"], "notifications/progress");
        assert_eq!(notification["params"]["progressToken"], "progress-1");
        assert_eq!(notification["params"]["message"], progress.message);
        assert!(!notification.to_string().contains("password="));

        let progress_without_token =
            BridgeProgress::privileged_password_prompt(SUDO_PASSWORD_NEEDED, None);
        output.clear();
        write_mcp_progress(&mut output, &progress_without_token)
            .expect("logging notification should encode");
        let notification: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(notification["method"], "notifications/message");
        assert_eq!(notification["params"]["logger"], "fileterm");
    }

    #[test]
    fn action_approval_progress_explains_that_the_call_is_waiting() {
        let progress = BridgeProgress::action_approval_waiting(
            "execute_visible_command",
            Some(json!("progress-1")),
        );
        assert_eq!(progress.status, "input-required");
        assert_eq!(progress.code, "FILETERM_ACTION_APPROVAL_REQUIRED");
        assert!(progress.message.contains("main window"));
        assert_eq!(progress.progress_token, Some(json!("progress-1")));
    }

    #[test]
    fn privileged_prompt_errors_preserve_stable_codes_and_retry_semantics() {
        assert_eq!(mcp_error_code(SUDO_PASSWORD_NEEDED), SUDO_PASSWORD_NEEDED);
        assert!(mcp_error_is_retryable(SUDO_PASSWORD_NEEDED));
        assert_eq!(
            mcp_error_code(SUDO_PASSWORD_CANCELLED),
            SUDO_PASSWORD_CANCELLED
        );
        assert!(!mcp_error_is_retryable(SUDO_PASSWORD_CANCELLED));
        assert_eq!(
            mcp_error_code("SSH_CREDENTIALS_NEEDED: enter credentials in FileTerm"),
            "SSH_CREDENTIALS_NEEDED"
        );
        assert!(mcp_error_is_retryable("SSH_CREDENTIALS_NEEDED"));
        assert_eq!(
            mcp_error_code("FILETERM_CONNECTION_OPERATION_NOT_FOUND: missing"),
            "FILETERM_CONNECTION_OPERATION_NOT_FOUND"
        );
        assert_eq!(
            mcp_error_code(NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED),
            NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED
        );
        assert!(mcp_error_is_retryable(
            NETWORK_DEVICE_REMOTE_EXEC_UNSUPPORTED
        ));
        assert_eq!(
            mcp_error_code(VISIBLE_TERMINAL_SESSION_NOT_ACTIVE),
            VISIBLE_TERMINAL_SESSION_NOT_ACTIVE
        );
        assert!(mcp_error_is_retryable(VISIBLE_TERMINAL_SESSION_NOT_ACTIVE));
        assert_eq!(
            mcp_error_code(VISIBLE_TERMINAL_COMMAND_INVALID),
            VISIBLE_TERMINAL_COMMAND_INVALID
        );
        assert!(!mcp_error_is_retryable(VISIBLE_TERMINAL_COMMAND_INVALID));
        assert_eq!(
            mcp_error_code("FILETERM_REMOTE_COMMAND_NOT_FOUND: missing"),
            FILETERM_REMOTE_COMMAND_NOT_FOUND
        );
        assert!(!mcp_error_is_retryable(FILETERM_REMOTE_COMMAND_NOT_FOUND));
        assert_eq!(
            mcp_error_code("FILETERM_REMOTE_COMMAND_SCOPE_MISMATCH: wrong tab"),
            FILETERM_REMOTE_COMMAND_SCOPE_MISMATCH
        );
        assert!(!mcp_error_is_retryable(FILETERM_REMOTE_COMMAND_SCOPE_MISMATCH));
        assert_eq!(
            mcp_error_code("BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED: detached command"),
            BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED
        );
        assert!(!mcp_error_is_retryable(BACKGROUND_REMOTE_SAVE_PASSWORD_UNSUPPORTED));
        assert!(mcp_error_is_retryable(FILETERM_REMOTE_COMMAND_LIMIT));
    }

    #[test]
    fn network_device_command_errors_preserve_stable_codes() {
        assert_eq!(
            mcp_error_code(NETWORK_DEVICE_CWD_UNSUPPORTED),
            NETWORK_DEVICE_CWD_UNSUPPORTED
        );
        assert_eq!(
            mcp_error_code(NETWORK_DEVICE_COMMAND_INVALID),
            NETWORK_DEVICE_COMMAND_INVALID
        );
        assert!(!mcp_error_is_retryable(NETWORK_DEVICE_CWD_UNSUPPORTED));
    }
}
