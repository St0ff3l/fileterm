use super::*;
#[tokio::test]
async fn late_local_conflicts_are_not_overwritten_and_legacy_replace_still_works() {
    let root = std::env::temp_dir().join(format!("fileterm-conflict-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.unwrap();
    let partial = root.join("part");
    let destination = root.join("target");
    tokio::fs::write(&partial, b"new").await.unwrap();
    tokio::fs::write(&destination, b"appeared after browsing")
        .await
        .unwrap();
    assert!(
        !commit_local_transfer(&partial, &destination, Some(TransferConflictPolicy::Skip))
            .await
            .unwrap()
    );
    assert_eq!(
        tokio::fs::read(&destination).await.unwrap(),
        b"appeared after browsing"
    );
    assert_eq!(tokio::fs::read(&partial).await.unwrap(), b"new");
    assert!(commit_local_transfer(&partial, &destination, None)
        .await
        .unwrap());
    assert_eq!(tokio::fs::read(&destination).await.unwrap(), b"new");
    tokio::fs::write(&partial, b"first").await.unwrap();
    tokio::fs::remove_file(&destination).await.unwrap();
    assert!(
        commit_local_transfer(&partial, &destination, Some(TransferConflictPolicy::Skip))
            .await
            .unwrap()
    );
    assert!(!partial.exists());
    tokio::fs::remove_dir_all(root).await.unwrap();
}

#[test]
fn saved_profile_edits_do_not_rebind_live_endpoint_but_reconnect_and_legacy_identity_are_rejected(
) {
    let mut profile = serde_json::json!({"host":"old-server", "username":"alice"});
    let connected =
        crate::sessions::transfer_endpoint::fingerprint(std::slice::from_ref(&profile));
    let recorded = connected.clone();
    profile["host"] = serde_json::json!("new-server");
    assert!(ensure_copy_endpoint(&recorded, &connected).is_ok());
    let reconnected =
        crate::sessions::transfer_endpoint::fingerprint(std::slice::from_ref(&profile));
    assert!(ensure_copy_endpoint(&recorded, &reconnected).is_err());
    assert!(
        ensure_copy_endpoint("[\"ssh\",\"old-server\",22,\"alice\",null]", &connected).is_err()
    );
}

#[test]
fn explicit_policy_persists_and_owns_unique_directory_checkpoints_without_changing_legacy_tasks(
) {
    let value = serde_json::json!({"id":"one", "direction":"upload", "name":"x", "progress":0, "status":"queued", "fileAccessMode":"user", "destinationPath":"/x", "partialPath":"/x.fileterm-part", "manifest":{"version":1,"directories":[],"files":[{"relativePath":"a","sourcePath":"/source/a","destinationPath":"/x/a","partialPath":"/x/a.fileterm-part","sourceIdentity":{"size":1},"status":"pending","transferredBytes":0}]}});
    let mut task: TransferTask = serde_json::from_value(value).unwrap();
    let legacy = serde_json::to_value(&task).unwrap();
    prepare_conflict_task(&mut task, None).unwrap();
    assert_eq!(legacy, serde_json::to_value(&task).unwrap());
    prepare_conflict_task(&mut task, Some(TransferConflictPolicy::Skip)).unwrap();
    assert_eq!(
        task.manifest.as_ref().unwrap().files[0].partial_path,
        "/x/a.one.fileterm-part"
    );
    let restored: TransferTask =
        serde_json::from_slice(&serde_json::to_vec(&task).unwrap()).unwrap();
    assert_eq!(restored.conflict_policy, Some(TransferConflictPolicy::Skip));
    assert_eq!(task.to_ui_task().conflict_policy, restored.conflict_policy);
    task.file_access_mode = Some("root".into());
    assert!(prepare_conflict_task(&mut task, Some(TransferConflictPolicy::Skip)).is_err());
}
