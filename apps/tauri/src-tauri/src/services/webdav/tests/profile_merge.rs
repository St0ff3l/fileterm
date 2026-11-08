use super::*;
use serde_json::json;

fn profile(id: &str, name: &str) -> Value {
    json!({"id":id,"type":"ssh","name":name,"host":name,"port":22})
}

#[test]
fn restores_ordered_and_legacy_jumps_with_new_ids_in_both_modes() {
    for mode in [DownloadMode::MergeLocal, DownloadMode::OverwriteLocal] {
        let hop = profile("remote-hop", "hop");
        let mut target = profile("remote-target", "target");
        target["jumpProfileIds"] = json!(["remote-hop"]);
        target["jumpProfileId"] = json!("remote-hop");
        let (saved, _, _) = merge_profiles(&[], vec![target, hop], mode).unwrap();
        assert_ne!(saved[1]["id"], "remote-hop");
        assert_eq!(saved[0]["jumpProfileIds"][0], saved[1]["id"]);
        assert_eq!(saved[0]["jumpProfileId"], saved[1]["id"]);
    }
}

#[test]
fn merge_preserves_existing_identity_and_does_not_rewrite_unrelated_local_jumps() {
    let local_hop = profile("local-hop", "hop");
    let mut local_target = profile("local-target", "local-target");
    local_target["jumpProfileIds"] = json!(["local-hop"]);
    let mut incoming_target = profile("other-target", "target");
    incoming_target["jumpProfileIds"] = json!(["other-hop"]);
    let (saved, added, _) = merge_profiles(
        &[local_hop, local_target],
        vec![incoming_target, profile("other-hop", "hop")],
        DownloadMode::MergeLocal,
    )
    .unwrap();
    assert_eq!(added, 1);
    assert_eq!(saved[2]["jumpProfileIds"], json!(["local-hop"]));
    assert_eq!(saved[1]["jumpProfileIds"], json!(["local-hop"]));
    let mut broken = profile("x", "x");
    broken["jumpProfileId"] = json!("missing");
    assert!(merge_profiles(&[], vec![broken], DownloadMode::OverwriteLocal).is_err());
}

#[test]
fn foreign_id_collisions_do_not_rebind_inherited_local_jump_routes() {
    let hop = profile("collision", "local-hop");
    let mut target = profile("local-target", "target");
    target["jumpProfileIds"] = json!(["collision"]);
    let foreign = profile("collision", "other-server");
    let incoming_target = profile("foreign-target", "target");
    let (saved, _, _) = merge_profiles(
        &[hop, target],
        vec![foreign, incoming_target],
        DownloadMode::MergeLocal,
    )
    .unwrap();
    assert_eq!(saved[1]["jumpProfileIds"], json!(["collision"]));
    assert_ne!(saved[2]["id"], "collision");
}

#[test]
fn restoration_preserves_distinct_profiles_with_the_same_endpoint() {
    for mode in [DownloadMode::MergeLocal, DownloadMode::OverwriteLocal] {
        let mut hop_a = profile("hop-a", "shared-server");
        let mut hop_b = profile("hop-b", "shared-server");
        hop_a["password"] = json!("password-a");
        hop_b["password"] = json!("password-b");
        let mut target_a = profile("target-a", "target-a");
        let mut target_b = profile("target-b", "target-b");
        target_a["jumpProfileIds"] = json!(["hop-a"]);
        target_b["jumpProfileIds"] = json!(["hop-b"]);
        let (saved, imported, _) =
            merge_profiles(&[], vec![hop_a, hop_b, target_a, target_b], mode).unwrap();
        assert_eq!(
            saved.len(),
            4,
            "equal endpoint fields must not erase separate saved identities"
        );
        assert_eq!(imported, 4);
        assert_ne!(saved[0]["id"], saved[1]["id"]);
        assert_eq!(saved[0]["password"], "password-a");
        assert_eq!(saved[1]["password"], "password-b");
        assert_eq!(saved[2]["jumpProfileIds"][0], saved[0]["id"]);
        assert_eq!(saved[3]["jumpProfileIds"][0], saved[1]["id"]);
    }
}

#[test]
fn restoration_matches_existing_duplicate_endpoints_by_exact_identity() {
    for mode in [DownloadMode::MergeLocal, DownloadMode::OverwriteLocal] {
        let a = profile("a", "shared");
        let b = profile("b", "shared");
        let mut incoming_a = a.clone();
        let mut incoming_b = b.clone();
        incoming_a["password"] = json!("new-a");
        incoming_b["password"] = json!("new-b");
        let (saved, _, _) = merge_profiles(&[a, b], vec![incoming_b, incoming_a], mode).unwrap();
        assert_eq!(saved.len(), 2);
        assert_eq!(
            saved.iter().find(|p| p["id"] == "a").unwrap()["password"],
            "new-a"
        );
        assert_eq!(
            saved.iter().find(|p| p["id"] == "b").unwrap()["password"],
            "new-b"
        );
    }
}

#[test]
fn ambiguous_endpoint_matches_do_not_overwrite_local_credentials() {
    let mut a = profile("a", "shared");
    let mut b = profile("b", "shared");
    a["password"] = json!("local-a");
    b["password"] = json!("local-b");
    let incoming = profile("foreign", "shared");
    let (saved, imported, updated) =
        merge_profiles(&[a, b], vec![incoming], DownloadMode::MergeLocal).unwrap();
    assert_eq!((saved.len(), imported, updated), (3, 1, 0));
    assert_eq!(saved[0]["password"], "local-a");
    assert_eq!(saved[1]["password"], "local-b");
}

#[test]
fn repeated_cross_device_imports_update_each_duplicate_without_creating_more_profiles() {
    let mut a = profile("remote-a", "shared");
    let mut b = profile("remote-b", "shared");
    a["password"] = json!("a");
    b["password"] = json!("b");
    let original = vec![a.clone(), b.clone()];
    let (existing, _, _) =
        merge_profiles(&[], vec![a.clone(), b.clone()], DownloadMode::MergeLocal).unwrap();
    a["password"] = json!("changed-a");
    let (saved, imported, updated) =
        merge_profiles(&existing, vec![b, a], DownloadMode::MergeLocal).unwrap();
    assert_eq!((saved.len(), imported, updated), (2, 0, 2));
    assert_eq!(saved[0]["id"], existing[0]["id"]);
    assert_eq!(saved[0]["password"], "changed-a");
    assert_eq!(saved[1]["password"], "b");
    let public = profile_ops::strip_secret_fields_public(&saved[0]);
    assert!(public.get(profile_ops::BACKUP_IDENTITY_FIELD).is_none());
    // A local re-export uses new local IDs; the private identity still binds it to the cloud record.
    let (cloud, imported, updated) =
        merge_profiles(&original, saved, DownloadMode::MergeLocal).unwrap();
    assert_eq!((cloud.len(), imported, updated), (2, 0, 2));
    assert_eq!(cloud[0]["id"], "remote-a");
    assert_eq!(cloud[0]["password"], "changed-a");
}

#[test]
fn divergent_endpoints_with_a_shared_backup_origin_remain_restorable() {
    let mut local = profile("local", "new-endpoint");
    local[profile_ops::BACKUP_IDENTITY_FIELD] = json!("origin");
    let incoming = profile("origin", "old-endpoint");
    let (merged, _, _) =
        merge_profiles(&[local], vec![incoming], DownloadMode::MergeLocal).unwrap();
    assert_eq!(merged.len(), 2);
    let (restored, _, _) = merge_profiles(&[], merged, DownloadMode::OverwriteLocal).unwrap();
    assert_eq!(restored.len(), 2);
}
