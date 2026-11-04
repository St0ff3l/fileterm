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
fn live_history_releases_completed_manifests_but_retains_recovery_data() {
    let manifest = TransferManifest {
        version: 1,
        directories: vec!["keep-directory".into()],
        files: vec![serde_json::from_value(serde_json::json!({
            "relativePath": "file", "sourcePath": "/source/file",
            "destinationPath": "/target/file", "partialPath": "/target/file.fileterm-part",
            "sourceIdentity": {"size": 10}, "status": "done", "transferredBytes": 10
        }))
        .unwrap()],
    };
    let mut tasks = vec![
        task("active", "running", false, false),
        task("paused", "paused", true, false),
        task("retry", "failed", true, false),
        task("cleanup", "canceled", false, true),
    ];
    for task in &mut tasks {
        task.manifest = Some(manifest.clone());
    }
    for index in 0..5000 {
        let mut completed = task(&format!("done-{index}"), "done", false, false);
        completed.updated_at = Some(index);
        completed.manifest = Some(manifest.clone());
        tasks.push(completed);
        prune_transfer_history(&mut tasks);
        assert!(tasks.len() <= JOURNAL_MAX_TASKS);
    }
    for task in &tasks[..4] {
        assert_eq!(task.manifest.as_ref().unwrap().files.len(), 1);
        assert_eq!(task.manifest.as_ref().unwrap().directories.len(), 1);
    }
    for task in &tasks[4..] {
        let manifest = task.manifest.as_ref().unwrap();
        assert_eq!(manifest.files.capacity(), 0);
        assert_eq!(manifest.directories.capacity(), 0);
    }
    assert_eq!(tasks[4].id, "done-4804");
    assert_eq!(tasks.last().unwrap().id, "done-4999");
    let mut protected = (0..250)
        .map(|index| task(&index.to_string(), "paused", true, false))
        .collect();
    prune_transfer_history(&mut protected);
    assert_eq!(protected.len(), 250);
    let mut oversized = Vec::with_capacity(50_000);
    oversized.push(task("done", "done", false, false));
    prune_transfer_history(&mut oversized);
    assert!(oversized.capacity() <= JOURNAL_MAX_TASKS * 2);
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
            version: 1,
            transfers: vec![task("legacy", "paused", true, false)],
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(read_journal_at(&path, &backup).unwrap()[0].id, "legacy");
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
