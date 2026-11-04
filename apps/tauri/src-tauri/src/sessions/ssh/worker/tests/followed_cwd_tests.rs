use super::FollowedCwd;

#[test]
fn empty_enter_only_realigns_a_diverged_confirmed_mapping() {
    let mapping = FollowedCwd {
        cwd: "/home/alice/work".into(),
        remote_path: "/work".into(),
        mode: "user".into(),
        user: None,
    };
    assert!(!mapping.needs_realign("/home/alice/work", "/work", "user", &None));
    assert!(mapping.needs_realign("/home/alice/work", "/", "user", &None));
    assert!(!mapping.needs_realign("/home/alice/other", "/", "user", &None));
    assert!(!mapping.needs_realign("/home/alice/work", "/", "root", &Some("root".into())));
}
