/// Negotiate the optional SFTP channel and publish its initial directory state.
///
/// SFTP is deliberately isolated from the shell lifecycle: a failed or slow
/// subsystem must degrade file capabilities without taking down the terminal.
async fn initialize_sftp_session(
    startup: &SshWorkerStartupContext<'_>,
) -> (Option<SharedSftpSession>, Option<String>) {
let app = startup.app;
let tab_id = startup.tab_id;
let profile = startup.profile;
let handle = startup.handle;
let operation_timeout = startup.operation_timeout;
let network_device_mode = startup.network_device_mode;
let exec_enabled = startup.exec_channel_enabled;
let cancellation = startup.cancellation;
let state = startup.state;
// ── SFTP subsystem ─────────────────────────────────────────────────────
// russh-sftp 2.3 needs an explicit subsystem request before converting
// the channel into its protocol stream. A failed SFTP negotiation must
// not tear down an otherwise healthy SSH shell: Electron keeps terminal
// and tunnel features available while exposing the file-channel error.
let sftp_enabled = effective_sftp_enabled(profile);
let (sftp_arc, sftp_unavailable_reason) = if network_device_mode {
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        "network-device mode; skipping SFTP channel",
    );
    (None, None)
} else if !sftp_enabled {
    let reason = "SFTP disabled for this connection profile".to_string();
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        "disabled by connection profile",
    );
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.sftp_unavailable_reason = Some(reason.clone());
            session.capabilities.files = false;
            session.capabilities.file_access = false;
        }
    }
    emit_terminal_data(app, tab_id, &format!("\r\n[files] {reason}\r\n")).await;
    (None, Some(reason))
} else {
    match open_sftp_session(handle, operation_timeout).await {
        Ok(sftp) => {
            crate::services::logging::session(
                app,
                "INFO",
                "sftp",
                tab_id,
                "SFTP session ready",
            );
            let sftp_arc = Arc::new(RwLock::new(sftp));
            let configured_initial_remote_path = {
                let sessions = state.sessions.read().await;
                sessions
                    .get(tab_id)
                    .map(|session| session.remote_path.clone())
                    .unwrap_or_else(|| {
                        crate::services::workspace::initial_remote_path_for_profile(profile)
                    })
            };
            let initial_remote_path = if is_implicit_ssh_home_path(
                &configured_initial_remote_path,
            ) {
                match resolve_initial_sftp_home_path(&sftp_arc, operation_timeout).await {
                    Ok(resolved_path) => {
                        crate::services::logging::ssh_debug(
                            app,
                            tab_id,
                            format!(
                                "initial SFTP home resolved configured={} resolved={resolved_path}",
                                configured_initial_remote_path
                            ),
                        );
                        resolved_path
                    }
                    Err(error) => {
                        crate::services::logging::ssh_debug(
                            app,
                            tab_id,
                            format!(
                                "initial SFTP home resolution failed configured={} error={error}; using configured path",
                                configured_initial_remote_path
                            ),
                        );
                        configured_initial_remote_path.clone()
                    }
                }
            } else {
                configured_initial_remote_path.clone()
            };
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    if session.remote_path == configured_initial_remote_path {
                        session.remote_path = initial_remote_path.clone();
                    }
                    session.remote_capabilities = Some(default_sftp_capabilities());
                }
            }
            // A server can accept the SFTP subsystem and then stop replying
            // to read_dir. Do not await the initial directory load before the
            // terminal select loop: otherwise Ctrl+C reaches IPC but cannot be
            // consumed until the SFTP request returns. The bound includes both
            // the lock wait and read_dir; the task publishes its own snapshot.
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    session.remote_files_loading = true;
                }
            }
            let initial_sftp = Arc::clone(&sftp_arc);
            let initial_handle = Arc::clone(handle);
            let initial_app = app.clone();
            let initial_tab_id = tab_id.to_string();
            let initial_cancellation = cancellation.clone();
            let initial_listing_timeout = operation_timeout.min(INITIAL_SFTP_LISTING_TIMEOUT);
            tokio::spawn(async move {
                crate::services::logging::ssh_debug(
                    &initial_app,
                    &initial_tab_id,
                    format!(
                        "initial directory listing started path={initial_remote_path} timeout_secs={}",
                        initial_listing_timeout.as_secs()
                    ),
                );
                let initial_files = tokio::select! {
                    _ = initial_cancellation.cancelled() => {
                        let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                        if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                            session.remote_files_loading = false;
                        }
                        if let Ok(snapshot) =
                            crate::commands::get_workspace_snapshot(initial_app.clone()).await
                        {
                            let _ = initial_app.emit("workspace:snapshot", snapshot);
                        }
                        crate::services::logging::ssh_debug(
                            &initial_app,
                            &initial_tab_id,
                            "initial directory listing cancelled",
                        );
                        return;
                    },
                    result = timeout(initial_listing_timeout, async {
                        let sftp = initial_sftp.write().await;
                        list_dir(&sftp, &initial_remote_path).await
                    }) => match result {
                        Ok(result) => result,
                        Err(_) => Err(format!("列出远程目录 {initial_remote_path} 超时")),
                    },
                };

                let initial_listing_error = initial_files.as_ref().err().cloned();
                let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                let mut initial_listing_is_current = false;
                let mut initial_listing_fallback_used = false;
                if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                    initial_listing_is_current = initial_remote_listing_matches_current_session(
                        &initial_remote_path,
                        &session.remote_path,
                        session.shell_cwd.as_deref(),
                        session.follow_shell_cwd,
                    );
                    if initial_listing_is_current {
                        session.remote_files_loading = false;
                        if let Ok(files) = &initial_files {
                            session.remote_files = files.clone();
                        }
                    } else {
                        if initial_remote_listing_can_be_fallback(
                            initial_listing_is_current,
                            &initial_remote_path,
                            &session.remote_path,
                            session.remote_files.is_empty(),
                        ) {
                            // A shell CWD may be outside the SFTP user's
                            // namespace (Synology commonly chroots SFTP
                            // at the user's home). Keep a successful
                            // listing of the visible SFTP path instead of
                            // leaving the pane empty after CWD follow fails.
                            if let Ok(files) = &initial_files {
                                session.remote_files = files.clone();
                                initial_listing_fallback_used = true;
                            }
                        }
                        // A manual navigation, a completed CWD follow, or
                        // an unmapped shell CWD owns the final state. The
                        // detached startup request must never leave the
                        // file pane loading forever.
                        session.remote_files_loading = false;
                    }
                }

                match &initial_files {
                    Ok(files) => crate::services::logging::ssh_debug(
                        &initial_app,
                        &initial_tab_id,
                        format!(
                            "initial directory listing completed path={initial_remote_path} entries={} current={initial_listing_is_current} fallback={initial_listing_fallback_used}",
                            files.len()
                        ),
                    ),
                    Err(error) => crate::services::logging::session(
                        &initial_app,
                        "WARN",
                        "sftp",
                        &initial_tab_id,
                        format!(
                            "initial directory listing failed path={initial_remote_path} current={initial_listing_is_current}: {error}"
                        ),
                    ),
                }

                if initial_listing_is_current {
                    if let Some(error) = initial_listing_error {
                        // A usable SFTP channel can still lack access to the
                        // profile's configured starting directory.
                        emit_terminal_data(
                            &initial_app,
                            &initial_tab_id,
                            &format!(
                                "\r\n[files] 列出目录 {initial_remote_path} 失败: {error}\r\n"
                            ),
                        )
                        .await;
                    }
                }

                // Publish the directory result before running optional
                // capability probes. fs_info/readlink/hardlink and the
                // SSH exec probe are best-effort metadata; a slow or
                // restricted server must not keep usable file rows behind
                // the loading spinner.
                if let Ok(snapshot) =
                    crate::commands::get_workspace_snapshot(initial_app.clone()).await
                {
                    let _ = initial_app.emit("workspace:snapshot", snapshot);
                }

                if initial_cancellation.is_cancelled() {
                    return;
                }

                crate::services::logging::ssh_debug(
                    &initial_app,
                    &initial_tab_id,
                    format!(
                        "initial capability probes started path={initial_remote_path} exec_enabled={exec_enabled} timeout_secs={}",
                        operation_timeout
                            .min(INITIAL_CAPABILITY_PROBE_TIMEOUT)
                            .as_secs()
                    ),
                );
                let capability_timeout =
                    operation_timeout.min(INITIAL_CAPABILITY_PROBE_TIMEOUT);
                let mut remote_capabilities = tokio::select! {
                    _ = initial_cancellation.cancelled() => return,
                    result = timeout(capability_timeout, async {
                        let sftp = initial_sftp.write().await;
                        inspect_sftp_capabilities(&sftp, &initial_remote_path).await
                    }) => match result {
                        Ok(capabilities) => capabilities,
                        Err(_) => {
                            crate::services::logging::session(
                                &initial_app,
                                "WARN",
                                "sftp",
                                &initial_tab_id,
                                format!(
                                    "initial SFTP capability probe timed out path={initial_remote_path} timeout_secs={}",
                                    capability_timeout.as_secs()
                                ),
                            );
                            default_sftp_capabilities()
                        },
                    },
                };
                let (server_copy, checksum_algorithms) = if exec_enabled {
                    tokio::select! {
                        _ = initial_cancellation.cancelled() => return,
                        result = inspect_ssh_exec_capabilities(&initial_handle, capability_timeout) => result,
                    }
                } else {
                    (false, Vec::new())
                };
                remote_capabilities.server_copy = server_copy;
                remote_capabilities.checksum_algorithms = checksum_algorithms;

                // The initial probe is deliberately detached from the
                // terminal select loop, but it must not publish results
                // after this worker has been stopped for a reconnect or
                // tab close. Otherwise a slow old SFTP probe can overwrite
                // the snapshot of the replacement session.
                if initial_cancellation.is_cancelled() {
                    return;
                }

                let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                    session.remote_capabilities = Some(remote_capabilities.clone());
                }
                crate::services::logging::ssh_debug(
                    &initial_app,
                    &initial_tab_id,
                    format!(
                        "initial capability probes completed path={initial_remote_path} server_copy={} checksums={}",
                        remote_capabilities.server_copy,
                        remote_capabilities.checksum_algorithms.len()
                    ),
                );
                if let Ok(snapshot) =
                    crate::commands::get_workspace_snapshot(initial_app.clone()).await
                {
                    let _ = initial_app.emit("workspace:snapshot", snapshot);
                }
            });
            (Some(sftp_arc), None)
        }
        Err(error) => {
            let reason = format_sftp_unavailable_reason(&error);
            crate::services::logging::session(
                app,
                "WARN",
                "sftp",
                tab_id,
                format!("unavailable: {reason}"),
            );
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    session.sftp_unavailable_reason = Some(reason.clone());
                    // The interactive SSH shell is still usable, but the
                    // file capability must reflect the failed subsystem
                    // handshake. Leaving it enabled makes the renderer
                    // offer file actions that can only return the cached
                    // SFTP error.
                    session.capabilities.files = false;
                    session.capabilities.file_access = false;
                }
            }
            emit_terminal_data(app, tab_id, &format!("\r\n[files] {reason}\r\n")).await;
            (None, Some(reason))
        }
    }
};

(sftp_arc, sftp_unavailable_reason)
}
