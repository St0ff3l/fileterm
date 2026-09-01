fn remote_bind_host_matches(bind_host: &str, connected_address: &str) -> bool {
    bind_host == connected_address || matches!(bind_host, "0.0.0.0" | "::" | "*")
}

fn effective_remote_forward_port(requested_port: u16, returned_port: u32) -> Result<u32, String> {
    if requested_port != 0 {
        return Ok(u32::from(requested_port));
    }
    if returned_port == 0 || returned_port > u32::from(u16::MAX) {
        return Err(format!(
            "SSH server returned an invalid remote forward port: {returned_port}"
        ));
    }
    Ok(returned_port)
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SshTunnelRule {
    id: String,
    #[serde(default)]
    name: String,
    kind: String,
    bind_host: String,
    bind_port: u16,
    #[serde(default)]
    target_host: Option<String>,
    #[serde(default)]
    target_port: Option<u16>,
    #[serde(default)]
    auto_start: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SshTunnelSnapshot {
    #[serde(flatten)]
    rule: SshTunnelRule,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    runtime_only: bool,
}

struct TunnelManager {
    tab_id: String,
    app: AppHandle,
    handle: Arc<Handle<ClientHandler>>,
    tunnels: HashMap<String, SshTunnelSnapshot>,
    local_stops: HashMap<String, oneshot::Sender<()>>,
    remote_rules: HashMap<String, (String, u32)>,
}

/// Tunnel operations use a dedicated FIFO worker instead of borrowing the
/// SSH session's main select loop. Starting or stopping a remote tunnel can
/// legitimately wait for the server's global-request reply; that wait must
/// never delay terminal input or SIGINT handling.
enum TunnelCommand {
    List {
        respond_to: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    Create {
        rule: SshTunnelRule,
        respond_to: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    Start {
        rule_id: String,
        respond_to: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    Stop {
        rule_id: String,
        respond_to: oneshot::Sender<Result<Vec<Value>, String>>,
    },
    Delete {
        rule_id: String,
        respond_to: oneshot::Sender<Result<Vec<Value>, String>>,
    },
}

impl TunnelCommand {
    fn reject(self, error: &str) {
        let respond_to = match self {
            Self::List { respond_to }
            | Self::Create { respond_to, .. }
            | Self::Start { respond_to, .. }
            | Self::Stop { respond_to, .. }
            | Self::Delete { respond_to, .. } => respond_to,
        };
        let _ = respond_to.send(Err(error.to_string()));
    }
}

fn enqueue_tunnel_command(sender: &mpsc::UnboundedSender<TunnelCommand>, command: TunnelCommand) {
    if let Err(error) = sender.send(command) {
        error.0.reject("SSH tunnel worker stopped");
    }
}

async fn run_tunnel_command_loop(
    mut tunnel_manager: TunnelManager,
    mut command_rx: mpsc::UnboundedReceiver<TunnelCommand>,
) {
    while let Some(command) = command_rx.recv().await {
        match command {
            TunnelCommand::List { respond_to } => {
                let _ = respond_to.send(tunnel_manager.list());
            }
            TunnelCommand::Create { rule, respond_to } => {
                let _ = respond_to.send(tunnel_manager.create(rule).await);
            }
            TunnelCommand::Start {
                rule_id,
                respond_to,
            } => {
                let _ = respond_to.send(tunnel_manager.start(&rule_id).await);
            }
            TunnelCommand::Stop {
                rule_id,
                respond_to,
            } => {
                let _ = respond_to.send(tunnel_manager.stop(&rule_id).await);
            }
            TunnelCommand::Delete {
                rule_id,
                respond_to,
            } => {
                let _ = respond_to.send(tunnel_manager.delete(&rule_id).await);
            }
        }
    }

    // The SSH worker owns the only sender. Once it exits, finish tunnel
    // cleanup in this isolated task so disconnecting can never pin terminal
    // input behind a remote cancel request.
    tunnel_manager.stop_all().await;
}

impl TunnelManager {
    fn new(tab_id: &str, app: &AppHandle, handle: Arc<Handle<ClientHandler>>) -> Self {
        Self {
            tab_id: tab_id.to_string(),
            app: app.clone(),
            handle,
            tunnels: HashMap::new(),
            local_stops: HashMap::new(),
            remote_rules: HashMap::new(),
        }
    }

    fn list(&self) -> Result<Vec<Value>, String> {
        let mut tunnels = self
            .tunnels
            .values()
            .cloned()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        tunnels.sort_by(|left, right| {
            left["name"]
                .as_str()
                .unwrap_or("")
                .cmp(right["name"].as_str().unwrap_or(""))
        });
        Ok(tunnels)
    }

    fn register(&mut self, rule: SshTunnelRule, runtime_only: bool) -> Result<(), String> {
        validate_tunnel_rule(&rule)?;
        if let Some(existing) = self.tunnels.get(&rule.id) {
            if existing.status == "running" || existing.status == "starting" {
                return Err(format!("Tunnel {} is already running", rule.id));
            }
        }
        let conflict = self.tunnels.values().any(|existing| {
            existing.rule.id != rule.id
                && (existing.rule.kind == "remote") == (rule.kind == "remote")
                && existing.rule.bind_host == rule.bind_host
                && existing.rule.bind_port == rule.bind_port
        });
        if conflict {
            return Err(format!(
                "Tunnel {}:{} is already configured",
                rule.bind_host, rule.bind_port
            ));
        }
        self.tunnels.insert(
            rule.id.clone(),
            SshTunnelSnapshot {
                rule,
                status: "stopped".to_string(),
                error: None,
                runtime_only,
            },
        );
        Ok(())
    }

    async fn create(&mut self, rule: SshTunnelRule) -> Result<Vec<Value>, String> {
        self.register(rule.clone(), true)?;
        self.start(&rule.id).await?;
        self.list()
    }

    async fn start(&mut self, rule_id: &str) -> Result<Vec<Value>, String> {
        if self.local_stops.contains_key(rule_id) || self.remote_rules.contains_key(rule_id) {
            return self.list();
        }
        let rule = self
            .tunnels
            .get(rule_id)
            .map(|snapshot| snapshot.rule.clone())
            .ok_or_else(|| format!("Tunnel {rule_id} was not found"))?;
        validate_tunnel_rule(&rule)?;
        self.set_status(rule_id, "starting", None);

        let start_result = if rule.kind == "remote" {
            self.start_remote(&rule).await
        } else {
            self.start_local_or_dynamic(&rule).await
        };
        match start_result {
            Ok(()) => {
                self.set_status(rule_id, "running", None);
                crate::services::logging::session(
                    &self.app,
                    "INFO",
                    "tunnel",
                    &self.tab_id,
                    format!("started id={rule_id} kind={}", rule.kind),
                );
                self.list()
            }
            Err(error) => {
                self.set_status(rule_id, "error", Some(error.clone()));
                crate::services::logging::session(
                    &self.app,
                    "ERROR",
                    "tunnel",
                    &self.tab_id,
                    format!("start failed id={rule_id} error={error}"),
                );
                Err(error)
            }
        }
    }

    async fn start_local_or_dynamic(&mut self, rule: &SshTunnelRule) -> Result<(), String> {
        let listener = TcpListener::bind(tunnel_bind_address(&rule.bind_host, rule.bind_port)?)
            .await
            .map_err(|error| {
                format!(
                    "Tunnel listen failed on {}:{}: {error}",
                    rule.bind_host, rule.bind_port
                )
            })?;
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let handle = Arc::clone(&self.handle);
        let rule = rule.clone();
        let rule_id = rule.id.clone();
        let tab_id = self.tab_id.clone();
        let app = self.app.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => match accepted {
                        Ok((socket, _peer)) => {
                            let handle = Arc::clone(&handle);
                            let rule = rule.clone();
                            let connection_tab_id = tab_id.clone();
                            let connection_app = app.clone();
                            tokio::spawn(async move {
                                let result = if rule.kind == "dynamic" {
                                    forward_socks5_connection(socket, handle).await
                                } else {
                                    forward_local_connection(socket, handle, &rule).await
                                };
                                if let Err(error) = result {
                                    crate::services::logging::session(&connection_app, "WARN", "tunnel", &connection_tab_id, format!("connection failed id={} error={error}", rule.id));
                                }
                            });
                        }
                        Err(error) => {
                            crate::services::logging::session(&app, "ERROR", "tunnel", &tab_id, format!("listener failed id={} error={error}", rule.id));
                            break;
                        }
                    }
                }
            }
        });
        self.local_stops.insert(rule_id, stop_tx);
        Ok(())
    }

    async fn start_remote(&mut self, rule: &SshTunnelRule) -> Result<(), String> {
        // 加 timeout：tcpip_forward 在 inline await 路径上，服务器卡住会
        // 阻塞 worker 主循环，导致终端 select! 无法响应 Ctrl+C。
        let returned_port = timeout(
            SSH_TUNNEL_OP_TIMEOUT,
            self.handle
                .tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32),
        )
        .await
        .map_err(|_| {
            "Remote tunnel request timed out: 服务器未在 5 秒内响应 tcpip_forward".to_string()
        })?
        .map_err(|error| format!("Remote tunnel request failed: {error}"))?;
        // RFC 4254 only returns an allocated port when the client requests
        // port 0. russh represents a successful fixed-port reply as 0 because
        // OpenSSH sends REQUEST_SUCCESS without a payload. Keep the requested
        // port for lookup and cancellation in that case.
        let actual_port = effective_remote_forward_port(rule.bind_port, returned_port)?;
        let target = crate::services::workspace::RemoteForwardTarget {
            bind_host: rule.bind_host.clone(),
            bind_port: actual_port,
            target_host: rule.target_host.clone().unwrap_or_default(),
            target_port: rule.target_port.unwrap_or_default(),
        };
        let state = self
            .app
            .state::<crate::services::workspace::WorkspaceState>();
        state
            .remote_forwards
            .write()
            .await
            .entry(self.tab_id.clone())
            .or_default()
            .push(target);
        self.remote_rules
            .insert(rule.id.clone(), (rule.bind_host.clone(), actual_port));
        Ok(())
    }

    async fn stop(&mut self, rule_id: &str) -> Result<Vec<Value>, String> {
        if !self.tunnels.contains_key(rule_id) {
            return Err(format!("Tunnel {rule_id} was not found"));
        }
        self.set_status(rule_id, "stopping", None);
        if let Some(stop) = self.local_stops.remove(rule_id) {
            let _ = stop.send(());
        }
        if let Some((bind_host, bind_port)) = self.remote_rules.get(rule_id).cloned() {
            // 加 timeout：cancel_tcpip_forward 同样在 inline await 路径上，
            // 服务器卡住会阻塞 worker 主循环。超时后仍清理本地状态，避免
            // 服务器侧的残留转发把 worker 永久钉死。
            match timeout(
                SSH_TUNNEL_OP_TIMEOUT,
                self.handle
                    .cancel_tcpip_forward(bind_host.clone(), bind_port),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    crate::services::logging::session(
                        &self.app,
                        "WARN",
                        "tunnel",
                        &self.tab_id,
                        format!("cancel_tcpip_forward failed id={rule_id} error={error}"),
                    );
                }
                Err(_) => {
                    crate::services::logging::session(
                        &self.app,
                        "WARN",
                        "tunnel",
                        &self.tab_id,
                        format!("cancel_tcpip_forward timed out id={rule_id}"),
                    );
                }
            }
            self.remote_rules.remove(rule_id);
            let state = self
                .app
                .state::<crate::services::workspace::WorkspaceState>();
            let mut forwards = state.remote_forwards.write().await;
            if let Some(rules) = forwards.get_mut(&self.tab_id) {
                rules.retain(|rule| !(rule.bind_host == bind_host && rule.bind_port == bind_port));
                if rules.is_empty() {
                    forwards.remove(&self.tab_id);
                }
            }
        }
        self.set_status(rule_id, "stopped", None);
        crate::services::logging::session(
            &self.app,
            "INFO",
            "tunnel",
            &self.tab_id,
            format!("stopped id={rule_id}"),
        );
        self.list()
    }

    async fn delete(&mut self, rule_id: &str) -> Result<Vec<Value>, String> {
        self.stop(rule_id).await?;
        self.tunnels.remove(rule_id);
        crate::services::logging::session(
            &self.app,
            "INFO",
            "tunnel",
            &self.tab_id,
            format!("deleted id={rule_id}"),
        );
        self.list()
    }

    async fn stop_all(&mut self) {
        let ids = self.tunnels.keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }

    fn set_status(&mut self, rule_id: &str, status: &str, error: Option<String>) {
        if let Some(snapshot) = self.tunnels.get_mut(rule_id) {
            snapshot.status = status.to_string();
            snapshot.error = error;
        }
    }
}

fn validate_tunnel_rule(rule: &SshTunnelRule) -> Result<(), String> {
    if rule.id.trim().is_empty() || !matches!(rule.kind.as_str(), "local" | "remote" | "dynamic") {
        return Err("Tunnel requires a valid id and kind".to_string());
    }
    if rule.bind_host.trim().is_empty() || rule.bind_port == 0 {
        return Err("Tunnel requires a valid bind address and port".to_string());
    }
    if rule.kind != "dynamic"
        && (rule.target_host.as_deref().unwrap_or("").trim().is_empty()
            || rule.target_port.unwrap_or(0) == 0)
    {
        return Err(format!("{} tunnel requires a valid target", rule.kind));
    }
    Ok(())
}

fn tunnel_bind_address(host: &str, port: u16) -> Result<String, String> {
    let host = match host.trim() {
        "*" => "0.0.0.0",
        "" => return Err("Tunnel bind host is empty".to_string()),
        value => value,
    };
    Ok(if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    })
}

async fn forward_local_connection<H: Handler>(
    mut socket: TcpStream,
    handle: Arc<Handle<H>>,
    rule: &SshTunnelRule,
) -> Result<(), String> {
    let origin = socket.local_addr().ok();
    let origin_host = origin
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let origin_port = origin.map(|address| address.port()).unwrap_or(0);
    // 加 timeout：channel_open_direct_tcpip 在远端服务器卡住时会永久
    // await，虽然本函数在 spawn task 里不阻塞主循环，但卡住的 task
    // 不会清理，local 端 TCP 连接也不会关闭，用户侧表现为隧道连接
    // "连上但没数据"。5 秒与 SSH_TUNNEL_OP_TIMEOUT 对齐。
    let channel = timeout(
        SSH_TUNNEL_OP_TIMEOUT,
        handle.channel_open_direct_tcpip(
            rule.target_host.clone().unwrap_or_default(),
            rule.target_port.unwrap_or_default() as u32,
            origin_host,
            origin_port as u32,
        ),
    )
    .await
    .map_err(|_| {
        "SSH local forward timed out: 服务器未在 5 秒内响应 channel_open_direct_tcpip".to_string()
    })?
    .map_err(|error| format!("SSH local forward failed: {error}"))?;
    let mut channel = channel.into_stream();
    copy_bidirectional(&mut socket, &mut channel)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn forward_socks5_connection<H: Handler>(
    mut socket: TcpStream,
    handle: Arc<Handle<H>>,
) -> Result<(), String> {
    // SOCKS5 握手阶段加整体 timeout：恶意客户端可以连上 TCP 但不发
    // 任何数据，让 read_exact 永久 await，spawn task 永远不退出，local
    // 监听端口上的连接数无界增长。10 秒足够正常 SOCKS5 客户端完成握手。
    let handshake_deadline = Duration::from_secs(10);
    let mut greeting = [0_u8; 2];
    timeout(handshake_deadline, socket.read_exact(&mut greeting))
        .await
        .map_err(|_| "SOCKS5 handshake timed out: greeting".to_string())?
        .map_err(|error| error.to_string())?;
    if greeting[0] != 5 {
        return Err("Only SOCKS5 is supported".to_string());
    }
    let mut methods = vec![0_u8; greeting[1] as usize];
    timeout(handshake_deadline, socket.read_exact(&mut methods))
        .await
        .map_err(|_| "SOCKS5 handshake timed out: methods".to_string())?
        .map_err(|error| error.to_string())?;
    if !methods.contains(&0) {
        socket
            .write_all(&[5, 0xff])
            .await
            .map_err(|error| error.to_string())?;
        return Err("SOCKS5 client does not support no-authentication".to_string());
    }
    socket
        .write_all(&[5, 0])
        .await
        .map_err(|error| error.to_string())?;

    let mut request = [0_u8; 4];
    timeout(handshake_deadline, socket.read_exact(&mut request))
        .await
        .map_err(|_| "SOCKS5 handshake timed out: request".to_string())?
        .map_err(|error| error.to_string())?;
    if request[0] != 5 || request[1] != 1 {
        return Err("Only SOCKS5 CONNECT is supported".to_string());
    }
    let target_host = read_socks5_host(&mut socket, request[3]).await?;
    let mut port = [0_u8; 2];
    timeout(handshake_deadline, socket.read_exact(&mut port))
        .await
        .map_err(|_| "SOCKS5 handshake timed out: port".to_string())?
        .map_err(|error| error.to_string())?;
    let target_port = u16::from_be_bytes(port);
    let origin = socket.local_addr().ok();
    let origin_host = origin
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let origin_port = origin.map(|address| address.port()).unwrap_or(0);
    // channel_open_direct_tcpip 加 timeout，同 forward_local_connection。
    let channel = timeout(
        SSH_TUNNEL_OP_TIMEOUT,
        handle.channel_open_direct_tcpip(
            target_host,
            target_port as u32,
            origin_host,
            origin_port as u32,
        ),
    )
    .await
    .map_err(|_| {
        "SSH SOCKS5 forward timed out: 服务器未在 5 秒内响应 channel_open_direct_tcpip".to_string()
    })?
    .map_err(|error| format!("SSH SOCKS5 forward failed: {error}"))?;
    let mut channel = channel.into_stream();
    socket
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|error| error.to_string())?;
    copy_bidirectional(&mut socket, &mut channel)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn read_socks5_host(socket: &mut TcpStream, address_type: u8) -> Result<String, String> {
    // 复用 forward_socks5_connection 的握手 deadline，防止恶意客户端
    // 在 SOCKS5 握手最后阶段（读取目标地址）卡住。
    let read_deadline = Duration::from_secs(10);
    match address_type {
        1 => {
            let mut address = [0_u8; 4];
            timeout(read_deadline, socket.read_exact(&mut address))
                .await
                .map_err(|_| "SOCKS5 handshake timed out: IPv4 address".to_string())?
                .map_err(|error| error.to_string())?;
            Ok(std::net::Ipv4Addr::from(address).to_string())
        }
        3 => {
            let mut length = [0_u8; 1];
            timeout(read_deadline, socket.read_exact(&mut length))
                .await
                .map_err(|_| "SOCKS5 handshake timed out: hostname length".to_string())?
                .map_err(|error| error.to_string())?;
            let mut name = vec![0_u8; length[0] as usize];
            timeout(read_deadline, socket.read_exact(&mut name))
                .await
                .map_err(|_| "SOCKS5 handshake timed out: hostname".to_string())?
                .map_err(|error| error.to_string())?;
            String::from_utf8(name).map_err(|_| "Invalid SOCKS5 hostname".to_string())
        }
        4 => {
            let mut address = [0_u8; 16];
            timeout(read_deadline, socket.read_exact(&mut address))
                .await
                .map_err(|_| "SOCKS5 handshake timed out: IPv6 address".to_string())?
                .map_err(|error| error.to_string())?;
            Ok(std::net::Ipv6Addr::from(address).to_string())
        }
        _ => Err("Unsupported SOCKS5 address type".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Worker loop
// ─────────────────────────────────────────────────────────────────────────────
