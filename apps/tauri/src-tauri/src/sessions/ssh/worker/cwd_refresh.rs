#[derive(Clone)]
struct CwdRefreshRequest {
    cwd: String,
    silent: bool,
    sftp: SharedSftpSession,
    file_access_mode: String,
    root_file_access_method: RootFileAccessMethod,
    sudo_user: Option<String>,
    sudo_password: Option<String>,
}

struct CwdRefreshQueue {
    pending: Arc<std::sync::Mutex<Option<CwdRefreshRequest>>>,
    wake: mpsc::Sender<()>,
    followed: Arc<std::sync::Mutex<Option<FollowedCwd>>>,
}

// Cache only confirmed mappings. Shell and SFTP paths may have different roots.
struct FollowedCwd {
    cwd: String,
    remote_path: String,
    mode: String,
    user: Option<String>,
}

impl FollowedCwd {
    fn needs_realign(
        &self,
        cwd: &str,
        remote_path: &str,
        mode: &str,
        user: &Option<String>,
    ) -> bool {
        self.cwd == cwd
            && self.mode == mode
            && &self.user == user
            && self.remote_path != remote_path
    }
}

impl CwdRefreshQueue {
    fn needs_realign(
        &self,
        cwd: &str,
        remote_path: &str,
        mode: &str,
        user: &Option<String>,
    ) -> bool {
        self.followed
            .lock()
            .expect("CWD mapping cache poisoned")
            .as_ref()
            .is_some_and(|last| last.needs_realign(cwd, remote_path, mode, user))
    }

    fn enqueue(&self, mut request: CwdRefreshRequest) {
        let mut pending = self.pending.lock().expect("CWD refresh queue poisoned");
        if let Some(previous) = pending.as_ref() {
            // A quiet command after cd/startup must not erase pending navigation.
            if previous.cwd == request.cwd
                && previous.file_access_mode == request.file_access_mode
                && previous.root_file_access_method == request.root_file_access_method
                && previous.sudo_user == request.sudo_user
            {
                request.silent &= previous.silent;
            }
        }
        *pending = Some(request);
        drop(pending);
        // Capacity one: a wake already pending also covers the latest request.
        let _ = self.wake.try_send(());
    }
}

/// At most one listing is active and one latest change is pending. Requests
/// come from startup, changed CWD markers, or file-access transitions.
fn start_cwd_refresh(
    app: AppHandle,
    tab_id: String,
    handle: Arc<Handle<ClientHandler>>,
    operation_timeout: Duration,
    cancellation: CancellationToken,
) -> CwdRefreshQueue {
    let pending = Arc::new(std::sync::Mutex::new(None::<CwdRefreshRequest>));
    let followed = Arc::new(std::sync::Mutex::new(None));
    let (wake, mut receiver) = mpsc::channel(1);
    let sender = CwdRefreshQueue {
        pending: Arc::clone(&pending),
        wake,
        followed: Arc::clone(&followed),
    };
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => return,
                signal = receiver.recv() => if signal.is_none() { return },
            }
            // Debounce bursts of prompt redraws while retaining the last one.
            tokio::select! {
                _ = cancellation.cancelled() => return,
                _ = tokio::time::sleep(Duration::from_millis(200)) => {},
            }
            let Some(request) = pending.lock().expect("CWD refresh queue poisoned").take() else {
                continue;
            };
            let mut mapping = FollowedCwd {
                cwd: request.cwd.clone(),
                remote_path: String::new(),
                mode: request.file_access_mode.clone(),
                user: request.sudo_user.clone(),
            };
            let applied_path = tokio::select! {
                _ = cancellation.cancelled() => return,
                result = follow_shell_cwd(
                    app.clone(), tab_id.clone(), request.cwd, request.sftp,
                    Arc::clone(&handle), operation_timeout, request.file_access_mode,
                    request.root_file_access_method, request.sudo_user, request.sudo_password,
                    request.silent,
                ) => result,
            };
            if let Some(path) = applied_path {
                mapping.remote_path = path;
                *followed.lock().expect("CWD mapping cache poisoned") = Some(mapping);
            }
        }
    });
    sender
}

#[cfg(test)]
mod shell_identity_regression_tests {
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
}

#[cfg(test)]
mod followed_cwd_tests {
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
}
