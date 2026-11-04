use tokio_util::sync::CancellationToken;

use super::StagedReceiveFile;
use crate::sessions::serial::limits::{SerialTransferLimits, TransferBudget};

#[tokio::test]
async fn staged_receive_publishes_only_after_commit() {
    let target = std::env::temp_dir().join(format!("fileterm-staged-{}", uuid::Uuid::new_v4()));
    let mut staged = StagedReceiveFile::create(&target, 1024 * 1024)
        .await
        .unwrap();
    let mut budget = TransferBudget::new(SerialTransferLimits::default());
    budget.begin_file(None).unwrap();
    staged
        .write_all(b"partial", &CancellationToken::new(), Some(&mut budget))
        .await
        .unwrap();
    assert!(!target.exists());
    let committed = staged.commit().await.unwrap();
    assert_eq!(committed, target);
    assert_eq!(tokio::fs::read(&target).await.unwrap(), b"partial");
    let _ = tokio::fs::remove_file(target).await;
}

#[tokio::test]
async fn staged_receive_cleanup_does_not_remove_existing_target() {
    let target =
        std::env::temp_dir().join(format!("fileterm-staged-existing-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&target, b"keep").await.unwrap();
    assert!(StagedReceiveFile::create(&target, 1024).await.is_err());
    assert_eq!(tokio::fs::read(&target).await.unwrap(), b"keep");
    let _ = tokio::fs::remove_file(target).await;
}

#[test]
fn safe_names_reject_paths_and_device_names() {
    for name in ["../escape", "folder\\escape", "CON", "COM1", "report:"] {
        assert!(!super::is_safe_transfer_file_name(name));
    }
    assert!(super::is_safe_transfer_file_name("report.bin"));
}
