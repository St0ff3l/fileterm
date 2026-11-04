use super::{
    ssh_connection_error_code, ConnectionOperationRegistry, ConnectionOperationState,
    FILETERM_CONNECTION_FAILED, SSH_AUTH_FAILURE, SSH_CREDENTIALS_CANCELLED,
    SSH_CREDENTIALS_NEEDED, SSH_CREDENTIALS_TIMEOUT,
};

#[tokio::test]
async fn operation_publishes_connected_state_by_tab() {
    let registry = ConnectionOperationRegistry::default();
    let handle = registry.begin("profile-1").await;
    registry.attach_tab(&handle.id, "tab-1").await.unwrap();
    let mut receiver = registry.info(&handle.id).await.unwrap().receiver;
    registry
        .publish_for_tab("tab-1", ConnectionOperationState::Connected)
        .await;
    receiver.changed().await.unwrap();
    assert_eq!(*receiver.borrow(), ConnectionOperationState::Connected);
}

#[tokio::test]
async fn terminal_operation_state_is_not_overwritten_by_later_disconnect() {
    let registry = ConnectionOperationRegistry::default();
    let handle = registry.begin("profile-1").await;
    registry.attach_tab(&handle.id, "tab-1").await.unwrap();
    let mut receiver = registry.info(&handle.id).await.unwrap().receiver;
    registry
        .publish_for_tab("tab-1", ConnectionOperationState::Connected)
        .await;
    registry
        .fail_for_tab("tab-1", FILETERM_CONNECTION_FAILED)
        .await;
    receiver.changed().await.unwrap();
    assert_eq!(*receiver.borrow(), ConnectionOperationState::Connected);
}

#[tokio::test]
async fn begin_or_join_reuses_one_profile_flight_until_tab_is_forgotten() {
    let registry = ConnectionOperationRegistry::default();
    let (first, first_created) = registry.begin_or_join("profile-1").await;
    let (second, second_created) = registry.begin_or_join("profile-1").await;
    assert!(first_created);
    assert!(!second_created);
    assert_eq!(first.id, second.id);

    registry.attach_tab(&first.id, "tab-1").await.unwrap();
    registry
        .publish_for_tab("tab-1", ConnectionOperationState::Connected)
        .await;
    let (connected, connected_created) = registry.begin_or_join("profile-1").await;
    assert!(!connected_created);
    assert_eq!(connected.id, first.id);

    registry.forget_tab("tab-1").await;
    let (after_close, after_close_created) = registry.begin_or_join("profile-1").await;
    assert!(after_close_created);
    assert_ne!(after_close.id, first.id);
}

#[tokio::test]
async fn failed_profile_flight_can_be_retried() {
    let registry = ConnectionOperationRegistry::default();
    let (first, first_created) = registry.begin_or_join("profile-1").await;
    assert!(first_created);
    registry
        .fail_for_operation(&first.id, FILETERM_CONNECTION_FAILED)
        .await;

    let (retry, retry_created) = registry.begin_or_join("profile-1").await;
    assert!(retry_created);
    assert_ne!(retry.id, first.id);
}

#[tokio::test]
async fn forgetting_pending_tab_notifies_waiters_of_failure() {
    let registry = ConnectionOperationRegistry::default();
    let (handle, _) = registry.begin_or_join("profile-1").await;
    registry.attach_tab(&handle.id, "tab-1").await.unwrap();
    let mut receiver = registry.info(&handle.id).await.unwrap().receiver;

    registry.forget_tab("tab-1").await;
    receiver.changed().await.unwrap();
    assert_eq!(
        *receiver.borrow(),
        ConnectionOperationState::Failed {
            code: FILETERM_CONNECTION_FAILED.to_string()
        }
    );
}

#[test]
fn ssh_errors_have_stable_non_secret_codes() {
    assert_eq!(
        ssh_connection_error_code("SSH credentials request canceled"),
        SSH_CREDENTIALS_CANCELLED
    );
    assert_eq!(
        ssh_connection_error_code("SSH credentials request timed out"),
        SSH_CREDENTIALS_TIMEOUT
    );
    assert_eq!(
        ssh_connection_error_code("SSH username and password are required"),
        SSH_CREDENTIALS_NEEDED
    );
    assert_eq!(
        ssh_connection_error_code("Authentication failed"),
        SSH_AUTH_FAILURE
    );
    assert_eq!(
        ssh_connection_error_code("connection refused"),
        FILETERM_CONNECTION_FAILED
    );
}
