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
