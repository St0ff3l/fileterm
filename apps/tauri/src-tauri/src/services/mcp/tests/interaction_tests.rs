use super::*;

#[test]
fn final_history_only_contains_safe_interaction_fields_for_success_and_error() {
    let mut requested = BridgeProgress::privileged_password_prompt(
        SUDO_PASSWORD_NEEDED,
        Some(json!("private-progress-token")),
    );
    requested.message = "secret-value-that-must-not-reach-agent".to_string();
    let mut unrelated = requested.clone();
    unrelated.event = "terminal-output".to_string();
    unrelated.message = "secret-output".to_string();
    for ok in [true, false] {
        for status in [
            "input-completed",
            "cancelled",
            "timed-out",
            "unavailable",
            "aborted",
        ] {
            let resolved =
                BridgeProgress::privileged_password_state(SUDO_PASSWORD_NEEDED, status, None);
            let response = attach_interaction_history(
                json!({"id": 1, "ok": ok}),
                vec![requested.clone(), unrelated.clone(), resolved],
            );
            assert_eq!(response["interactionHistory"].as_array().unwrap().len(), 2);
            assert_eq!(response["interactionHistory"][1]["status"], status);
            let encoded = response.to_string();
            assert!(!encoded.contains("private-progress-token"));
            assert!(!encoded.contains("secret-value-that-must-not-reach-agent"));
            assert!(!encoded.contains("secret-output"));
            assert!(!encoded.contains("progressToken"));
        }
    }
    requested.event = "action-approval-resolved".to_string();
    requested.status = "approved".to_string();
    let response = attach_interaction_history(json!({}), vec![requested]);
    assert_eq!(response["interactionHistory"][0]["status"], "approved");
}

#[test]
fn legacy_bridge_response_does_not_establish_passwordless_authentication() {
    let response: BridgeResponse =
        serde_json::from_value(json!({"ok": true, "result": {"exitCode": 0}})).unwrap();
    assert!(response.interaction_history.is_empty());
    let result = attach_interaction_history(response.result.unwrap(), Vec::new());
    assert!(result.get("interactionHistory").is_none());
}
