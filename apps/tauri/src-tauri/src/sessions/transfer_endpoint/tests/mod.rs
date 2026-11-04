use super::*;
#[test]
fn identity_tracks_resolved_route_but_not_password_or_label_edits() {
    let old = json!({"type":"ssh", "host":"a", "username":"alice", "password":"secret"});
    let original = fingerprint(std::slice::from_ref(&old));
    let mut edited = old.clone();
    edited["password"] = json!("different");
    edited["name"] = json!("new name");
    assert_eq!(original, fingerprint(std::slice::from_ref(&edited)));
    edited["host"] = json!("b");
    assert_ne!(original, fingerprint(std::slice::from_ref(&edited)));
    assert_ne!(original, fingerprint(&[json!({"host":"jump"}), old]));
    assert!(!original.contains("secret"));
}
