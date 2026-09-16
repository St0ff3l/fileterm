use super::*;

fn upload_command(
    respond_to: oneshot::Sender<Result<(), String>>,
    cancel: CancellationToken,
) -> WorkerCmd {
    WorkerCmd::UploadLocalFile {
        local_path: "fixture-source".into(),
        remote_path: "fixture-target".into(),
        resume_offset: 0,
        transfer_id: "fixture-transfer".into(),
        cancel,
        verify_checksum: false,
        respond_to,
    }
}

#[tokio::test]
async fn operation_timeout_cancels_worker_but_allows_run_failure_handling() {
    let parent = CancellationToken::new();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let task_parent = parent.clone();
    let call = tokio::spawn(async move {
        dispatch_worker_command(
            sender,
            Duration::from_millis(50),
            Some(task_parent),
            upload_command,
        )
        .await
    });
    let WorkerCmd::UploadLocalFile {
        cancel,
        respond_to: _response,
        ..
    } = receiver.recv().await.unwrap()
    else {
        panic!("unexpected command")
    };
    let error = call.await.unwrap().unwrap_err();
    assert!(error.to_string().contains("超时"));
    assert!(cancel.is_cancelled());
    assert!(
        !parent.is_cancelled(),
        "run must handle this as failure, not user cancellation"
    );
}

#[tokio::test]
async fn user_cancellation_still_reaches_in_flight_worker() {
    let parent = CancellationToken::new();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let task_parent = parent.clone();
    let call = tokio::spawn(async move {
        dispatch_worker_command(
            sender,
            Duration::from_secs(10),
            Some(task_parent),
            upload_command,
        )
        .await
    });
    let WorkerCmd::UploadLocalFile {
        cancel,
        respond_to: _response,
        ..
    } = receiver.recv().await.unwrap()
    else {
        panic!("unexpected command")
    };
    parent.cancel();
    let error = tokio::time::timeout(Duration::from_secs(2), call)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("已取消"));
    assert!(cancel.is_cancelled());
}

#[tokio::test]
async fn dropping_request_cancels_worker_without_canceling_parent() {
    let parent = CancellationToken::new();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let task_parent = parent.clone();
    let call = tokio::spawn(async move {
        dispatch_worker_command(
            sender,
            Duration::from_secs(10),
            Some(task_parent),
            upload_command,
        )
        .await
    });
    let WorkerCmd::UploadLocalFile {
        cancel,
        respond_to: _response,
        ..
    } = receiver.recv().await.unwrap()
    else {
        panic!("unexpected command")
    };
    call.abort();
    assert!(call.await.unwrap_err().is_cancelled());
    assert!(cancel.is_cancelled());
    assert!(!parent.is_cancelled());
}

#[tokio::test]
async fn already_canceled_request_never_enters_worker_queue() {
    let parent = CancellationToken::new();
    parent.cancel();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    assert!(
        dispatch_worker_command(sender, Duration::from_secs(1), Some(parent), upload_command)
            .await
            .is_err()
    );
    assert!(receiver.try_recv().is_err());
}
