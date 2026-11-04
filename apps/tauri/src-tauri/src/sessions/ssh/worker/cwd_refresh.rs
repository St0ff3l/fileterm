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
#[path = "tests/shell_identity_regression_tests.rs"]
mod shell_identity_regression_tests;
#[cfg(test)]
#[path = "tests/followed_cwd_tests.rs"]
mod followed_cwd_tests;
