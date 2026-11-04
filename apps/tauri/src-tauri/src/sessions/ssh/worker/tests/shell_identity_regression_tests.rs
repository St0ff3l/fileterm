use super::{observe_shell_user, resolve_shell_file_access, track_cwd_and_user};

#[test]
fn container_root_is_the_login_identity_and_later_su_still_switches_files() {
    let mut login = None;
    let mut current = None;
    let mut buffer = String::new();
    let (cwd, user) = track_cwd_and_user(
        "\x1b]7;file://pod/work\x07\x1b]1337;RemoteUser=root\x07",
        &mut buffer,
    );
    assert_eq!(cwd.as_deref(), Some("/work"));
    assert!(observe_shell_user(
        &mut login,
        &mut current,
        user.as_deref().unwrap()
    ));
    assert_eq!(
        resolve_shell_file_access(login.as_deref().unwrap(), current.as_deref().unwrap()),
        ("user", None)
    );
    assert!(observe_shell_user(&mut login, &mut current, "postgres"));
    assert_eq!(
        resolve_shell_file_access(login.as_deref().unwrap(), "postgres"),
        ("root", Some("postgres".to_string()))
    );
    assert!(observe_shell_user(&mut login, &mut current, "root"));
    assert_eq!(
        resolve_shell_file_access(login.as_deref().unwrap(), "root"),
        ("user", None)
    );
}

#[test]
fn normal_sudo_exit_preserves_original_login_and_same_directory_markers() {
    let mut login = None;
    let mut current = None;
    observe_shell_user(&mut login, &mut current, "alice");
    observe_shell_user(&mut login, &mut current, "root");
    assert_eq!(
        resolve_shell_file_access(login.as_deref().unwrap(), "root"),
        ("root", Some("root".to_string()))
    );
    observe_shell_user(&mut login, &mut current, "alice");
    assert_eq!(
        resolve_shell_file_access(login.as_deref().unwrap(), "alice"),
        ("user", None)
    );
    let mut buffer = String::new();
    for _ in 0..2 {
        let (cwd, _) = track_cwd_and_user("\x1b]7;file://host/work\x07", &mut buffer);
        assert_eq!(cwd.as_deref(), Some("/work"));
        let (_, user) = track_cwd_and_user("\x1b]1337;RemoteUser=alice\x07", &mut buffer);
        assert_eq!(user.as_deref(), Some("alice"));
    }
    assert_eq!(
        track_cwd_and_user("ordinary command output", &mut buffer),
        (None, None)
    );
}
