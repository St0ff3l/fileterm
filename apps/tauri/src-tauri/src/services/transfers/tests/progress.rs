#[test]
fn late_progress_and_checksum_updates_do_not_revive_stopped_tasks() {
    for status in [
        "queued",
        "paused",
        "interrupted",
        "canceled",
        "failed",
        "done",
    ] {
        let mut task = task_with_status(status);
        task.message = Some("preserve me".into());
        task.progress = 75.0;
        task.speed = Some("old speed".into());
        let before = serde_json::to_value(&task).unwrap();
        super::apply_transfer_progress(&mut task, 90, 100, Some("new speed".into()));
        assert!(!super::apply_sha256_verifying(&mut task));
        assert_eq!(serde_json::to_value(&task).unwrap(), before, "{status}");
    }
}

#[test]
fn progress_preserves_checksum_and_commit_status_and_messages() {
    for status in ["verifying", "finalizing"] {
        let mut task = task_with_status(status);
        task.message = Some("current stage".into());
        super::apply_transfer_progress(&mut task, 100, 100, Some("new speed".into()));
        assert_eq!(task.status, status);
        assert_eq!(task.message.as_deref(), Some("current stage"));
        assert!(task.speed.is_none());
        assert_eq!(task.transferred_bytes, Some(100));
        assert_eq!(task.progress, 99.0);
        if status == "finalizing" {
            assert!(!super::apply_sha256_verifying(&mut task));
            assert_eq!(task.status, "finalizing");
        }
    }
}

#[test]
fn running_transfer_enters_checksum_state_without_losing_progress() {
    let mut task = task_with_status("running");
    super::apply_transfer_progress(&mut task, 90, 100, Some("10 MiB/s".into()));
    assert_eq!(task.speed.as_deref(), Some("10 MiB/s"));
    assert!(super::apply_sha256_verifying(&mut task));
    assert_eq!(task.status, "verifying");
    assert!(task.message.as_deref().unwrap().contains("SHA-256"));
    assert!(task.speed.is_none());
    assert_eq!(task.progress, 90.0);
}
