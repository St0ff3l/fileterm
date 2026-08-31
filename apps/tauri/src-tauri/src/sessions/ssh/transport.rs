/// Compute the OpenSSH-style SHA256 fingerprint of a host key.
///
/// Matches Electron's `computeHostFingerprint`:
/// `SHA256:` + base64(sha256(ssh_wire_encoded_public_key)) with `=` padding
/// stripped. The `ssh-key` crate's `Fingerprint` `Display` impl produces
/// exactly this format, so we defer to it instead of re-encoding manually.
fn fingerprint_sha256_base64(key: &russh::keys::PublicKey) -> String {
    format!("{}", key.fingerprint(russh::keys::HashAlg::Sha256))
}

/// Open an SSH session using the profile credentials. `trusted_fingerprint`
/// flows into the Handler's `check_server_key` so it can short-circuit the
/// accept/reject prompt when the fingerprint already matches.
/// Load a jump host profile from the profiles.json storage by its id.
/// Mirrors Electron's `resolveProfile(jumpProfileId)`.
/// 校验 profile 类型必须为 ssh：UI 层已过滤，但存储层可能被篡改或残留
/// 旧数据，FTP/Serial/ Telnet profile 无法作为 SSH 跳板，提前拒绝避免
/// 在 russh 握手阶段才失败、错误信息不清晰。
fn load_jump_profile(app: &AppHandle, profile_id: &str) -> Result<Value, String> {
    let profiles = crate::storage::read_json_array(app, "profiles.json")
        .map_err(|e| format!("Failed to read profiles.json for jump host: {}", e))?;
    let profile = profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(profile_id))
        .cloned()
        .ok_or_else(|| format!("Jump Host profile '{}' not found", profile_id))?;
    let profile_type = profile.get("type").and_then(Value::as_str).unwrap_or("");
    if profile_type != "ssh" {
        return Err(format!(
            "Jump Host profile '{}' must be an SSH profile, got '{}'",
            profile_id, profile_type
        ));
    }
    Ok(profile)
}

trait SshTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SshTransport for T {}

type BoxedSshTransport = Box<dyn SshTransport>;

#[allow(clippy::too_many_arguments)] // SSH handler construction keeps the connection identity and interaction policy explicit.
fn new_client_handler(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
    trusted_fingerprint: Option<String>,
    host_verification_waiting: Arc<AtomicBool>,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    remote_sshid: SharedRemoteSshId,
) -> ClientHandler {
    ClientHandler {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile_id: profile_id.to_string(),
        host: host.to_string(),
        port,
        trusted_fingerprint,
        host_verification_waiting,
        interaction_timeout,
        interaction_window_label,
        remote_sshid,
    }
}

async fn connect_target_through_jump(
    jump_handle: &Handle<ClientHandler>,
    config: Arc<russh::client::Config>,
    handler: ClientHandler,
    host: &str,
    port: u16,
    connect_timeout: Duration,
    interaction_timeout: Duration,
) -> Result<Handle<ClientHandler>, String> {
    let host_verification_waiting = handler.host_verification_waiting.clone();
    let log_app = handler.app.clone();
    let log_tab_id = handler.tab_id.clone();
    let channel = wait_for_ssh_stage("SSH jump-host channel setup", connect_timeout, async {
        jump_handle
            .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
            .await
            .map_err(|error| format!("Jump Host direct-tcpip failed: {error}"))
    })
    .await?;
    crate::services::logging::session(
        &log_app,
        "INFO",
        "ssh",
        &log_tab_id,
        format!("socket connected via jump host target={host}:{port}"),
    );
    let result = wait_for_ssh_handshake_with_network_timeout(
        "SSH handshake via jump host",
        host_verification_waiting,
        connect_timeout,
        interaction_timeout,
        async {
            russh::client::connect_stream(config, channel.into_stream(), handler)
                .await
                .map_err(|error| format!("SSH connect via jump host failed: {error}"))
        },
    )
    .await;
    if result.is_ok() {
        crate::services::logging::session(
            &log_app,
            "INFO",
            "ssh",
            &log_tab_id,
            "SSH handshake completed via jump host",
        );
    }
    result
}

/// Creates the raw transport used by russh. Profiles with a SOCKS5 or HTTP
/// CONNECT proxy must reach the target through that proxy before SSH begins
/// its handshake; passing the profile directly to `russh::connect` bypasses
/// proxy configuration entirely.
async fn connect_ssh_transport(
    profile: &Value,
    host: &str,
    port: u16,
) -> Result<BoxedSshTransport, String> {
    let proxy = profile.get("proxy").and_then(Value::as_object);
    let proxy_type = proxy
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");

    if proxy_type == "none" {
        // 外层 wait_for_ssh_stage(SSH_TRANSPORT_TIMEOUT) 已提供 30s 超时保护，
        // 此处无需再加内层 timeout。
        let stream = TcpStream::connect((host, port))
            .await
            .map_err(|error| format!("SSH connect failed: {error}"))?;
        let _ = stream.set_nodelay(true);
        return Ok(Box::new(stream));
    }

    let proxy_host = proxy
        .and_then(|value| value.get("host"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Proxy host is required".to_string())?;
    validate_proxy_host(proxy_host)?;
    let proxy_port = proxy
        .and_then(|value| value.get("port"))
        .and_then(Value::as_u64)
        .filter(|value| (1..=u16::MAX as u64).contains(value))
        .ok_or_else(|| "Proxy port must be between 1 and 65535".to_string())?
        as u16;
    let username = proxy
        .and_then(|value| value.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let password = proxy
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .unwrap_or("");
    validate_proxy_credentials(username, password)?;

    match proxy_type {
        "socks5" => {
            let stream = if username.is_empty() {
                timeout(
                    PROXY_IO_TIMEOUT,
                    Socks5Stream::connect((proxy_host, proxy_port), (host, port)),
                )
                .await
                .map_err(|_| "SOCKS5 proxy connect timed out".to_string())?
                .map_err(|error| format!("SOCKS5 proxy connect failed: {error}"))?
            } else {
                timeout(
                    PROXY_IO_TIMEOUT,
                    Socks5Stream::connect_with_password(
                        (proxy_host, proxy_port),
                        (host, port),
                        username,
                        password,
                    ),
                )
                .await
                .map_err(|_| "SOCKS5 proxy authentication timed out".to_string())?
                .map_err(|error| format!("SOCKS5 proxy authentication failed: {error}"))?
            };
            Ok(Box::new(stream))
        }
        "http" => Ok(Box::new(
            connect_http_proxy(proxy_host, proxy_port, host, port, username, password).await?,
        )),
        other => Err(format!("Unsupported proxy type: {other}")),
    }
}

/// 校验代理主机名：拒绝控制字符（含 CRLF，防止 HTTP CONNECT 头注入；
/// SOCKS5 虽是二进制协议，但控制字符 host 对任何代理都是非法输入），
/// 拒绝超长 host（RFC 1035 限制 253 字符，留余量到 255）。
fn validate_proxy_host(host: &str) -> Result<(), String> {
    if host.len() > 255 {
        return Err("Proxy host is too long (max 255 characters)".to_string());
    }
    if host.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err("Proxy host contains control characters".to_string());
    }
    Ok(())
}

/// 校验代理凭据：SOCKS5 用户名/密码认证（RFC 1929）限制各 255 字节；
/// HTTP Basic Auth 无硬限制，但超长值既无意义又可能是注入尝试。
/// 控制字符检查防止 HTTP CONNECT 头注入（build_http_connect_request
/// 已检查 CRLF，这里作为纵深防御覆盖 SOCKS5 路径）。
fn validate_proxy_credentials(username: &str, password: &str) -> Result<(), String> {
    for (field, label) in [(username, "username"), (password, "password")] {
        if field.len() > 255 {
            return Err(format!(
                "Proxy {} is too long (max 255 bytes, RFC 1929)",
                label
            ));
        }
        if field.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(format!("Proxy {} contains control characters", label));
        }
    }
    Ok(())
}

async fn connect_http_proxy(
    proxy_host: &str,
    proxy_port: u16,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<TcpStream, String> {
    let mut stream = timeout(
        PROXY_IO_TIMEOUT,
        TcpStream::connect((proxy_host, proxy_port)),
    )
    .await
    .map_err(|_| {
        format!(
            "HTTP proxy connect timed out after {} seconds",
            PROXY_IO_TIMEOUT.as_secs()
        )
    })?
    .map_err(|error| format!("HTTP proxy connect failed: {error}"))?;
    let _ = stream.set_nodelay(true);
    let request = build_http_connect_request(host, port, username, password)?;
    timeout(PROXY_IO_TIMEOUT, stream.write_all(&request))
        .await
        .map_err(|_| "HTTP proxy CONNECT write timed out".to_string())?
        .map_err(|error| format!("HTTP proxy CONNECT write failed: {error}"))?;

    let mut response = Vec::with_capacity(1024);
    // Do not consume bytes beyond the HTTP boundary: a proxy may coalesce
    // its 200 response with the first SSH identification bytes, and a raw
    // TcpStream has no way to put those bytes back for russh.
    let mut chunk = [0_u8; 1];
    while !response.windows(4).any(|window| window == b"\r\n\r\n") {
        if response.len() >= 32 * 1024 {
            return Err("HTTP proxy response headers are too large".to_string());
        }
        let read = timeout(PROXY_IO_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| "HTTP proxy CONNECT read timed out".to_string())?
            .map_err(|error| format!("HTTP proxy CONNECT read failed: {error}"))?;
        if read == 0 {
            return Err("HTTP proxy closed before CONNECT completed".to_string());
        }
        response.extend_from_slice(&chunk[..read]);
    }

    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or(response.len());
    let status_line = std::str::from_utf8(&response[..header_end])
        .map_err(|_| "HTTP proxy returned a non-text response".to_string())?
        .lines()
        .next()
        .unwrap_or("");
    let status = parse_http_connect_status(status_line)?;
    if status != 200 {
        return Err(format!("HTTP proxy CONNECT failed: {status_line}"));
    }
    Ok(stream)
}

/// 从 HTTP CONNECT 响应状态行提取并校验状态码。
/// 状态行格式：`HTTP/1.1 200 Connection established`。校验 `HTTP/` 前缀
/// 防止恶意代理返回非 HTTP 文本伪装成功；状态码必须是 3 位 ASCII 数字，
/// 避免 `split_whitespace().nth(1)` 在异常格式下取到非状态码字段。
fn parse_http_connect_status(status_line: &str) -> Result<u16, String> {
    let mut parts = status_line.split_whitespace();
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err(format!(
            "HTTP proxy returned a malformed status line: {status_line}"
        ));
    }
    let code = parts.next().unwrap_or("");
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "HTTP proxy returned a malformed status code: {status_line}"
        ));
    }
    code.parse::<u16>()
        .map_err(|_| format!("HTTP proxy returned an invalid status code: {status_line}"))
}

fn build_http_connect_request(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<Vec<u8>, String> {
    if [host, username, password]
        .iter()
        .any(|value| value.contains(['\r', '\n']))
    {
        return Err("Proxy values must not contain line breaks".to_string());
    }
    let authority = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    );
    if !username.is_empty() {
        let credentials =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    Ok(request.into_bytes())
}

fn missing_password_credential(profile: &Value) -> Option<&'static str> {
    if profile
        .get("authType")
        .and_then(Value::as_str)
        .unwrap_or("password")
        != "password"
    {
        return None;
    }
    if profile
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        return Some("missing-username");
    }
    if profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    if profile
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty()
    {
        return Some("missing-password");
    }
    None
}

fn password_for_authentication(profile: &Value) -> Option<&str> {
    if profile
        .get("useEmptyPassword")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Some("");
    }
    profile
        .get("password")
        .and_then(Value::as_str)
        .filter(|password| !password.is_empty())
}

/// Renderer-side connection forms keep an empty string as the stable default
/// for `trustedHostFingerprint`. Treat that exactly like an absent field: an
/// empty value is not a previously trusted key and must not be surfaced as a
/// misleading "mismatch" in the host-verification prompt.
fn trusted_host_fingerprint(profile: &Value) -> Option<String> {
    profile
        .get("trustedHostFingerprint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|fingerprint| !fingerprint.is_empty())
        .map(str::to_string)
}

async fn ensure_password_credentials(
    profile: &mut Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
) -> Result<(), String> {
    let Some(reason) = missing_password_credential(profile) else {
        return Ok(());
    };
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<Value>();
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        state
            .pending_interactions
            .write()
            .await
            .insert(request_id.clone(), tx);
    }
    let payload = serde_json::json!({
        "requestId": request_id,
        "kind": "credentials",
        "tabId": tab_id,
        "profileId": profile.get("id").and_then(Value::as_str).unwrap_or(""),
        "host": profile.get("host").and_then(Value::as_str).unwrap_or(""),
        "port": profile.get("port").and_then(Value::as_u64).unwrap_or(22),
        "username": profile.get("username").and_then(Value::as_str),
        "passwordRequired": true,
        "reason": reason,
    });
    if let Err(error) = app.emit("ssh:interaction", payload) {
        app.state::<crate::services::workspace::WorkspaceState>()
            .pending_interactions
            .write()
            .await
            .remove(&request_id);
        return Err(error.to_string());
    }

    let response = match timeout(interaction_timeout, rx).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => return Err("SSH credentials request canceled".to_string()),
        Err(_) => {
            app.state::<crate::services::workspace::WorkspaceState>()
                .pending_interactions
                .write()
                .await
                .remove(&request_id);
            return Err("SSH credentials request timed out".to_string());
        }
    };
    if response
        .get("canceled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err("SSH credentials request canceled".to_string());
    }
    let username = response
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let password = response
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("");
    if username.is_empty() || password.is_empty() {
        return Err("SSH username and password are required".to_string());
    }
    let object = profile
        .as_object_mut()
        .ok_or_else(|| "SSH profile is invalid".to_string())?;
    object.insert("username".to_string(), Value::String(username.to_string()));
    object.insert("password".to_string(), Value::String(password.to_string()));
    Ok(())
}

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

async fn open_session(
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
    interaction_timeout: Duration,
    interaction_window_label: Option<String>,
    authentication_target: SshAuthenticationTarget,
) -> Result<OpenSshSession, String> {
    let mut effective_profile = profile.clone();
    let has_jump_host = effective_profile
        .get("jumpProfileId")
        .and_then(Value::as_str)
        .is_some();
    // A jump-host flow must authenticate the jump first. Defer a missing
    // target password until that flow has completed so the renderer never
    // presents a target-credential dialog before the jump-host dialog.
    if !has_jump_host {
        ensure_password_credentials(&mut effective_profile, app, tab_id, interaction_timeout)
            .await?;
    }
    let profile = &effective_profile;
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = port_from_profile(profile, 22, "SSH")?;
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

        crate::services::logging::session(app, "INFO", "ssh", tab_id, "resolving jump host");
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

        crate::services::logging::session(
            app,
            "INFO",
            "ssh",
            tab_id,
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
        ))
        .await?;
        let jump_handle = jump_session.handle;

        let mut target_profile = effective_profile.clone();
        ensure_password_credentials(&mut target_profile, app, tab_id, interaction_timeout).await?;

        crate::services::logging::session(
            app,
            "INFO",
            "ssh",
            tab_id,
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
                    remote_sshid.clone(),
                ),
                &host,
                port,
                connect_timeout,
                interaction_timeout,
            )
            .await?;
            if authenticate_session(
                &mut target_handle,
                &username,
                &auth_type,
                &target_profile,
                app,
                tab_id,
                SshAuthenticationTarget::Target,
            )
            .await?
            {
                Ok(OpenSshSession {
                    handle: target_handle,
                    remote_sshid: read_shared_remote_sshid(&remote_sshid),
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
                    remote_sshid.clone(),
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
        tab_id,
        authentication_target,
    )
    .await?
    {
        Ok(OpenSshSession {
            handle,
            remote_sshid: read_shared_remote_sshid(&remote_sshid),
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
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "connection test started");
    let session = match open_session(
        profile,
        app,
        tab_id,
        SSH_CONNECTION_TEST_INTERACTION_TIMEOUT,
        Some(interaction_window_label),
        SshAuthenticationTarget::Direct,
    )
    .await
    {
        Ok(handle) => handle,
        Err(error) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "ssh",
                tab_id,
                format!("connection test failed stage=open_session error={error}"),
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
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "connection test completed");
    Ok(())
}
