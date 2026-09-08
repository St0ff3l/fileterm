/// Pair each prompt's CWD with its identity marker, even if the path did not
/// change. Identity must be applied before a privileged directory read starts.
fn take_prompt_cwd(
    cwd: Option<String>,
    has_user: bool,
    pending: &mut Option<String>,
) -> Option<String> {
    if has_user {
        let previous = pending.take();
        cwd.or(previous)
    } else {
        if cwd.is_some() {
            *pending = cwd;
        }
        None
    }
}

#[derive(Clone)]
struct CwdRefreshRequest {
    cwd: String,
    sftp: SharedSftpSession,
    file_access_mode: String,
    root_file_access_method: RootFileAccessMethod,
    sudo_user: Option<String>,
    sudo_password: Option<String>,
}

/// At most one listing is active and one latest prompt is pending. Continuous
/// output does not trigger refresh; only runtime-decoded shell markers do.
fn start_cwd_refresh(
    app: AppHandle,
    tab_id: String,
    handle: Arc<Handle<ClientHandler>>,
    operation_timeout: Duration,
    cancellation: CancellationToken,
) -> tokio::sync::watch::Sender<Option<CwdRefreshRequest>> {
    let (sender, mut receiver) = tokio::sync::watch::channel::<Option<CwdRefreshRequest>>(None);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => return,
                result = receiver.changed() => if result.is_err() { return },
            }
            // Debounce bursts of prompt redraws while retaining the last one.
            tokio::select! {
                _ = cancellation.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_millis(200)) => {},
            }
            let Some(request) = receiver.borrow_and_update().clone() else {
                continue;
            };
            tokio::select! {
                _ = cancellation.cancelled() => return,
                _ = follow_shell_cwd(
                    app.clone(), tab_id.clone(), request.cwd, request.sftp,
                    Arc::clone(&handle), operation_timeout, request.file_access_mode,
                    request.root_file_access_method, request.sudo_user, request.sudo_password,
                ) => {},
            }
        }
    });
    sender
}

#[cfg(test)]
mod shell_identity_regression_tests {
    use super::{
        observe_shell_user, resolve_shell_file_access, take_prompt_cwd, track_cwd_and_user,
    };

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
        let mut pending = None;
        for _ in 0..2 {
            let (cwd, _) = track_cwd_and_user("\x1b]7;file://host/work\x07", &mut buffer);
            assert_eq!(cwd.as_deref(), Some("/work"));
            assert_eq!(take_prompt_cwd(cwd, false, &mut pending), None);
            let (_, user) = track_cwd_and_user("\x1b]1337;RemoteUser=alice\x07", &mut buffer);
            assert_eq!(user.as_deref(), Some("alice"));
            assert_eq!(
                take_prompt_cwd(None, user.is_some(), &mut pending),
                Some("/work".to_string())
            );
        }
        assert_eq!(take_prompt_cwd(None, false, &mut pending), None);
        assert_eq!(
            take_prompt_cwd(Some("/work".to_string()), true, &mut pending),
            Some("/work".to_string())
        );
        assert_eq!(
            track_cwd_and_user("ordinary command output", &mut buffer),
            (None, None)
        );
    }
}
