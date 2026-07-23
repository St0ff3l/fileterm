use base64::Engine;
use serde_json::Value;
use tauri::AppHandle;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_socks::tcp::Socks5Stream;

use super::telnet_direct::connect_direct_telnet;
use super::terminal::{decode_terminal, emit_terminal_data, encode_terminal, set_terminal_state};
use super::WorkerCmd;
use crate::services::WorkspaceTabStatus;

const IAC: u8 = 255;
const DONT: u8 = 254;
const DO: u8 = 253;
const WONT: u8 = 252;
const WILL: u8 = 251;
const SB: u8 = 250;
const SE: u8 = 240;
const TERMINAL_TYPE: u8 = 24;
const NAWS: u8 = 31;
/// Telnet 传输层连接（直连或经代理）整体超时。Telnet 服务器或代理无响应时，
/// `TcpStream::connect` 和 SOCKS5/HTTP CONNECT 握手都会永久 await，导致
/// 标签页卡在 connecting 状态无法重试。30s 与 SSH 侧 SSH_TRANSPORT_TIMEOUT 对齐。
const TELNET_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(30);
/// HTTP/SOCKS5 代理单步 IO 超时。代理服务器或中间网络卡住时，TCP 连接、
/// CONNECT 请求写入、响应逐字节读取都不能让外层 30s 超时全部消耗在
/// 单次 read 上——慢速代理可以每 29s 发一个字节拖满整个阶段。8s 覆盖
/// 正常代理 RTT，超时后立即给出明确错误。
const PROXY_IO_TIMEOUT: Duration = Duration::from_secs(8);

trait TelnetTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> TelnetTransport for T {}

#[derive(Clone, Copy)]
enum ParseState {
    Data,
    Iac,
    Option(u8),
    Subnegotiation,
    SubnegotiationIac,
}

struct TelnetParser {
    state: ParseState,
    subnegotiation: Vec<u8>,
    cols: u16,
    rows: u16,
}

impl TelnetParser {
    fn new() -> Self {
        Self {
            state: ParseState::Data,
            subnegotiation: Vec::new(),
            cols: 80,
            rows: 24,
        }
    }

    fn set_size(&mut self, cols: u32, rows: u32) -> Vec<u8> {
        self.cols = cols.clamp(1, u16::MAX as u32) as u16;
        self.rows = rows.clamp(1, u16::MAX as u32) as u16;
        self.naws()
    }

    fn naws(&self) -> Vec<u8> {
        vec![
            IAC,
            SB,
            NAWS,
            (self.cols >> 8) as u8,
            self.cols as u8,
            (self.rows >> 8) as u8,
            self.rows as u8,
            IAC,
            SE,
        ]
    }

    fn feed(&mut self, input: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut output = Vec::new();
        let mut writes = Vec::new();
        for byte in input {
            match self.state {
                ParseState::Data => {
                    if *byte == IAC {
                        self.state = ParseState::Iac;
                    } else {
                        output.push(*byte);
                    }
                }
                ParseState::Iac => match *byte {
                    IAC => {
                        output.push(IAC);
                        self.state = ParseState::Data;
                    }
                    DO | DONT | WILL | WONT => self.state = ParseState::Option(*byte),
                    SB => {
                        self.subnegotiation.clear();
                        self.state = ParseState::Subnegotiation;
                    }
                    _ => self.state = ParseState::Data,
                },
                ParseState::Option(command) => {
                    let supported = matches!(*byte, 0 | 1 | 3 | TERMINAL_TYPE | NAWS);
                    match command {
                        DO => {
                            writes.push(vec![IAC, if supported { WILL } else { WONT }, *byte]);
                            if *byte == NAWS {
                                writes.push(self.naws());
                            }
                        }
                        WILL => writes.push(vec![IAC, if supported { DO } else { DONT }, *byte]),
                        DONT => writes.push(vec![IAC, WONT, *byte]),
                        _ => writes.push(vec![IAC, DONT, *byte]),
                    }
                    self.state = ParseState::Data;
                }
                ParseState::Subnegotiation => {
                    if *byte == IAC {
                        self.state = ParseState::SubnegotiationIac;
                    } else {
                        self.subnegotiation.push(*byte);
                    }
                }
                ParseState::SubnegotiationIac => {
                    if *byte == SE {
                        if self.subnegotiation.first() == Some(&TERMINAL_TYPE)
                            && self.subnegotiation.get(1) == Some(&1)
                        {
                            let mut reply = vec![IAC, SB, TERMINAL_TYPE, 0];
                            reply.extend_from_slice(b"xterm-256color");
                            reply.extend_from_slice(&[IAC, SE]);
                            writes.push(reply);
                        }
                    } else if *byte == IAC {
                        self.subnegotiation.push(IAC);
                    }
                    self.state = ParseState::Subnegotiation;
                }
            }
        }
        (output, writes)
    }
}

pub fn start_telnet_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
) {
    crate::services::logging::session(&app, "INFO", "telnet", &tab_id, "worker starting");
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_telnet_worker(&tab_id, &profile, command_rx, &app).await {
            crate::services::logging::session(&app, "ERROR", "telnet", &tab_id, &error);
            emit_terminal_data(&app, &tab_id, &format!("\r\n[Telnet] {error}\r\n")).await;
            set_terminal_state(
                &app,
                &tab_id,
                format!("Telnet error: {error}"),
                WorkspaceTabStatus::Error,
            )
            .await;
        }
    });
}

async fn run_telnet_worker(
    tab_id: &str,
    profile: &Value,
    mut command_rx: mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or("127.0.0.1");
    let port = profile.get("port").and_then(Value::as_u64).unwrap_or(23) as u16;
    let encoding = profile
        .get("encoding")
        .and_then(Value::as_str)
        .unwrap_or("utf-8")
        .to_string();
    let stream = connect_transport(profile, host, port).await?;
    crate::services::logging::session(
        app,
        "INFO",
        "telnet",
        tab_id,
        format!("connected host={host} port={port}"),
    );
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut parser = TelnetParser::new();
    set_terminal_state(
        app,
        tab_id,
        format!("Telnet {host}:{port}"),
        WorkspaceTabStatus::Connected,
    )
    .await;
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;
    let mut buffer = vec![0_u8; 32 * 1024];

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(WorkerCmd::WriteTerminal(data)) => {
                        let mut bytes = encode_terminal(&data, &encoding);
                        let mut escaped = Vec::with_capacity(bytes.len());
                        for byte in bytes.drain(..) {
                            escaped.push(byte);
                            if byte == IAC { escaped.push(IAC); }
                        }
                        writer.write_all(&escaped).await.map_err(|error| error.to_string())?;
                    }
                    Some(WorkerCmd::ResizeTerminal { cols, rows, .. }) => {
                        writer.write_all(&parser.set_size(cols, rows)).await.map_err(|error| error.to_string())?;
                    }
                    Some(WorkerCmd::Disconnect) | None => {
                        crate::services::logging::session(app, "INFO", "telnet", tab_id, "disconnecting");
                        let _ = writer.shutdown().await;
                        set_terminal_state(app, tab_id, "Telnet disconnected".to_string(), WorkspaceTabStatus::Closed).await;
                        return Ok(());
                    }
                    Some(command) => reject_unsupported(command, "Telnet 不支持此文件或隧道操作"),
                }
            }
            read = reader.read(&mut buffer) => {
                let count = read.map_err(|error| error.to_string())?;
                if count == 0 {
                    crate::services::logging::session(app, "WARN", "telnet", tab_id, "remote closed connection");
                    set_terminal_state(app, tab_id, "Telnet disconnected".to_string(), WorkspaceTabStatus::Closed).await;
                    return Ok(());
                }
                let (visible, writes) = parser.feed(&buffer[..count]);
                for write in writes {
                    writer.write_all(&write).await.map_err(|error| error.to_string())?;
                }
                if !visible.is_empty() {
                    emit_terminal_data(app, tab_id, &decode_terminal(&visible, &encoding)).await;
                }
            }
        }
    }
}

async fn connect_transport(
    profile: &Value,
    host: &str,
    port: u16,
) -> Result<Box<dyn TelnetTransport>, String> {
    let proxy = profile.get("proxy").and_then(Value::as_object);
    let proxy_type = proxy
        .and_then(|proxy| proxy.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");
    if proxy_type == "none" {
        // 直连路径整体超时：Telnet 服务器无响应时 TcpStream::connect 会永久
        // await，标签页卡在 connecting 状态无法重试。
        return timeout(TELNET_TRANSPORT_TIMEOUT, connect_direct_telnet(host, port))
            .await
            .map_err(|_| {
                format!(
                    "Telnet connect timed out after {} seconds",
                    TELNET_TRANSPORT_TIMEOUT.as_secs()
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
        WorkerCmd::WriteTerminal(_) | WorkerCmd::ResizeTerminal { .. } | WorkerCmd::Disconnect => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{connect_transport, TelnetParser, DO, IAC, NAWS, SB, SE, WILL};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};

    async fn read_http_headers(socket: &mut tokio::net::TcpStream) -> String {
        let mut headers = Vec::new();
        let mut byte = [0_u8; 1];
        while !headers.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert_eq!(count, 1, "client closed before completing CONNECT headers");
            headers.push(byte[0]);
        }
        String::from_utf8(headers).unwrap()
    }

    async fn read_socks5_connect_request(socket: &mut tokio::net::TcpStream) -> (String, u16) {
        let mut greeting = [0_u8; 2];
        socket.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0_u8; greeting[1] as usize];
        socket.read_exact(&mut methods).await.unwrap();
        assert!(methods.contains(&0));
        socket.write_all(&[5, 0]).await.unwrap();

        let mut request = [0_u8; 4];
        socket.read_exact(&mut request).await.unwrap();
        assert_eq!(&request[..3], &[5, 1, 0]);
        let host = match request[3] {
            1 => {
                let mut address = [0_u8; 4];
                socket.read_exact(&mut address).await.unwrap();
                std::net::Ipv4Addr::from(address).to_string()
            }
            3 => {
                let mut length = [0_u8; 1];
                socket.read_exact(&mut length).await.unwrap();
                let mut hostname = vec![0_u8; length[0] as usize];
                socket.read_exact(&mut hostname).await.unwrap();
                String::from_utf8(hostname).unwrap()
            }
            other => panic!("unexpected SOCKS5 address type: {other}"),
        };
        let mut port = [0_u8; 2];
        socket.read_exact(&mut port).await.unwrap();
        (host, u16::from_be_bytes(port))
    }

    #[test]
    fn negotiates_naws_and_hides_iac_control_bytes() {
        let mut parser = TelnetParser::new();
        let (output, writes) = parser.feed(&[IAC, DO, NAWS, b'o', b'k']);
        assert_eq!(output, b"ok");
        assert_eq!(writes[0], vec![IAC, WILL, NAWS]);
        assert_eq!(writes[1], vec![IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]);
    }

    #[tokio::test]
    async fn direct_transport_drop_releases_socket_on_every_desktop_platform() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut byte = [0_u8; 1];
            socket.read(&mut byte).await.unwrap()
        });
        let profile = serde_json::json!({ "proxy": { "type": "none" } });
        let transport = connect_transport(&profile, "127.0.0.1", address.port())
            .await
            .unwrap();
        drop(transport);
        assert_eq!(
            timeout(Duration::from_secs(2), peer)
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn http_connect_proxy_reaches_a_real_telnet_peer_and_relays_bytes() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let request = read_http_headers(&mut client).await;
            assert!(request.starts_with(&format!(
                "CONNECT 127.0.0.1:{} HTTP/1.1\r\n",
                target_address.port()
            )));
            assert!(request.contains("Proxy-Authorization: Basic cHJveHktdXNlcjpwcm94eS1wYXNz\r\n"));
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(target_address)
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });

        let profile = serde_json::json!({
            "proxy": {
                "type": "http",
                "host": "127.0.0.1",
                "port": proxy_address.port(),
                "username": "proxy-user",
                "password": "proxy-pass"
            }
        });
        let mut transport = connect_transport(&profile, "127.0.0.1", target_address.port())
            .await
            .unwrap();
        transport.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        transport.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(transport);

        target.await.unwrap();
        proxy.await.unwrap();
    }

    #[tokio::test]
    async fn socks5_proxy_reaches_a_real_telnet_peer_and_relays_bytes() {
        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });

        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let (host, port) = read_socks5_connect_request(&mut client).await;
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, target_address.port());
            client
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(target_address)
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });

        let profile = serde_json::json!({
            "proxy": {
                "type": "socks5",
                "host": "127.0.0.1",
                "port": proxy_address.port()
            }
        });
        let mut transport = connect_transport(&profile, "127.0.0.1", target_address.port())
            .await
            .unwrap();
        transport.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        transport.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(transport);

        target.await.unwrap();
        proxy.await.unwrap();
    }
}
