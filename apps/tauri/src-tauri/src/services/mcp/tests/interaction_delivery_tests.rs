use super::*;

#[test]
fn queued_final_event_survives_dispatch_completion_race() {
    let (sender, mut receiver) = mpsc::channel(4);
    let requested = BridgeProgress::privileged_password_prompt(SUDO_PASSWORD_NEEDED, None);
    let completed = BridgeProgress::privileged_password_state(
        SUDO_PASSWORD_NEEDED,
        "input-completed",
        None,
    );
    sender.try_send(completed).unwrap();
    let response = finalize_bridge_interactions(
        BridgeResponse::success(json!({})),
        vec![requested],
        &mut receiver,
    );
    assert_eq!(response.interaction_history.len(), 2);
    assert_eq!(response.interaction_history[1].status, "input-completed");
}

#[test]
fn every_tool_output_schema_allows_only_sanitized_interaction_fields() {
    for tool in tool_definitions() {
        let schema = &tool["outputSchema"]["properties"]["interactionHistory"];
        assert_eq!(schema["type"], "array", "{}", tool["name"]);
        assert_eq!(schema["items"]["additionalProperties"], false);
        assert_eq!(schema["items"]["properties"].as_object().unwrap().len(), 3);
    }
}

#[test]
fn storage_and_response_errors_do_not_imply_disconnection_or_no_side_effects() {
    let storage = public_app_error(AppError::Storage("secret-path".to_string()));
    assert!(!storage.contains("secret-path"));
    assert_eq!(
        mcp_error_code(&storage),
        "FILETERM_STORAGE_OPERATION_FAILED"
    );
    let response = public_app_error(AppError::Serialization("secret-value".to_string()));
    assert!(!response.contains("secret-value"));
    for error in [&storage, &response, FILETERM_CLI_JSONL_REQUEST_CANCELLED] {
        let metadata =
            agent_error_metadata("write_remote_file", &json!({"tab_id": "tab-1"}), error);
        assert_eq!(metadata["outcome"], "unknown");
        assert_eq!(metadata["safeToRetry"], false);
        assert_eq!(metadata["recovery"]["type"], "inspect-before-retry");
    }
    assert_eq!(
        mcp_error_code(FILETERM_CONNECTION_FAILED),
        FILETERM_CONNECTION_FAILED
    );
}

#[test]
fn ssh_prompt_metadata_never_contains_prompts_answers_or_identifiers() {
    for kind in [
        "credentials",
        "host-verification",
        "key-passphrase",
        "keyboard-interactive",
    ] {
        let result = attach_interaction_history(
            json!({}),
            vec![BridgeProgress::ssh_interaction(
                kind,
                "input-completed",
                Some(json!("internal-id")),
            )],
        );
        let encoded = result.to_string();
        assert!(!encoded.contains("internal-id"));
        assert!(!encoded.contains("message"));
        assert_eq!(result["interactionHistory"][0]["status"], "input-completed");
    }
}
