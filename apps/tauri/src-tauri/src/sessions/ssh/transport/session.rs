#[allow(clippy::too_many_arguments)] // Handler construction keeps the connection identity and interaction policy explicit.
fn new_session_client_handler(
    app: &AppHandle,
    tab_id: &str,
    profile: &Value,
    profile_id: &str,
    host: &str,
    port: u16,
    trusted: Option<String>,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    interaction: SshInteractionContext,
    remote_sshid: SharedRemoteSshId,
    disconnect_reason: SharedSshDisconnectReason,
    host_verification_waiting: Arc<AtomicBool>,
) -> ClientHandler {
    let mut handler = new_client_handler(
        app,
        tab_id,
        profile_id,
        host,
        port,
        trusted,
        host_verification_waiting,
        interaction_timeout,
        interaction_window_label,
        interaction,
        remote_sshid,
        disconnect_reason,
    );
    handler.known_hosts_files = ssh_config_known_hosts_files(profile);
    handler.strict_host_key_checking = profile
        .get(SSH_CONFIG_RESOLVED_KEY)
        .and_then(|resolved| resolved.get("strictHostKeyChecking"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    handler.agent_forward_socket = ssh_config_agent_forwarding(profile).map(|socket| {
        let home = app.path().home_dir().ok();
        socket
            .map(|raw| match home.as_deref() {
                Some(home) => crate::services::ssh_config::expand_user_path(&raw, home)
                    .to_string_lossy()
                    .into_owned(),
                None => raw,
            })
            .or_else(|| configured_agent_socket(profile, home.as_deref()))
    });
    handler
}

enum SshRouteHandshakeError {
    Retryable(SshRouteFailure, String),
    Fatal(String),
}

fn ssh_route_handshake_failure(error: &russh::Error) -> Option<SshRouteFailure> {
    match error {
        russh::Error::HUP | russh::Error::ConnectionTimeout => {
            Some(SshRouteFailure::HandshakeTransportFailed)
        }
        russh::Error::IO(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::ConnectionRefused
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::HostUnreachable
                    | std::io::ErrorKind::NetworkUnreachable
                    | std::io::ErrorKind::NotConnected
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::UnexpectedEof
            ) =>
        {
            Some(SshRouteFailure::HandshakeTransportFailed)
        }
        _ => None,
    }
}

async fn wait_for_ssh_route_handshake(
    host_verification_waiting: Arc<AtomicBool>,
    network_timeout: Duration,
    interaction_timeout: Duration,
    operation: impl Future<Output = Result<Handle<ClientHandler>, russh::Error>>,
) -> Result<Handle<ClientHandler>, SshRouteHandshakeError> {
    let (mut network_elapsed, mut verification_elapsed) = (Duration::ZERO, Duration::ZERO);
    let mut last_tick = Instant::now();
    tokio::pin!(operation);
    loop {
        tokio::select! {
            result = &mut operation => return result.map_err(|error| {
                let failure = ssh_route_handshake_failure(&error);
                match failure {
                    Some(failure) => SshRouteHandshakeError::Retryable(failure, error.to_string()),
                    None => SshRouteHandshakeError::Fatal(error.to_string()),
                }
            }),
            _ = sleep(Duration::from_millis(100)) => {
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last_tick);
                last_tick = now;
                if host_verification_waiting.load(Ordering::Acquire) {
                    verification_elapsed += elapsed;
                    if verification_elapsed >= interaction_timeout {
                        return Err(SshRouteHandshakeError::Fatal(format!(
                            "SSH host-key verification timed out after {}",
                            timeout_label(interaction_timeout),
                        )));
                    }
                } else {
                    network_elapsed += elapsed;
                    if network_elapsed >= network_timeout {
                        return Err(SshRouteHandshakeError::Retryable(
                            SshRouteFailure::HandshakeTimeout,
                            format!("SSH protocol handshake timed out after {}", timeout_label(network_timeout)),
                        ));
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Each hop keeps its own transport and authentication identity.
async fn open_session_hop(
    route_hop: &Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    authentication_target: SshAuthenticationTarget,
    flow: SshInteractionFlow,
    cancellation: CancellationToken,
    cleanup: &mut SshSessionCleanup<ClientHandler>,
    policy: Option<&SshRouteSelection>,
) -> Result<(Vec<u8>, SharedSshDisconnectReason, Option<String>), String> {
    let upstream = cleanup.handles.last().cloned();
    let mut effective_profile = route_hop.clone();
    let host = effective_profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let configured_username = effective_profile
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("root")
        .to_string();
    if let Some(normalized_username) =
        crate::sessions::system_metrics::normalize_jumpserver_cli_username(
            &configured_username,
            &host,
        )
    {
        effective_profile["username"] = Value::String(normalized_username);
        crate::services::logging::session(
            app,
            "INFO",
            "ssh",
            tab_id,
            format!(
                "JumpServer username normalized source_shape=full-cli-destination target_shape=direct-asset configured_user_segments={} normalized_user_segments=3 host_match=true",
                configured_username
                    .split(['@', '#'])
                    .filter(|part| !part.trim().is_empty())
                    .count(),
            ),
        );
    }
    let port = port_from_profile(&effective_profile, 22, "SSH")?;
    let interaction = SshInteractionContext::from_profile(
        &flow,
        tab_id,
        &effective_profile,
        &host,
        port,
        authentication_target,
        interaction_window_label.clone(),
        cancellation.clone(),
    );
    interaction.log_interaction(
        app,
        "DEBUG",
        "-",
        "session",
        "open",
        0,
        format!(
            "started jump_configured={} interaction_timeout_secs={}",
            upstream.is_some(),
            interaction_timeout.as_secs(),
        ),
    );
    ensure_ssh_credentials(
        &mut effective_profile,
        app,
        &interaction,
        interaction_timeout,
    )
    .await?;
    let profile = &effective_profile;
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root")
        .to_string();
    let auth_type = profile
        .get("authType")
        .and_then(|a| a.as_str())
        .unwrap_or("password")
        .to_string();
    let connect_timeout = ssh_connect_timeout(profile);
    let trusted = trusted_host_fingerprint(profile);
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "opening session host={host} port={port} auth_type={auth_type} saved_host_key={}",
            trusted.is_some()
        ),
    );

    let profile_id = profile
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();
    // 兼容老服务器（OpenSSH 4.x/5.x 时代）：默认算法列表只允许 SHA-2 类
    // MAC/KEX，对只支持 SHA-1 的服务器握手会因 NoCommonAlgo 被拒。开启
    // legacyAlgorithms 后追加 SHA-1 类算法到列表末尾——SHA-2 仍然优先，
    // 只有双方没交集时才回退到 SHA-1。
    let legacy_algorithms = profile
        .get("legacyAlgorithms")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let keepalive = KeepalivePolicy::from_profile(profile);
    let inactivity_timeout = keepalive
        .interval
        .and_then(|interval| interval.checked_mul((keepalive.max_misses as u32).saturating_add(1)));
    let config = russh::client::Config {
        // Keepalive：NAT/firewall 会静默掐掉空闲 TCP 连接，用户下次操作时
        // 才发现"连接已断"。Profile 可以关闭或调整间隔/最大丢失次数；
        // russh 的 inactivity timeout 与同一策略对齐，避免两个独立计时器
        // 互相打架。关闭 keepalive 时不额外设置空闲断开。
        inactivity_timeout,
        keepalive_interval: keepalive.interval,
        keepalive_max: keepalive.max_misses,
        ..build_ssh_algorithm_config(legacy_algorithms).map_err(|error| error.to_string())?
    };
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "SSH algorithm policy legacy_algorithms={legacy_algorithms} gex_min={} gex_preferred={} gex_max={} (Comware SHA-1 may use a different preferred size)",
            config.gex.min_group_size(),
            config.gex.preferred_group_size(),
            config.gex.max_group_size(),
        ),
    );
    let config = Arc::new(config);

    let (handle, remote_sshid, disconnect_reason, selected_route) =
        if let Some(upstream) = upstream.as_ref() {
            let remote_sshid = Arc::new(StdMutex::new(None));
            let disconnect_reason = Arc::new(StdMutex::new(None));
            let handler = new_session_client_handler(
                app,
                tab_id,
                profile,
                &profile_id,
                &host,
                port,
                trusted,
                interaction_timeout,
                interaction_window_label,
                interaction.clone(),
                remote_sshid.clone(),
                disconnect_reason.clone(),
                Arc::new(AtomicBool::new(false)),
            );
            let handle = connect_target_through_jump(
                upstream,
                config,
                handler,
                &host,
                port,
                connect_timeout,
                interaction_timeout,
            )
            .await?;
            (handle, remote_sshid, disconnect_reason, None)
        } else if let Some(policy) = policy {
            let mut excluded = std::collections::HashSet::new();
            let mut last_handshake_error = None;
            loop {
                let (stream, selected) = match policy
                    .connect_excluding(
                        app,
                        tab_id,
                        SshRouteTarget {
                            profile_id: profile_id.clone(),
                            host: host.clone(),
                            port,
                        },
                        connect_timeout,
                        &mut excluded,
                    )
                    .await
                {
                    Ok(connection) => connection,
                    Err(error) => {
                        return Err(last_handshake_error
                            .map(|previous| format!("{previous}; {error}"))
                            .unwrap_or(error));
                    }
                };
                let fallback = !excluded.is_empty();
                excluded.insert(selected.route.clone());
                let remote_sshid = Arc::new(StdMutex::new(None));
                let disconnect_reason = Arc::new(StdMutex::new(None));
                let host_verification_waiting = Arc::new(AtomicBool::new(false));
                let handler = new_session_client_handler(
                    app,
                    tab_id,
                    profile,
                    &profile_id,
                    &host,
                    port,
                    trusted.clone(),
                    interaction_timeout,
                    interaction_window_label.clone(),
                    interaction.clone(),
                    remote_sshid.clone(),
                    disconnect_reason.clone(),
                    host_verification_waiting.clone(),
                );
                let started = Instant::now();
                match wait_for_ssh_route_handshake(
                    host_verification_waiting,
                    connect_timeout,
                    interaction_timeout,
                    russh::client::connect_stream(config.clone(), stream, handler),
                )
                .await
                {
                    Ok(handle) => {
                        policy.report(
                            app,
                            tab_id,
                            &profile_id,
                            &selected.route,
                            started.elapsed(),
                            None,
                            fallback,
                            "handshake",
                        );
                        break (handle, remote_sshid, disconnect_reason, Some(selected));
                    }
                    Err(SshRouteHandshakeError::Retryable(failure, error)) => {
                        observe_ssh_route(
                            &selected.target,
                            &selected.route,
                            started.elapsed(),
                            Some(failure),
                        );
                        policy.report(
                            app,
                            tab_id,
                            &profile_id,
                            &selected.route,
                            started.elapsed(),
                            Some(failure),
                            fallback,
                            "handshake",
                        );
                        last_handshake_error = Some(error);
                    }
                    Err(SshRouteHandshakeError::Fatal(error)) => {
                        policy.report(
                            app,
                            tab_id,
                            &profile_id,
                            &selected.route,
                            started.elapsed(),
                            Some(SshRouteFailure::HandshakeRejected),
                            fallback,
                            "handshake",
                        );
                        return Err(error);
                    }
                }
            }
        } else {
            let remote_sshid = Arc::new(StdMutex::new(None));
            let disconnect_reason = Arc::new(StdMutex::new(None));
            let host_verification_waiting = Arc::new(AtomicBool::new(false));
            let handler = new_session_client_handler(
                app,
                tab_id,
                profile,
                &profile_id,
                &host,
                port,
                trusted,
                interaction_timeout,
                interaction_window_label,
                interaction.clone(),
                remote_sshid.clone(),
                disconnect_reason.clone(),
                host_verification_waiting.clone(),
            );
            let stream = wait_for_ssh_stage(
                &format!("SSH transport connection host={host:?} port={port}"),
                connect_timeout,
                connect_ssh_transport(profile, &host, port),
            )
            .await?;
            let handle = wait_for_ssh_handshake_with_network_timeout(
                "SSH protocol handshake",
                host_verification_waiting,
                connect_timeout,
                interaction_timeout,
                async {
                    russh::client::connect_stream(config, stream, handler)
                        .await
                        .map_err(|error| format!("SSH connect failed: {error}"))
                },
            )
            .await?;
            (handle, remote_sshid, disconnect_reason, None)
        };
    // Register before authentication so canceled MFA and failed credentials also disconnect.
    cleanup.handles.push(Arc::new(handle));
    let handle = Arc::get_mut(cleanup.handles.last_mut().unwrap()).unwrap();
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "SSH handshake completed");
    if authenticate_session(
        handle,
        &username,
        &auth_type,
        profile,
        app,
        &interaction,
        interaction_timeout,
    )
    .await?
    {
        let endpoint = selected_route.map(|selected| {
            if let Some(policy) = policy {
                policy.report(
                    app,
                    tab_id,
                    &profile_id,
                    &selected.route,
                    Duration::ZERO,
                    None,
                    false,
                    "authenticated",
                );
            }
            selected.authenticated(route_hop)
        });
        let remote_sshid = read_shared_remote_sshid(&remote_sshid);
        if matches!(authentication_target, SshAuthenticationTarget::JumpHost(_))
            && normalize_ssh_identification(&remote_sshid)
                .to_ascii_lowercase()
                .starts_with("ssh-2.0-go")
        {
            crate::services::logging::session(
                app, "WARN", "ssh", tab_id,
                "jump host identifies as SSH-2.0-Go; direct-tcpip target routing may be policy-gated by an application gateway",
            );
        }
        Ok((remote_sshid, disconnect_reason, endpoint))
    } else {
        Err(format!(
            "SSH Authentication failed at {host}:{port} (configured method: {auth_type})"
        ))
    }
}

#[cfg(test)]
mod ssh_route_handshake_tests {
    use super::*;

    #[test]
    fn retries_network_handshake_errors_but_not_server_or_host_key_errors() {
        assert!(ssh_route_handshake_failure(&russh::Error::HUP)
            .is_some_and(|failure| failure == SshRouteFailure::HandshakeTransportFailed));
        assert!(
            ssh_route_handshake_failure(&russh::Error::IO(std::io::Error::from(
                std::io::ErrorKind::UnexpectedEof
            )))
            .is_some_and(|failure| failure == SshRouteFailure::HandshakeTransportFailed)
        );
        assert!(ssh_route_handshake_failure(&russh::Error::Version).is_none());
        assert!(ssh_route_handshake_failure(&russh::Error::KeyChanged { line: 1 }).is_none());
    }

    #[tokio::test]
    async fn only_network_handshake_timeout_is_retryable() {
        let result = wait_for_ssh_route_handshake(
            Arc::new(AtomicBool::new(false)),
            Duration::from_millis(120),
            Duration::from_secs(1),
            std::future::pending(),
        )
        .await;
        assert!(matches!(
            result,
            Err(SshRouteHandshakeError::Retryable(
                SshRouteFailure::HandshakeTimeout,
                _
            ))
        ));

        let result = wait_for_ssh_route_handshake(
            Arc::new(AtomicBool::new(true)),
            Duration::from_millis(120),
            Duration::from_millis(120),
            std::future::pending(),
        )
        .await;
        assert!(matches!(result, Err(SshRouteHandshakeError::Fatal(_))));
    }
}

/// Verify SSH transport, host-key policy, and authentication without opening
/// a shell or SFTP channel. The caller supplies a transient tab id and the
/// owning WebView label so host-key prompts are delivered to the form that
/// started the test instead of racing with every renderer window.
pub async fn test_connection(
    app: &AppHandle,
    profile: &Value,
    tab_id: &str,
    interaction_window_label: String,
) -> Result<(), String> {
    let flow = SshInteractionFlow::new();
    let cancellation = CancellationToken::new();
    let draft_route = profile
        .get("sshRouteRule")
        .filter(|value| !value.is_null())
        .map(|draft| parse_ssh_route_draft(profile, draft.clone()))
        .transpose()?;
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "connection test started flow_id={} interaction_timeout_secs={}",
            flow.flow_id,
            SSH_CONNECTION_TEST_INTERACTION_TIMEOUT.as_secs(),
        ),
    );
    let session_result = open_session(
        profile,
        app,
        tab_id,
        SSH_CONNECTION_TEST_INTERACTION_TIMEOUT,
        Some(interaction_window_label),
        flow.clone(),
        cancellation.clone(),
        draft_route,
    )
    .await;
    // A connection test is transient. Cancel its interaction boundary as
    // soon as the open attempt returns so a late handler future cannot leave
    // a credentials/MFA sender behind while the form is already closing.
    cancellation.cancel();
    let session = match session_result {
        Ok(handle) => handle,
        Err(error) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "ssh",
                tab_id,
                format!(
                    "connection test failed flow_id={} stage=open_session error={error}",
                    flow.flow_id
                ),
            );
            return Err(error);
        }
    };
    let handle = session.handle;
    let remote_sshid = session.remote_sshid;
    let resolution = resolve_ssh_device_mode(profile, &remote_sshid);
    log_ssh_device_mode_resolution(app, tab_id, profile, &remote_sshid, resolution);
    let _ = timeout(
        Duration::from_secs(3),
        handle.disconnect(Disconnect::ByApplication, "connection test complete", "en"),
    )
    .await;
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!("connection test completed flow_id={}", flow.flow_id),
    );
    Ok(())
}

/// Verify a standalone SFTP profile by opening the SSH transport and the SFTP
/// subsystem directly, without requesting a PTY, shell, or terminal channel.
pub async fn test_sftp_connection(
    app: &AppHandle,
    profile: &Value,
    tab_id: &str,
    interaction_window_label: String,
) -> Result<(), String> {
    let flow = SshInteractionFlow::new();
    let cancellation = CancellationToken::new();
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        format!("connection test started flow_id={}", flow.flow_id),
    );
    let session_result = open_session(
        profile,
        app,
        tab_id,
        SSH_CONNECTION_TEST_INTERACTION_TIMEOUT,
        Some(interaction_window_label),
        flow.clone(),
        cancellation.clone(),
        None,
    )
    .await;
    cancellation.cancel();
    let session = session_result?;
    let result = match open_sftp_session(
        &session.handle,
        file_operation_timeout(profile),
        sftp_filename_encoding(profile),
    )
    .await
    {
        Ok(sftp) => sftp.close().await.map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    let _ = timeout(
        Duration::from_secs(3),
        session.handle.disconnect(
            Disconnect::ByApplication,
            "SFTP connection test complete",
            "en",
        ),
    )
    .await;
    result?;
    crate::services::logging::session(
        app,
        "INFO",
        "sftp",
        tab_id,
        format!("connection test completed flow_id={}", flow.flow_id),
    );
    Ok(())
}
