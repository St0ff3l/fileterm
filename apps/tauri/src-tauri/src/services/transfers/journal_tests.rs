use super::*;

fn task(id: &str, status: &str, resumable: bool, cleanup_pending: bool) -> TransferTask {
    serde_json::from_value(serde_json::json!({
        "id": id, "direction": "download", "name": id, "progress": 0,
        "status": status, "resumable": resumable, "cleanupPending": cleanup_pending
    }))
    .unwrap()
}

#[test]
fn journal_history_limit_never_evicts_unfinished_or_recoverable_work() {
    let mut tasks = vec![
        task("active", "running", false, false),
        task("paused", "paused", true, false),
        task("retry", "failed", true, false),
        task("cleanup", "canceled", false, true),
    ];
    for index in 0..JOURNAL_MAX_TASKS + 1 {
        let mut completed = task(&format!("done-{index}"), "done", false, false);
        completed.updated_at = Some(1000 + index as u64);
        tasks.push(completed);
    }
    let kept = select_journal_tasks(&tasks, JOURNAL_MAX_TASKS);
    assert_eq!(kept.len(), JOURNAL_MAX_TASKS);
    assert_eq!(
        kept[..4]
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        vec!["active", "paused", "retry", "cleanup"]
    );
    let required_only = select_journal_tasks(&tasks, 1);
    assert_eq!(required_only.len(), 4);
}

#[test]
fn journal_missing_corrupt_and_backup_recovery_are_distinct() {
    let root = std::env::temp_dir().join(format!("fileterm-journal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("journal.json");
    let backup = root.join("journal.bak");
    assert!(read_journal_at(&path, &backup).unwrap().is_empty());
    std::fs::write(&path, b"truncated").unwrap();
    assert!(read_journal_at(&path, &backup).is_err());
    std::fs::write(&backup, b"also truncated").unwrap();
    assert!(read_journal_at(&path, &backup).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"truncated");
    let journal = TransferJournal {
        version: JOURNAL_VERSION,
        transfers: vec![task("recover", "running", true, false)],
    };
    std::fs::write(&backup, serde_json::to_vec(&journal).unwrap()).unwrap();
    let recovered = read_journal_at(&path, &backup).unwrap();
    assert_eq!(recovered[0].id, "recover");
    assert_eq!(recovered[0].status, "paused");
    std::fs::write(
        &path,
        serde_json::to_vec(&TransferJournal {
            version: JOURNAL_VERSION + 1,
            transfers: vec![],
        })
        .unwrap(),
    )
    .unwrap();
    assert!(read_journal_at(&path, &backup)
        .unwrap_err()
        .to_string()
        .contains("版本"));
    std::fs::remove_dir_all(root).unwrap();
}
