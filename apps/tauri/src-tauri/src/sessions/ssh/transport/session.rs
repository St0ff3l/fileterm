#[allow(clippy::too_many_arguments)] // Each hop keeps its own transport and authentication identity.
async fn open_session_hop(
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    authentication_target: SshAuthenticationTarget,
    flow: SshInteractionFlow,
    cancellation: CancellationToken,
    cleanup: &mut SshSessionCleanup<ClientHandler>,
) -> Result<(Vec<u8>, SharedSshDisconnectReason), String> {
    let upstream = cleanup.handles.last().cloned();
    let mut effective_profile = profile.clone();
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

    let remote_sshid = Arc::new(StdMutex::new(None));
    let disconnect_reason = Arc::new(StdMutex::new(None));
    let host_verification_waiting = Arc::new(AtomicBool::new(false));
    let mut handler = new_client_handler(
        app,
        tab_id,
        &profile_id,
        &host,
        port,
        trusted,
        host_verification_waiting.clone(),
        interaction_timeout,
        interaction_window_label,
        interaction.clone(),
        remote_sshid.clone(),
        disconnect_reason.clone(),
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
    let handle = if let Some(upstream) = upstream.as_ref() {
        connect_target_through_jump(
            upstream,
            config,
            handler,
            &host,
            port,
            connect_timeout,
            interaction_timeout,
        )
        .await?
    } else {
        let stream = wait_for_ssh_stage(
            "SSH transport connection",
            connect_timeout,
            connect_ssh_transport(profile, &host, port),
        )
        .await?;
        wait_for_ssh_handshake_with_network_timeout(
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
        .await?
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
        Ok((remote_sshid, disconnect_reason))
    } else {
        Err(format!("SSH Authentication failed at {host}:{port}"))
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
