use super::*;
#[test]
fn rejects_relative_parent_and_control_paths_before_protocol_io() {
    for path in [
        "",
        "relative/file",
        "/root/../etc",
        "/name\r\nDELE file",
        "/bad\0path",
    ] {
        assert!(validate_copy_path(path).is_err(), "{path:?}");
    }
    for path in ["/", "/root/中文 文件", "/home/.hidden"] {
        assert!(validate_copy_path(path).is_ok());
    }
}
#[test]
fn copy_journal_keeps_both_endpoints_and_owned_checkpoints() {
    let task: TransferTask = serde_json::from_value(serde_json::json!({
        "id":"transfer-copy", "direction":"copy", "name":"data", "progress":0.0, "status":"paused", "resumable":true,
        "tabId":"target-tab", "profileId":"target-profile", "copySource": {
            "tabId":"source-tab", "profileId":"source-profile", "name":"Source", "targetName":"Target", "sourceEndpoint":"source", "targetEndpoint":"target", "replaceExisting":false
        }, "manifest": {"version":1, "directories":[], "files":[{
            "relativePath":"data", "sourcePath":"/a/data", "destinationPath":"/b/data", "partialPath":"/b/data.transfer-copy.fileterm-part", "sourceIdentity":{"size":3}, "status":"pending", "transferredBytes":0
        }]}
    })).unwrap();
    let mut reserved = task.clone();
    reserved.destination_path = Some("/target/data".into());
    let mut next = reserved.clone();
    next.destination_path = Some("/target/data/child".into());
    assert!(ensure_copy_target_is_free(std::slice::from_ref(&reserved), &next).is_err());
    next.direction = "upload".into();
    assert!(ensure_copy_target_is_free(std::slice::from_ref(&reserved), &next).is_err());
    next.profile_id = Some("other-server".into());
    assert!(ensure_copy_target_is_free(std::slice::from_ref(&reserved), &next).is_ok());
    next.profile_id = reserved.profile_id.clone();
    reserved.resumable = false;
    reserved.status = "done".into();
    assert!(ensure_copy_target_is_free(std::slice::from_ref(&reserved), &next).is_ok());
    let restored: TransferTask =
        serde_json::from_slice(&serde_json::to_vec(&task).unwrap()).unwrap();
    assert_eq!(
        restored.copy_source.as_ref().unwrap().profile_id,
        "source-profile"
    );
    assert_eq!(restored.profile_id.as_deref(), Some("target-profile"));
    assert!(restored.manifest.unwrap().files[0]
        .partial_path
        .contains("transfer-copy"));
    assert_eq!(task.to_ui_task().copy_source.unwrap().tab_id, "source-tab");
    assert!(task.to_ui_task().manifest.unwrap().files.is_empty());
}
