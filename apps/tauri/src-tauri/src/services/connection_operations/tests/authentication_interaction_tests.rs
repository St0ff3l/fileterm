use super::*;

#[tokio::test]
async fn credential_interactions_wake_waiters_without_implying_authentication_success() {
    let registry = ConnectionOperationRegistry::default();
    let operation = registry.begin("profile").await;
    registry.attach_tab(&operation.id, "tab").await.unwrap();
    let mut info = registry.info(&operation.id).await.unwrap();
    registry
        .request_interaction("tab", "request", "keyboard-interactive")
        .await;
    info.receiver.changed().await.unwrap();
    let info = registry.info(&operation.id).await.unwrap();
    assert_eq!(info.interactions[0].kind, "keyboard-interactive");
    assert_eq!(info.interactions[0].status, "requested");
    registry.resolve_interaction("request", false).await;
    let info = registry.info(&operation.id).await.unwrap();
    assert_eq!(info.interactions[0].status, "input-completed");
    assert_eq!(
        *info.receiver.borrow(),
        ConnectionOperationState::Connecting
    );
    registry
        .publish_for_tab(
            "tab",
            ConnectionOperationState::Failed {
                code: SSH_AUTH_FAILURE.to_string(),
            },
        )
        .await;
    assert_eq!(
        registry.info(&operation.id).await.unwrap().interactions[0].status,
        "input-completed"
    );
}

#[tokio::test]
async fn interaction_history_is_bounded_with_an_explicit_truncation_marker() {
    let registry = ConnectionOperationRegistry::default();
    let operation = registry.begin("profile").await;
    registry.attach_tab(&operation.id, "tab").await.unwrap();
    for index in 0..40 {
        registry
            .request_interaction("tab", &index.to_string(), "credentials")
            .await;
    }
    registry
        .request_interaction("tab", "ignored", "secret-kind")
        .await;
    let info = registry.info(&operation.id).await.unwrap();
    assert_eq!(info.interactions.len(), 17);
    assert_eq!(
        info.interactions.last().unwrap().status,
        "history-truncated"
    );
}
