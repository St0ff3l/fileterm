use super::*;

#[test]
fn target_edits_are_isolated_but_renames_and_passwords_do_not_split_collectors() {
    let mut profile = serde_json::json!({"id": "profile", "name": "Before", "password": "old"});
    let original = monitoring_target_key(&profile, "host-a", 22, "alice");
    assert_ne!(
        original,
        monitoring_target_key(&profile, "host-b", 22, "alice")
    );
    assert_ne!(
        original,
        monitoring_target_key(&profile, "host-a", 2222, "alice")
    );
    assert_ne!(
        original,
        monitoring_target_key(&profile, "host-a", 22, "bob")
    );
    profile["name"] = "After".into();
    profile["password"] = "new".into();
    assert_eq!(
        original,
        monitoring_target_key(&profile, "host-a", 22, "alice")
    );
    profile["proxy"] = serde_json::json!({"type": "socks5", "host": "proxy", "port": 1080});
    assert_ne!(
        original,
        monitoring_target_key(&profile, "host-a", 22, "alice")
    );
}
