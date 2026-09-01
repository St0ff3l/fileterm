async fn connect_ftp(profile: &Value, host: &str, port: u16) -> Result<FtpClient, String> {
    let expected_fingerprint = ftp_certificate_fingerprint_from_profile(profile)?;
    if let Some(expected_fingerprint) = expected_fingerprint {
        verify_ftp_certificate_pin(profile, host, port, &expected_fingerprint).await?;
    }
    connect_ftp_with_tls_connector(
        profile,
        host,
        port,
        AsyncNativeTlsConnector::from(suppaftp::async_native_tls::TlsConnector::new()),
    )
    .await
}

/// Verify the FTP/FTPS transport and credentials without opening a workspace
/// session or listing the configured remote directory.
pub async fn test_connection(profile: &Value) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "FTP host is required".to_string())?;
    let port = port_from_profile(profile, 21, "FTP")?;
    let mut client = connect_ftp(profile, host, port).await?;
    let _ = timeout(DEFAULT_FTP_OPERATION_TIMEOUT, client_quit(&mut client)).await;
    Ok(())
}

/// Connect an FTP client with an injected TLS connector.
///
/// Production always supplies the platform-default validating connector above.
/// Keeping the connector at this boundary lets the real FTPS fixture exercise
/// explicit and implicit data channels with a test-only self-signed identity,
/// without weakening the application's certificate verification policy.
async fn connect_ftp_with_tls_connector(
    profile: &Value,
    host: &str,
    port: u16,
    tls_connector: AsyncNativeTlsConnector,
) -> Result<FtpClient, String> {
    let username = profile
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("anonymous");
    let password = profile
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("anonymous@");
    let mode = profile
        .get("securityMode")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if profile
                .get("secure")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "explicit"
            } else {
                "none"
            }
        });
    let connect_timeout = seconds_from_profile(
        profile,
        "connectTimeoutSeconds",
        DEFAULT_FTP_CONNECT_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(300),
    );
    let proxy_type = profile
        .get("proxy")
        .and_then(Value::as_object)
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    let transfer_mode = profile
        .get("transferMode")
        .and_then(Value::as_str)
        .unwrap_or("passive");
    if proxy_type != "none" && transfer_mode == "active" {
        return Err(
            "FTP active mode cannot accept the server's data connection through a proxy; use passive mode"
                .to_string(),
        );
    }

    timeout(
        connect_timeout,
        async {
            match mode {
                "none" => {
                    let stream = connect_ftp_tcp(profile, host, port).await?;
                    let mut client = configure_ftp_data_transport(
                        AsyncFtpStream::connect_with_stream(stream)
                            .await
                            .map_err(|error| error.to_string())?,
                        profile,
                    )?;
                    configure_ftp_mode(&mut client, profile);
                    client
                        .login(username, password)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(FtpClient::Plain(client))
                }
                "explicit" => {
                    // `into_secure` needs a stream typed for the TLS backend up front; using
                    // the no-TLS alias here makes the generic stream types incompatible.
                    let stream = connect_ftp_tcp(profile, host, port).await?;
                    let client = configure_ftp_data_transport(
                        AsyncNativeTlsFtpStream::connect_with_stream(stream)
                            .await
                            .map_err(|error| error.to_string())?,
                        profile,
                    )?;
                    let mut client = client
                        .into_secure(tls_connector, host)
                        .await
                        .map_err(|error| error.to_string())?;
                    configure_ftp_mode(&mut client, profile);
                    client
                        .login(username, password)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(FtpClient::Secure(client))
                }
                "implicit" => {
                    if proxy_type != "none" {
                        return Err(
                            "FTP implicit FTPS currently requires a direct connection; use explicit FTPS with a proxy"
                                .to_string(),
                        );
                    }
                    let mut client = AsyncNativeTlsFtpStream::connect_secure_implicit(
                        (host, port),
                        tls_connector,
                        host,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                    configure_ftp_mode(&mut client, profile);
                    client
                        .login(username, password)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(FtpClient::Secure(client))
                }
                other => Err(format!("Unsupported FTP security mode: {other}")),
            }
        },
    )
    .await
    .map_err(|_| {
        format!(
            "FTP connect/login timed out after {} seconds",
            connect_timeout.as_secs()
        )
    })?
}

/// Verify an FTPS leaf certificate before the real FTP client sends login
/// credentials. `suppaftp` does not expose the command channel's TLS stream,
/// so a short, separately closed TLS probe is used when a pin is configured.
/// The probe follows AUTH TLS for explicit FTPS and uses a direct TLS
/// handshake for implicit FTPS; the actual connection still performs normal
/// system trust-store validation afterwards.
async fn verify_ftp_certificate_pin(
    profile: &Value,
    host: &str,
    port: u16,
    expected: &str,
) -> Result<(), String> {
    let mode = profile
        .get("securityMode")
        .and_then(Value::as_str)
        .unwrap_or("explicit");
    let connect_timeout = seconds_from_profile(
        profile,
        "connectTimeoutSeconds",
        DEFAULT_FTP_CONNECT_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(300),
    );
    timeout(connect_timeout, async {
        let stream = connect_ftp_tcp(profile, host, port).await?;
        let stream = if mode == "explicit" {
            let mut reader = BufReader::new(stream);
            let mut greeting = read_ftp_response_code(&mut reader).await?;
            while (100..200).contains(&greeting) {
                greeting = read_ftp_response_code(&mut reader).await?;
            }
            if !(200..400).contains(&greeting) {
                return Err(format!("FTP server rejected the greeting ({greeting})"));
            }
            reader
                .get_mut()
                .write_all(b"AUTH TLS\r\n")
                .await
                .map_err(|error| format!("FTP AUTH TLS write failed: {error}"))?;
            let response = read_ftp_response_code(&mut reader).await?;
            if response != 234 && response != 334 {
                return Err(format!("FTP server rejected AUTH TLS ({response})"));
            }
            reader.into_inner()
        } else {
            stream
        };
        let connector = suppaftp::async_native_tls::TlsConnector::new();
        let mut tls_stream = connector
            .connect(host, stream)
            .await
            .map_err(|error| format!("FTPS certificate probe failed: {error}"))?;
        let certificate = tls_stream
            .peer_certificate()
            .map_err(|error| format!("FTPS certificate probe failed: {error}"))?
            .ok_or_else(|| "FTPS server did not provide a peer certificate".to_string())?;
        let der = certificate
            .to_der()
            .map_err(|error| format!("FTPS certificate read failed: {error}"))?;
        let actual = ftp_certificate_fingerprint(&der);
        let result = if actual == expected {
            Ok(())
        } else {
            Err(format!(
                "FTPS certificate fingerprint mismatch (expected {expected}, got {actual})"
            ))
        };
        let _ = tls_stream.shutdown().await;
        result
    })
    .await
    .map_err(|_| {
        format!(
            "FTPS certificate probe timed out after {} seconds",
            connect_timeout.as_secs()
        )
    })?
}

async fn read_ftp_response_code(reader: &mut BufReader<TcpStream>) -> Result<u16, String> {
    let mut first_code = None;
    let mut response_bytes = 0_usize;
    loop {
        let mut line = Vec::new();
        let read = reader
            .read_until(b'\n', &mut line)
            .await
            .map_err(|error| format!("FTP response read failed: {error}"))?;
        if read == 0 {
            return Err("FTP server closed during certificate probe".to_string());
        }
        response_bytes = response_bytes.saturating_add(read);
        if response_bytes > 32 * 1024 {
            return Err("FTP response exceeded 32 KiB during certificate probe".to_string());
        }
        if line.len() < 3 || !line[..3].iter().all(u8::is_ascii_digit) {
            return Err(
                "FTP server returned a malformed response during certificate probe".to_string(),
            );
        }
        let code = u16::from(line[0] - b'0') * 100
            + u16::from(line[1] - b'0') * 10
            + u16::from(line[2] - b'0');
        match (first_code, line.get(3).copied()) {
            (None, Some(b' ')) => return Ok(code),
            (None, Some(b'-')) => first_code = Some(code),
            (Some(expected), Some(b' ')) if expected == code => return Ok(code),
            _ => {}
        }
    }
}

fn ftp_certificate_fingerprint(der: &[u8]) -> String {
    format_ftp_digest(&Sha256::digest(der))
}

fn format_ftp_digest(digest: &[u8]) -> String {
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sha256:{hex}")
}

fn ftp_certificate_fingerprint_from_profile(profile: &Value) -> Result<Option<String>, String> {
    let mode = profile
        .get("securityMode")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if profile
                .get("secure")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "explicit"
            } else {
                "none"
            }
        });
    if mode == "none" {
        return Ok(None);
    }
    let Some(value) = profile
        .get("certificateFingerprint")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    normalize_ftp_certificate_fingerprint(value).map(Some)
}

fn normalize_ftp_certificate_fingerprint(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let payload = trimmed
        .get(7..)
        .filter(|_| trimmed[..7].eq_ignore_ascii_case("sha256:"))
        .unwrap_or(trimmed);
    let compact = payload
        .chars()
        .filter(|character| !character.is_ascii_whitespace() && *character != ':')
        .collect::<String>();
    if compact.len() == 64 && compact.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(format!("sha256:{}", compact.to_ascii_lowercase()));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|_| "FTPS certificate fingerprint must be SHA-256 hex or Base64".to_string())?;
    if decoded.len() != 32 {
        return Err("FTPS certificate fingerprint must contain 32 digest bytes".to_string());
    }
    Ok(format_ftp_digest(&decoded))
}

async fn connect_ftp_tcp(profile: &Value, host: &str, port: u16) -> Result<TcpStream, String> {
    let proxy = profile.get("proxy").and_then(Value::as_object);
    let proxy_type = proxy
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    if proxy_type == "none" {
        return TcpStream::connect((host, port))
            .await
            .map_err(|error| format!("FTP connect failed: {error}"));
    }

    validate_ftp_proxy_value(host, "FTP target host")?;
    let proxy_host = proxy
        .and_then(|proxy| proxy.get("host"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "FTP proxy host is required".to_string())?;
    validate_ftp_proxy_value(proxy_host, "FTP proxy host")?;
    let proxy_port = proxy
        .and_then(|proxy| proxy.get("port"))
        .and_then(Value::as_u64)
        .filter(|value| (1..=u16::MAX as u64).contains(value))
        .ok_or_else(|| "FTP proxy port must be between 1 and 65535".to_string())?
        as u16;
    let username = proxy
        .and_then(|proxy| proxy.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let password = proxy
        .and_then(|proxy| proxy.get("password"))
        .and_then(Value::as_str)
        .unwrap_or("");
    validate_ftp_proxy_value(username, "FTP proxy username")?;
    validate_ftp_proxy_value(password, "FTP proxy password")?;

    match proxy_type {
        "socks5" => {
            let stream = if username.is_empty() {
                timeout(
                    FTP_PROXY_IO_TIMEOUT,
                    Socks5Stream::connect((proxy_host, proxy_port), (host, port)),
                )
                .await
                .map_err(|_| "FTP SOCKS5 proxy connect timed out".to_string())?
                .map_err(|error| format!("FTP SOCKS5 proxy connect failed: {error}"))?
            } else {
                timeout(
                    FTP_PROXY_IO_TIMEOUT,
                    Socks5Stream::connect_with_password(
                        (proxy_host, proxy_port),
                        (host, port),
                        username,
                        password,
                    ),
                )
                .await
                .map_err(|_| "FTP SOCKS5 proxy authentication timed out".to_string())?
                .map_err(|error| format!("FTP SOCKS5 proxy authentication failed: {error}"))?
            };
            Ok(stream.into_inner())
        }
        "http" => {
            connect_ftp_http_proxy(proxy_host, proxy_port, host, port, username, password).await
        }
        other => Err(format!("Unsupported FTP proxy type: {other}")),
    }
}

fn validate_ftp_proxy_value(value: &str, label: &str) -> Result<(), String> {
    if value.len() > 255 {
        return Err(format!("{label} is too long (max 255 bytes)"));
    }
    if value.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return Err(format!("{label} contains control characters"));
    }
    Ok(())
}

async fn connect_ftp_http_proxy(
    proxy_host: &str,
    proxy_port: u16,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<TcpStream, String> {
    let mut stream = timeout(
        FTP_PROXY_IO_TIMEOUT,
        TcpStream::connect((proxy_host, proxy_port)),
    )
    .await
    .map_err(|_| "FTP HTTP proxy connect timed out".to_string())?
    .map_err(|error| format!("FTP HTTP proxy connect failed: {error}"))?;
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
    timeout(FTP_PROXY_IO_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| "FTP HTTP proxy CONNECT write timed out".to_string())?
        .map_err(|error| format!("FTP HTTP proxy CONNECT write failed: {error}"))?;

    let mut response = Vec::with_capacity(1024);
    let mut byte = [0_u8; 1];
    while !response.windows(4).any(|window| window == b"\r\n\r\n") {
        if response.len() >= 32 * 1024 {
            return Err("FTP HTTP proxy response headers are too large".to_string());
        }
        let read = timeout(FTP_PROXY_IO_TIMEOUT, stream.read(&mut byte))
            .await
            .map_err(|_| "FTP HTTP proxy CONNECT read timed out".to_string())?
            .map_err(|error| format!("FTP HTTP proxy CONNECT read failed: {error}"))?;
        if read == 0 {
            return Err("FTP HTTP proxy closed before CONNECT completed".to_string());
        }
        response.extend_from_slice(&byte[..read]);
    }
    let status_line = std::str::from_utf8(&response)
        .map_err(|_| "FTP HTTP proxy returned a non-text response".to_string())?
        .lines()
        .next()
        .unwrap_or("");
    let mut parts = status_line.split_whitespace();
    let version = parts.next().unwrap_or("");
    let code = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") || code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(format!(
            "FTP HTTP proxy returned a malformed status line: {status_line}"
        ));
    }
    if code != "200" {
        return Err(format!("FTP HTTP CONNECT failed: {status_line}"));
    }
    Ok(stream)
}

fn configure_ftp_data_transport<T: TokioTlsStream + Send>(
    client: ImplAsyncFtpStream<T>,
    profile: &Value,
) -> Result<ImplAsyncFtpStream<T>, String> {
    let proxy_type = profile
        .get("proxy")
        .and_then(Value::as_object)
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    if proxy_type == "none" {
        return Ok(client);
    }

    let proxy_profile = profile.clone();
    Ok(client.passive_stream_builder(move |target: SocketAddr| {
        let profile = proxy_profile.clone();
        Box::pin(async move {
            let target_host = target.ip().to_string();
            connect_ftp_tcp(&profile, &target_host, target.port())
                .await
                .map_err(|error| FtpError::ConnectionError(std::io::Error::other(error)))
        })
    }))
}

fn configure_ftp_mode<T: TokioTlsStream + Send>(ftp: &mut ImplAsyncFtpStream<T>, profile: &Value) {
    let mode = match profile
        .get("transferMode")
        .and_then(Value::as_str)
        .unwrap_or("passive")
    {
        "active" => Mode::Active,
        _ => Mode::Passive,
    };
    ftp.set_mode(mode);
}

async fn ftp_with_timeout<T, F>(profile: &Value, operation: &str, future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    let operation_timeout = seconds_from_profile(
        profile,
        "operationTimeoutSeconds",
        DEFAULT_FTP_OPERATION_TIMEOUT,
        Duration::from_secs(5),
        Duration::from_secs(3600),
    );
    timeout(operation_timeout, future).await.map_err(|_| {
        format!(
            "FTP {operation} timed out after {} seconds",
            operation_timeout.as_secs()
        )
    })?
}

async fn ftp_with_cancellation<T, F>(
    profile: &Value,
    operation: &str,
    cancellation: CancellationToken,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tokio::select! {
        _ = cancellation.cancelled() => Err("远程文件操作已取消".to_string()),
        result = ftp_with_timeout(profile, operation, future) => result,
    }
}

async fn ftp_io_with_timeout<T, E, F>(
    duration: Duration,
    operation: &str,
    future: F,
) -> Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    timeout(duration, future)
        .await
        .map_err(|_| {
            format!(
                "FTP {operation} timed out after {} seconds",
                duration.as_secs()
            )
        })?
        .map_err(|error| error.to_string())
}
