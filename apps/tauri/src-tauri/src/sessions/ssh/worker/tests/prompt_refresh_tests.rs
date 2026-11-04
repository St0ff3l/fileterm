use super::{apply_followed_remote_listing, replace_changed_remote_files, ShellPromptRefresh};

#[test]
fn completed_same_cwd_command_realigns_manually_browsed_parent() {
    let mut tracker = ShellPromptRefresh::default();
    tracker.observe_output("\x1b]7777;FileTermCommandTracking=1\x07");
    // The shell stays in /home/stoffel/work while the pane browses its parent.
    let mut path = "/home/stoffel".to_owned();
    let mut files = Vec::new();
    assert_eq!(
        tracker.take_prompt(
            Some("/home/stoffel/work".into()),
            Some("/home/stoffel/work"),
            true
        ),
        None
    );
    tracker.observe_output("\x1b]7777;FileTermCommand\x07");
    let (cwd, silent) = tracker
        .take_prompt(
            Some("/home/stoffel/work".into()),
            Some("/home/stoffel/work"),
            true,
        )
        .unwrap();
    assert!(silent);
    // Both directories can be empty: the changed path must still publish.
    assert!(apply_followed_remote_listing(
        &mut path,
        &mut files,
        &cwd,
        &[]
    ));
    assert_eq!(path, "/home/stoffel/work");
    assert!(!apply_followed_remote_listing(
        &mut path,
        &mut files,
        &cwd,
        &[]
    ));
}

#[test]
fn quiet_follow_uses_resolved_sftp_namespace() {
    let mut path = "/".to_owned();
    let mut files = Vec::new();
    // Shell /home/stoffel/work resolves to /work in a home-chroot SFTP.
    assert!(apply_followed_remote_listing(
        &mut path,
        &mut files,
        "/work",
        &[]
    ));
    assert_eq!(path, "/work");
    assert!(!apply_followed_remote_listing(
        &mut path,
        &mut files,
        "/work",
        &[]
    ));
}

#[test]
fn unchanged_listings_do_not_publish_but_file_and_permission_changes_do() {
    let a = serde_json::json!({"path":"/a", "permission":"rw-r--r--", "size":"1 B"});
    let b = serde_json::json!({"path":"/b", "permission":"rw-r--r--", "size":"1 B"});
    let mut files = vec![a.clone()];
    assert!(!replace_changed_remote_files(
        &mut files,
        std::slice::from_ref(&a)
    ));
    assert!(replace_changed_remote_files(&mut files, &[a.clone(), b]));
    assert!(replace_changed_remote_files(
        &mut files,
        std::slice::from_ref(&a)
    ));
    let mut changed = a;
    changed["permission"] = "rwxr-xr-x".into();
    assert!(replace_changed_remote_files(
        &mut files,
        std::slice::from_ref(&changed)
    ));
    assert!(!replace_changed_remote_files(&mut files, &[changed]));
}

#[test]
fn empty_prompts_do_not_refresh_but_completed_commands_do() {
    let mut tracker = ShellPromptRefresh::default();
    tracker.observe_output("\x1b]7777;FileTermCommandTracking=1\x07");
    assert_eq!(
        tracker.take_prompt(Some("/a".into()), None, true),
        Some(("/a".into(), false))
    );
    for _ in 0..3 {
        assert_eq!(
            tracker.take_prompt(Some("/a".into()), Some("/a"), true),
            None
        );
    }
    tracker.observe_output("\x1b]7777;FileTermCommand\x07command output");
    assert_eq!(tracker.take_prompt(None, Some("/a"), false), None);
    assert_eq!(
        tracker.take_prompt(Some("/a".into()), Some("/a"), true),
        Some(("/a".into(), true))
    );
    assert_eq!(
        tracker.take_prompt(Some("/a".into()), Some("/a"), true),
        None
    );
}

#[test]
fn command_markers_survive_every_packet_boundary_and_both_terminators() {
    for ending in ["\x07", "\x1b\\"] {
        let markers = format!(
            "\x1b]7777;FileTermCommandTracking=1{ending}\x1b]7777;FileTermCommand{ending}"
        );
        for split in 0..=markers.len() {
            let mut tracker = ShellPromptRefresh::default();
            tracker.observe_output(&markers[..split]);
            tracker.observe_output(&markers[split..]);
            assert_eq!(
                tracker.take_prompt(Some("/a".into()), Some("/a"), false),
                None
            );
            assert_eq!(
                tracker.take_prompt(None, Some("/a"), true),
                Some(("/a".into(), true))
            );
            assert_eq!(
                tracker.take_prompt(Some("/a".into()), Some("/a"), true),
                None
            );
        }
    }
}

#[test]
fn changed_cwd_waits_for_identity_and_unsupported_shells_refresh_silently() {
    let mut tracker = ShellPromptRefresh::default();
    tracker.observe_output("\x1b]7777;FileTermCommandTracking=1\x07");
    assert_eq!(
        tracker.take_prompt(Some("/b".into()), Some("/a"), false),
        None
    );
    assert_eq!(
        tracker.take_prompt(None, Some("/b"), true),
        Some(("/b".into(), false))
    );
    tracker.observe_output("\x1b]7777;FileTermCommandTracking=0\x07");
    assert_eq!(
        tracker.take_prompt(Some("/b".into()), Some("/b"), true),
        Some(("/b".into(), true))
    );
}
