/// 构造兼容老服务器的算法偏好列表。
///
/// russh 0.62 的 `Preferred::DEFAULT` 注释明确"SHA-1 MAC variants are
/// excluded from defaults"，KEX 也只列出 SHA-2 系（DH_G14_SHA256 等）。
/// 这对 OpenSSH 4.x/5.x 时代的老服务器（只支持 hmac-sha1 / diffie-hellman
/// -group14-sha1 / diffie-hellman-group1-sha1）会导致 `NoCommonAlgo` 握手
/// 失败。
///
/// 这里把 SHA-1 类算法**追加到默认列表末尾**——SHA-2 仍然优先，只有当
/// 服务器不支持 SHA-2 时才回退到 SHA-1。RSA-SHA1 host key 已在默认列表
/// （`Algorithm::Rsa { hash: None }` 即 ssh-rsa），无需额外追加。
fn build_legacy_preferred() -> russh::Preferred {
    use std::borrow::Cow;

    let mut kex_list: Vec<russh::kex::Name> = russh::Preferred::DEFAULT.kex.to_vec();
    // SHA-1 KEX（按强度降序：group14 > group1 > gex-sha1）
    kex_list.push(russh::kex::DH_G14_SHA1);
    kex_list.push(russh::kex::DH_G1_SHA1);
    kex_list.push(russh::kex::DH_GEX_SHA1);

    let mut mac_list: Vec<russh::mac::Name> = russh::Preferred::DEFAULT.mac.to_vec();
    // SHA-1 MAC（ETM 优先于非 ETM，与默认列表风格一致）
    mac_list.push(russh::mac::HMAC_SHA1_ETM);
    mac_list.push(russh::mac::HMAC_SHA1);

    russh::Preferred {
        kex: Cow::Owned(kex_list),
        host_key_certificates: russh::Preferred::DEFAULT.host_key_certificates.clone(),
        key: russh::Preferred::DEFAULT.key.clone(),
        cipher: russh::Preferred::DEFAULT.cipher.clone(),
        mac: Cow::Owned(mac_list),
        compression: russh::Preferred::DEFAULT.compression.clone(),
    }
}

#[allow(clippy::too_many_arguments)] // Recursive jump-host setup keeps transport and interaction policy explicit.
async fn open_session(
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    authentication_target: SshAuthenticationTarget,
    flow: SshInteractionFlow,
    cancellation: CancellationToken,
) -> Result<OpenSshSession, String> {
    let mut effective_profile = profile.clone();
    let has_jump_host = effective_profile
        .get("jumpProfileId")
        .and_then(Value::as_str)
        .is_some();
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
            has_jump_host,
            interaction_timeout.as_secs(),
        ),
    );
    // A jump-host flow must authenticate the jump first. Defer a missing
    // target password until that flow has completed so the renderer never
    // presents a target-credential dialog before the jump-host dialog.
    if !has_jump_host {
        ensure_password_credentials(&mut effective_profile, app, &interaction, interaction_timeout)
            .await?;
    }
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
    let connect_timeout = seconds_from_profile(
        profile,
        "connectTimeoutSeconds",
        SSH_TRANSPORT_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(300),
    );
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
        // Netcatty #1045 的 Comware GEX 兼容只在显式开启 legacyAlgorithms
        // 后生效，并且由 russh 在握手前按远端 identification 精确匹配。
        comware_legacy_gex: legacy_algorithms,
        preferred: if legacy_algorithms {
            build_legacy_preferred()
        } else {
            russh::Preferred::default()
        },
        ..Default::default()
    };
    let config = Arc::new(config);

    // ── Jump Host support ─────────────────────────────────────────────────
    // Mirrors Electron's `connectJumpHost`: if the profile has a
    // `jumpProfileId`, first connect to the jump host, then open a
    // `direct-tcpip` channel through it to reach the target host.
    // The jump host's channel is used as the TCP socket for the main
    // SSH connection.
    let jump_profile_id = profile
        .get("jumpProfileId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    if let Some(jpid) = jump_profile_id {
        // Proxy + JumpHost 互斥校验：参考 OpenSSH ProxyJump 与 ProxyCommand
        // 互斥的设计。如果 profile 同时配了 proxy 和 jumpProfileId，proxy
        // 会被静默忽略——目标主机是通过跳板机的 direct-tcpip 通道到达的，
        // 不经过 SOCKS5/HTTP 代理。用户以为走了代理其实没走，既是安全隐患
        // （流量没走预期路径）也是 UX 问题（调试困难）。
        let proxy_type = profile
            .get("proxy")
            .and_then(|p| p.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("none");
        if proxy_type != "none" {
            return Err(
                "Proxy and Jump Host are mutually exclusive: the target is reached via the jump host's direct-tcpip channel, the proxy setting is ignored. Please remove one of them.".to_string()
            );
        }

        interaction.log_interaction(
            app,
            "INFO",
            "-",
            "session",
            "jump-host",
            0,
            "resolving jump host",
        );
        // Load the jump profile from disk (same directory as profiles.json)
        let jump_profile = load_jump_profile(app, &jpid)?;

        // Validate: jump must be a different SSH profile, and must not
        // itself have a jumpProfileId (no chained jumps).
        let jump_id = jump_profile
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if jump_id == profile.get("id").and_then(|v| v.as_str()).unwrap_or("") {
            return Err("Jump Host must reference a different profile".to_string());
        }
        if jump_profile.get("jumpProfileId").is_some() {
            return Err("Jump Host cannot itself reference another Jump Host".to_string());
        }

        interaction.log_interaction(
            app,
            "INFO",
            "-",
            "session",
            "jump-host",
            0,
            "connecting through jump host",
        );

        // Connect + authenticate to the jump host.
        // Box::pin is required because `open_session` is recursive (the jump
        // host itself could be resolved via another open_session call) and
        // Rust requires indirection for recursive async fns to avoid
        // infinitely-sized futures.
        let jump_session = Box::pin(open_session(
            &jump_profile,
            app,
            tab_id,
            interaction_timeout,
            interaction_window_label.clone(),
            SshAuthenticationTarget::JumpHost,
            flow.clone(),
            cancellation.clone(),
        ))
        .await?;
        let jump_identification = normalize_ssh_identification(&jump_session.remote_sshid);
        crate::services::logging::session(
            app,
            "INFO",
            "ssh",
            tab_id,
            format!(
                "jump host SSH session ready flow_role=jump-host identification={}",
                if jump_identification.is_empty() {
                    "unknown"
                } else {
                    jump_identification.as_str()
                }
            ),
        );
        if jump_identification
            .to_ascii_lowercase()
            .starts_with("ssh-2.0-go")
        {
            crate::services::logging::session(
                app,
                "WARN",
                "ssh",
                tab_id,
                "jump host identifies as SSH-2.0-Go; direct-tcpip target routing may be policy-gated by an application gateway (for KoKo use a direct asset username or an ordinary OpenSSH jump host)",
            );
        }
        let jump_handle = jump_session.handle;

        let mut target_profile = effective_profile.clone();
        let target_interaction = SshInteractionContext::from_profile(
            &flow,
            tab_id,
            &target_profile,
            &host,
            port,
            SshAuthenticationTarget::Target,
            interaction_window_label.clone(),
            cancellation.clone(),
        );
        ensure_password_credentials(
            &mut target_profile,
            app,
            &target_interaction,
            interaction_timeout,
        )
        .await?;
        let target_username = target_profile
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or("root")
            .to_string();
        let target_auth_type = target_profile
            .get("authType")
            .and_then(Value::as_str)
            .unwrap_or("password")
            .to_string();

        interaction.log_interaction(
            app,
            "INFO",
            "-",
            "session",
            "target",
            0,
            "jump host connected; opening target channel",
        );

        // 将跳板机目标连接 + 认证封装在 async block 中，以便在失败路径上
        // 显式发送 SSH_MSG_DISCONNECT 清理每个 session。参考 OpenSSH
        // 在 ProxyJump 失败时对每跳发送 disconnect 的做法——仅靠 Drop 不会
        // 发送 disconnect 消息，服务端可能残留半开 session 直到 TCP 超时。
        // target / retry handle 也需要显式 disconnect，否则目标机的
        // MaxStartups 统计可能虚高，极端情况下导致后续连接被拒绝。
        let target_result: Result<OpenSshSession, String> = async {
            let remote_sshid = Arc::new(StdMutex::new(None));
            let disconnect_reason = Arc::new(StdMutex::new(None));
            let target_host_verification_waiting = Arc::new(AtomicBool::new(false));
            let mut target_handle = connect_target_through_jump(
                &jump_handle,
                config.clone(),
                new_client_handler(
                    app,
                    tab_id,
                    &profile_id,
                    &host,
                    port,
                    trusted.clone(),
                    target_host_verification_waiting,
                    interaction_timeout,
                    interaction_window_label.clone(),
                    target_interaction.clone(),
                    remote_sshid.clone(),
                    disconnect_reason.clone(),
                ),
                &host,
                port,
                connect_timeout,
                interaction_timeout,
            )
            .await?;
            let target_remote_sshid = read_shared_remote_sshid(&remote_sshid);
            let target_identification = normalize_ssh_identification(&target_remote_sshid);
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                format!(
                    "target SSH session ready flow_role=target transport=direct-tcpip identification={}",
                    if target_identification.is_empty() {
                        "unknown"
                    } else {
                        target_identification.as_str()
                    }
                ),
            );
            if authenticate_session(
                &mut target_handle,
                &target_username,
                &target_auth_type,
                &target_profile,
                app,
                &target_interaction,
                interaction_timeout,
            )
            .await?
            {
                Ok(OpenSshSession {
                    handle: target_handle,
                    remote_sshid: target_remote_sshid,
                    disconnect_reason,
                })
            } else {
                let _ = timeout(
                    Duration::from_secs(3),
                    target_handle.disconnect(
                        Disconnect::ByApplication,
                        "authentication rejected",
                        "en",
                    ),
                )
                .await;
                Err("SSH Authentication failed (via jump host)".to_string())
            }
        }
        .await;

        match target_result {
            Ok(session) => return Ok(session),
            Err(error) => {
                // 显式断开跳板机 session，3s 超时防止 disconnect 本身卡住
                // （网络已中断时 russh 可能无法发送 disconnect 消息）。
                let _ = timeout(
                    Duration::from_secs(3),
                    jump_handle.disconnect(
                        Disconnect::ByApplication,
                        "target authentication failed",
                        "en",
                    ),
                )
                .await;
                return Err(error);
            }
        }
    }

    let stream = wait_for_ssh_stage(
        "SSH transport connection",
        connect_timeout,
        connect_ssh_transport(profile, &host, port),
    )
    .await?;
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!("socket connected target={host}:{port}"),
    );
    let remote_sshid = Arc::new(StdMutex::new(None));
    let disconnect_reason = Arc::new(StdMutex::new(None));
    let host_verification_waiting = Arc::new(AtomicBool::new(false));
    let mut handle = wait_for_ssh_handshake_with_network_timeout(
        "SSH protocol handshake",
        host_verification_waiting.clone(),
        connect_timeout,
        interaction_timeout,
        async {
            russh::client::connect_stream(
                config.clone(),
                stream,
                new_client_handler(
                    app,
                    tab_id,
                    &profile_id,
                    &host,
                    port,
                    trusted.clone(),
                    host_verification_waiting,
                    interaction_timeout,
                    interaction_window_label.clone(),
                    interaction.clone(),
                    remote_sshid.clone(),
                    disconnect_reason.clone(),
                ),
            )
            .await
            .map_err(|error| format!("SSH connect failed: {error}"))
        },
    )
    .await?;
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "SSH handshake completed");
    if authenticate_session(
        &mut handle,
        &username,
        &auth_type,
        profile,
        app,
        &interaction,
        interaction_timeout,
    )
    .await?
    {
        Ok(OpenSshSession {
            handle,
            remote_sshid: read_shared_remote_sshid(&remote_sshid),
            disconnect_reason,
        })
    } else {
        let _ = timeout(
            Duration::from_secs(3),
            handle.disconnect(Disconnect::ByApplication, "authentication rejected", "en"),
        )
        .await;
        Err("SSH Authentication failed".to_string())
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
        SshAuthenticationTarget::Direct,
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
