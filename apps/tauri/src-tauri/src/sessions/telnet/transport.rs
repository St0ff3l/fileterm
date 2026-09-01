fn parse_login_script(script: &str) -> Vec<String> {
    script
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .map(ToOwned::to_owned)
        .take(64)
        .collect()
}

async fn write_telnet<W>(writer: &mut W, bytes: &[u8]) -> Result<(), String>
where
    W: AsyncWrite + Unpin,
{
    timeout(TELNET_WRITE_TIMEOUT, writer.write_all(bytes))
        .await
        .map_err(|_| {
            format!(
                "Telnet write timed out after {} seconds",
                TELNET_WRITE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| error.to_string())
}

async fn connect_transport(
    profile: &Value,
    host: &str,
    port: u16,
) -> Result<Box<dyn TelnetTransport>, String> {
    let connect_timeout = seconds_from_profile(
        profile,
        "connectTimeoutSeconds",
        TELNET_TRANSPORT_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(300),
    );
    let proxy = profile.get("proxy").and_then(Value::as_object);
    let proxy_type = proxy
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    if proxy_type == "none" {
        // 直连路径整体超时：Telnet 服务器无响应时 TcpStream::connect 会永久
        // await，标签页卡在 connecting 状态无法重试。
        return timeout(connect_timeout, connect_direct_telnet(host, port))
            .await
            .map_err(|_| {
                format!(
                    "Telnet connect timed out after {} seconds",
                    connect_timeout.as_secs()
                )
            })?
            .map(|stream| Box::new(stream) as Box<dyn TelnetTransport>);
    }
    let proxy_host = proxy
        .and_then(|proxy| proxy.get("host"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Telnet proxy host is required".to_string())?;
    validate_proxy_host(proxy_host)?;
    let proxy_port = proxy
        .and_then(|proxy| proxy.get("port"))
        .and_then(Value::as_u64)
        .filter(|value| (1..=u16::MAX as u64).contains(value))
        .ok_or_else(|| "Telnet proxy port is invalid".to_string())? as u16;
    let username = proxy
        .and_then(|proxy| proxy.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let password = proxy
        .and_then(|proxy| proxy.get("password"))
        .and_then(Value::as_str)
        .unwrap_or("");
    validate_proxy_credentials(username, password)?;

    timeout(connect_timeout, async {
        match proxy_type {
            "socks5" if username.is_empty() => {
                let stream = timeout(
                    PROXY_IO_TIMEOUT,
                    Socks5Stream::connect((proxy_host, proxy_port), (host, port)),
                )
                .await
                .map_err(|_| "Telnet SOCKS5 proxy connect timed out".to_string())?
                .map_err(|error| format!("Telnet SOCKS5 proxy connect failed: {error}"))?;
                Ok(Box::new(stream) as Box<dyn TelnetTransport>)
            }
            "socks5" => {
                let stream = timeout(
                    PROXY_IO_TIMEOUT,
                    Socks5Stream::connect_with_password(
                        (proxy_host, proxy_port),
                        (host, port),
                        username,
                        password,
                    ),
                )
                .await
                .map_err(|_| "Telnet SOCKS5 proxy authentication timed out".to_string())?
                .map_err(|error| format!("Telnet SOCKS5 proxy authentication failed: {error}"))?;
                Ok(Box::new(stream) as Box<dyn TelnetTransport>)
            }
            "http" => connect_http_proxy(proxy_host, proxy_port, host, port, username, password)
                .await
                .map(|stream| Box::new(stream) as Box<dyn TelnetTransport>),
            other => Err(format!("Unsupported Telnet proxy type: {other}")),
        }
    })
    .await
    .map_err(|_| {
        format!(
            "Telnet proxy connect timed out after {} seconds",
            connect_timeout.as_secs()
        )
    })?
}

/// Verify that the Telnet transport (including its configured proxy) accepts
/// a connection. Telnet has no standard authentication handshake; any login
/// script is intentionally left for the interactive session.
pub async fn test_connection(profile: &Value) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Telnet host is required".to_string())?;
    let port = port_from_profile(profile, 23, "Telnet")?;
    let _transport = connect_transport(profile, host, port).await?;
    Ok(())
}

/// 校验代理主机名：拒绝控制字符（含 CRLF，防止 HTTP CONNECT 头注入；
/// SOCKS5 虽是二进制协议，但控制字符 host 对任何代理都是非法输入），
/// 拒绝超长 host（RFC 1035 限制 253 字符，留余量到 255）。
fn validate_proxy_host(host: &str) -> Result<(), String> {
    if host.len() > 255 {
        return Err("Telnet proxy host is too long (max 255 characters)".to_string());
    }
    if host.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err("Telnet proxy host contains control characters".to_string());
    }
    Ok(())
}

/// 校验代理凭据：SOCKS5 用户名/密码认证（RFC 1929）限制各 255 字节；
/// 控制字符检查防止 HTTP CONNECT 头注入（connect_http_proxy 已检查
/// CRLF，这里作为纵深防御覆盖 SOCKS5 路径）。
fn validate_proxy_credentials(username: &str, password: &str) -> Result<(), String> {
    for (field, label) in [(username, "username"), (password, "password")] {
        if field.len() > 255 {
            return Err(format!(
                "Telnet proxy {} is too long (max 255 bytes, RFC 1929)",
                label
            ));
        }
        if field.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(format!(
                "Telnet proxy {} contains control characters",
                label
            ));
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
    if [host, username, password]
        .iter()
        .any(|value| value.contains(['\r', '\n']))
    {
        return Err("Telnet proxy values must not contain line breaks".to_string());
    }
    let mut stream = timeout(
        PROXY_IO_TIMEOUT,
        TcpStream::connect((proxy_host, proxy_port)),
    )
    .await
    .map_err(|_| {
        format!(
            "Telnet HTTP proxy connect timed out after {} seconds",
            PROXY_IO_TIMEOUT.as_secs()
        )
    })?
    .map_err(|error| format!("Telnet HTTP proxy connect failed: {error}"))?;
    let _ = stream.set_nodelay(true);
    let authority = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut request = format!(
        "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n"
    );
    if !username.is_empty() {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    request.push_str("\r\n");
    timeout(PROXY_IO_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| "Telnet HTTP proxy CONNECT write timed out".to_string())?
        .map_err(|error| format!("Telnet HTTP proxy CONNECT write failed: {error}"))?;
    let mut response = Vec::new();
    // Read one byte at a time until the HTTP header boundary. Reading a larger
    // chunk could consume the first Telnet bytes sent immediately after a
    // successful CONNECT; TcpStream cannot push those bytes back for the
    // terminal reader.
    let mut chunk = [0_u8; 1];
    while !response.windows(4).any(|window| window == b"\r\n\r\n") {
        if response.len() >= 32 * 1024 {
            return Err("Telnet proxy response headers are too large".to_string());
        }
        let count = timeout(PROXY_IO_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| "Telnet HTTP proxy CONNECT read timed out".to_string())?
            .map_err(|error| format!("Telnet HTTP proxy CONNECT read failed: {error}"))?;
        if count == 0 {
            return Err("Telnet proxy closed before CONNECT completed".to_string());
        }
        response.extend_from_slice(&chunk[..count]);
    }
    let status_line = std::str::from_utf8(&response)
        .map_err(|_| "Telnet proxy returned a non-text response".to_string())?
        .lines()
        .next()
        .unwrap_or("");
    let status = parse_http_connect_status(status_line)?;
    if status != 200 {
        return Err(format!("Telnet HTTP CONNECT failed: {status_line}"));
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
            "Telnet proxy returned a malformed status line: {status_line}"
        ));
    }
    let code = parts.next().unwrap_or("");
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "Telnet proxy returned a malformed status code: {status_line}"
        ));
    }
    code.parse::<u16>()
        .map_err(|_| format!("Telnet proxy returned an invalid status code: {status_line}"))
}

pub(crate) fn reject_unsupported(command: WorkerCmd, message: &str) {
    match command {
        WorkerCmd::ListRemoteFiles { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::ReadRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::ExecuteRemoteCommand { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::WriteRemoteFile { respond_to, .. }
        | WorkerCmd::CreateRemoteDirectory { respond_to, .. }
        | WorkerCmd::CreateRemoteFile { respond_to, .. }
        | WorkerCmd::CopyRemotePath { respond_to, .. }
        | WorkerCmd::MoveRemotePath { respond_to, .. }
        | WorkerCmd::RenameRemotePath { respond_to, .. }
        | WorkerCmd::DeleteRemotePath { respond_to, .. }
        | WorkerCmd::ChangeRemotePermissions { respond_to, .. }
        | WorkerCmd::SetRemoteFileAccessMode { respond_to, .. }
        | WorkerCmd::UploadLocalFile { respond_to, .. }
        | WorkerCmd::DownloadRemoteFile { respond_to, .. }
        | WorkerCmd::ReplaceRemoteFile { respond_to, .. }
        | WorkerCmd::CommitRemoteStaging { respond_to, .. }
        | WorkerCmd::RemoveRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::StatRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::ListSshTunnels { respond_to }
        | WorkerCmd::CreateSshTunnel { respond_to, .. }
        | WorkerCmd::StartSshTunnel { respond_to, .. }
        | WorkerCmd::StopSshTunnel { respond_to, .. }
        | WorkerCmd::DeleteSshTunnel { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::SerialControl { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::SerialTransfer { respond_to, .. } => {
            let _ = respond_to.send(Err(message.to_string()));
        }
        WorkerCmd::WriteTerminal(_) | WorkerCmd::ResizeTerminal { .. } | WorkerCmd::Disconnect => {}
    }
}
