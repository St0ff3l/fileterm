// SSH worker based on russh (pure-Rust async SSH implementation).
//
// Migration from ssh2 (libssh2 C bindings) to russh 0.62 was performed to:
//  1. Enable true in-handshake host key verification via async
//     `check_server_key` handler (the renderer can prompt the user while
//     the handshake is in flight, and accept/reject before it completes).
//  2. Support MFA multi-prompt keyboard-interactive flows.
//  3. Drop the `vendored-openssl` C dependency and unify the build across
//     macOS / Windows / Linux.
//  4. Move from a manual `set_blocking(true/false)` juggle to a native
//     tokio task per session.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use russh::client::{Handle, Handler};
use russh::keys::PrivateKeyWithHashAlg;
use russh::{Channel, ChannelMsg};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, RwLock};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tokio_socks::tcp::Socks5Stream;

use super::{TransferFileStat, WorkerCmd};

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Merge network sample history from the previous metrics into the next.
///
/// Mirrors `mergeSystemMetricsHistory` from `packages/core` so the session
/// snapshot retains the rolling `networkSamples` / `networkSamplesByInterface`
/// history. Other fields (cpu, memory, etc.) are taken from `next` verbatim.
fn merge_system_metrics_history(
    previous: Option<&serde_json::Value>,
    next: serde_json::Value,
    history_limit: usize,
) -> serde_json::Value {
    let mut merged = next.clone();
    if let Some(prev) = previous {
        let prev_samples = prev
            .get("networkSamples")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let next_point = next
            .get("networkSamples")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.last())
            .cloned()
            .unwrap_or(serde_json::json!({ "rx": 0, "tx": 0 }));

        let mut combined = prev_samples;
        combined.push(next_point);
        if combined.len() > history_limit {
            combined = combined[combined.len() - history_limit..].to_vec();
        }
        merged["networkSamples"] = serde_json::Value::Array(combined);

        // Per-interface accumulation
        let prev_by_iface = prev
            .get("networkSamplesByInterface")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        if let Some(next_by_iface) = next
            .get("networkSamplesByInterface")
            .and_then(|v| v.as_object())
            .cloned()
        {
            let mut merged_by_iface = serde_json::Map::new();
            for (name, samples_val) in next_by_iface.iter() {
                let next_iface_point = samples_val
                    .as_array()
                    .and_then(|arr| arr.last())
                    .cloned()
                    .unwrap_or(serde_json::json!({ "rx": 0, "tx": 0 }));
                let prev_iface_samples = prev_by_iface
                    .get(name)
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let mut combined = prev_iface_samples;
                combined.push(next_iface_point);
                if combined.len() > history_limit {
                    combined = combined[combined.len() - history_limit..].to_vec();
                }
                merged_by_iface.insert(name.clone(), serde_json::Value::Array(combined));
            }
            merged["networkSamplesByInterface"] = serde_json::Value::Object(merged_by_iface);
        }
    }
    merged
}

pub fn start_ssh_worker(
    tab_id: String,
    profile: Value,
    mut cmd_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    tokio::spawn(async move {
        let tid = tab_id.clone();
        crate::services::logging::ssh_debug(&app, &tid, "SSH worker started");
        // The initial "连接主机...\r\n" notice is already in the session
        // snapshot's `terminal_transcript` (set by `app_open_profile`), so
        // the renderer hydrates it via `bootText` — no need to emit it here.
        // Emitting here would race the renderer's listener registration.
        let run_result = run_worker_loop(&tab_id, &profile, &mut cmd_rx, &app, cancellation.clone()).await;
        if cancellation.is_cancelled() {
            crate::services::logging::ssh_debug(&app, &tid, "SSH worker cancelled");
            return;
        }
        match run_result {
            Ok(()) => {
                emit_terminal_data(&app, &tid, "连接已断开\r\n").await;
            }
            Err(e) => {
                eprintln!("[SSH Worker] error for tab {}: {}", tid, e);
                crate::services::logging::ssh_debug(&app, &tid, format!("SSH worker failed: {e}"));
                emit_terminal_data(&app, &tid, &format!("连接失败: {}\r\n", e)).await;
            }
        }
        update_tab_status_and_emit(&app, &tid, "disconnected").await;

        // ── Auto-reconnect with 2000ms delay ───────────────────────────────
        // Mirrors Electron's `workspace-service.ts` autoReconnectingTabs:
        // if the profile's `reconnectMode === 'auto'`, schedule a reconnect
        // after 2 seconds. The guard set prevents re-entrant triggers while
        // a reconnect is already pending.
        let reconnect_mode = profile
            .get("reconnectMode")
            .and_then(|v| v.as_str())
            .unwrap_or("manual");
        if reconnect_mode == "auto" {
            eprintln!(
                "[SSH Worker] tab={} auto-reconnect scheduled (2000ms delay)",
                tid
            );
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Re-check: tab may have been closed or already reconnected by
            // the user during the delay.
            let state = app.state::<crate::services::workspace::WorkspaceState>();
            let should_reconnect = {
                let tabs = state.tabs.read().await;
                let sessions = state.sessions.read().await;
                let tab_exists = tabs.iter().any(|t| t.id == tid);
                let session_connected = sessions
                    .get(&tid)
                    .map(|s| s.connected)
                    .unwrap_or(false);
                tab_exists && !session_connected
            };

            if should_reconnect {
                eprintln!("[SSH Worker] tab={} auto-reconnect firing", tid);
                // Trigger reconnect via the same path the renderer uses.
                let _ = crate::commands::app_reconnect_tab(app.clone(), tid.clone()).await;
            } else {
                eprintln!(
                    "[SSH Worker] tab={} auto-reconnect cancelled (tab closed or already connected)",
                    tid
                );
            }
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Handler implementation
// ─────────────────────────────────────────────────────────────────────────────

pub struct ClientHandler {
    app: AppHandle,
    tab_id: String,
    profile_id: String,
    host: String,
    port: u16,
    trusted_fingerprint: Option<String>,
}

pub type ClientHandle = Handle<ClientHandler>;

impl Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fp = fingerprint_sha256_base64(server_public_key);
        eprintln!(
            "[SSH host-key] tab={} profile={} host={} fp='{}' trusted={:?}",
            self.tab_id, self.profile_id, self.host, fp, self.trusted_fingerprint
        );
        // Short-circuit: if the profile already trusts this exact
        // fingerprint, accept without prompting. This is the common path
        // after the user previously chose "accept-and-save".
        if let Some(known) = &self.trusted_fingerprint {
            eprintln!(
                "[SSH host-key] comparing known='{}' (len={}) vs fp='{}' (len={}) equal={}",
                known,
                known.len(),
                fp,
                fp.len(),
                known == &fp
            );
            if known == &fp {
                return Ok(true);
            }
            eprintln!(
                "[SSH host-key] mismatch — byte diff: known_bytes={:?} fp_bytes={:?}",
                known.as_bytes(),
                fp.as_bytes()
            );
        }
        let known = self.trusted_fingerprint.clone();
        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel::<Value>();
        {
            let state = self
                .app
                .state::<crate::services::workspace::WorkspaceState>();
            let mut pending = state.pending_interactions.write().await;
            pending.insert(request_id.clone(), tx);
        }
        // Emit a `host-verification` interaction request. The payload shape
        // matches `SshHostVerificationRequest` in packages/core so the
        // renderer's `useSshInteractions` hook recognises it and shows the
        // accept/reject dialog. The renderer resolves via
        // `app_resolve_ssh_interaction`, which forwards the response back
        // through the oneshot channel.
        let _ = self.app.emit(
            "ssh:interaction",
            serde_json::json!({
                "requestId": request_id,
                "kind": "host-verification",
                "tabId": self.tab_id,
                "profileId": self.profile_id,
                "host": self.host,
                "port": self.port,
                "fingerprint": fp,
                "knownFingerprint": known,
            }),
        );
        let decision = match rx.await {
            Ok(response) => response
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("cancel")
                .to_string(),
            Err(_) => "cancel".to_string(),
        };
        match decision.as_str() {
            "accept-and-save" => {
                // Persist the trusted fingerprint so future connects
                // short-circuit the prompt.
                let _ = crate::services::profile_ops::update_trusted_host_fingerprint(
                    &self.app,
                    &self.profile_id,
                    &fp,
                )
                .await;
                self.trusted_fingerprint = Some(fp);
                Ok(true)
            }
            "accept-once" => Ok(true),
            _ => Ok(false),
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let state = self.app.state::<crate::services::workspace::WorkspaceState>();
        let target = {
            let forwards = state.remote_forwards.read().await;
            forwards.get(&self.tab_id).and_then(|rules| {
                rules.iter().find(|rule| {
                    rule.bind_port == connected_port
                        && remote_bind_host_matches(&rule.bind_host, connected_address)
                })
            }).cloned()
        };

        let Some(target) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };

        reply.accept().await;
        let tab_id = self.tab_id.clone();
        tokio::spawn(async move {
            let result = async {
                let mut local = TcpStream::connect((&*target.target_host, target.target_port)).await?;
                let mut remote = channel.into_stream();
                copy_bidirectional(&mut local, &mut remote).await?;
                Ok::<(), std::io::Error>(())
            }
            .await;
            if let Err(error) = result {
                eprintln!("[SSH tunnel] remote forward tab={tab_id} connection failed: {error}");
            }
        });
        Ok(())
    }
}

fn remote_bind_host_matches(bind_host: &str, connected_address: &str) -> bool {
    bind_host == connected_address || matches!(bind_host, "0.0.0.0" | "::" | "*")
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
        tunnels.sort_by(|left, right| left["name"].as_str().unwrap_or("").cmp(right["name"].as_str().unwrap_or("")));
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
            return Err(format!("Tunnel {}:{} is already configured", rule.bind_host, rule.bind_port));
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
                self.list()
            }
            Err(error) => {
                self.set_status(rule_id, "error", Some(error.clone()));
                Err(error)
            }
        }
    }

    async fn start_local_or_dynamic(&mut self, rule: &SshTunnelRule) -> Result<(), String> {
        let listener = TcpListener::bind(tunnel_bind_address(&rule.bind_host, rule.bind_port)?)
            .await
            .map_err(|error| format!("Tunnel listen failed on {}:{}: {error}", rule.bind_host, rule.bind_port))?;
        let (stop_tx, mut stop_rx) = oneshot::channel();
        let handle = Arc::clone(&self.handle);
        let rule = rule.clone();
        let rule_id = rule.id.clone();
        let tab_id = self.tab_id.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stop_rx => break,
                    accepted = listener.accept() => match accepted {
                        Ok((socket, _peer)) => {
                            let handle = Arc::clone(&handle);
                            let rule = rule.clone();
                            let connection_tab_id = tab_id.clone();
                            tokio::spawn(async move {
                                let result = if rule.kind == "dynamic" {
                                    forward_socks5_connection(socket, handle).await
                                } else {
                                    forward_local_connection(socket, handle, &rule).await
                                };
                                if let Err(error) = result {
                                    eprintln!("[SSH tunnel] tab={connection_tab_id} {} connection failed: {error}", rule.id);
                                }
                            });
                        }
                        Err(error) => {
                            eprintln!("[SSH tunnel] tab={tab_id} {} listener failed: {error}", rule.id);
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
        let actual_port = self
            .handle
            .tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32)
            .await
            .map_err(|error| format!("Remote tunnel request failed: {error}"))?;
        let target = crate::services::workspace::RemoteForwardTarget {
            bind_host: rule.bind_host.clone(),
            bind_port: actual_port,
            target_host: rule.target_host.clone().unwrap_or_default(),
            target_port: rule.target_port.unwrap_or_default(),
        };
        let state = self.app.state::<crate::services::workspace::WorkspaceState>();
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
            self.handle
                .cancel_tcpip_forward(bind_host.clone(), bind_port)
                .await
                .map_err(|error| format!("Remote tunnel stop failed: {error}"))?;
            self.remote_rules.remove(rule_id);
            let state = self.app.state::<crate::services::workspace::WorkspaceState>();
            let mut forwards = state.remote_forwards.write().await;
            if let Some(rules) = forwards.get_mut(&self.tab_id) {
                rules.retain(|rule| !(rule.bind_host == bind_host && rule.bind_port == bind_port));
                if rules.is_empty() {
                    forwards.remove(&self.tab_id);
                }
            }
        }
        self.set_status(rule_id, "stopped", None);
        self.list()
    }

    async fn delete(&mut self, rule_id: &str) -> Result<Vec<Value>, String> {
        self.stop(rule_id).await?;
        self.tunnels.remove(rule_id);
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
        && (rule.target_host.as_deref().unwrap_or("").trim().is_empty() || rule.target_port.unwrap_or(0) == 0)
    {
        return Err(format!("{} tunnel requires a valid target", rule.kind));
    }
    Ok(())
}

fn tunnel_bind_address(host: &str, port: u16) -> Result<String, String> {
    let host = match host.trim() {
        "*" => "0.0.0.0",
        value if value.is_empty() => return Err("Tunnel bind host is empty".to_string()),
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
    let origin_host = origin.map(|address| address.ip().to_string()).unwrap_or_else(|| "127.0.0.1".to_string());
    let origin_port = origin.map(|address| address.port()).unwrap_or(0);
    let mut channel = handle
        .channel_open_direct_tcpip(
            rule.target_host.clone().unwrap_or_default(),
            rule.target_port.unwrap_or_default() as u32,
            origin_host,
            origin_port as u32,
        )
        .await
        .map_err(|error| format!("SSH local forward failed: {error}"))?
        .into_stream();
    copy_bidirectional(&mut socket, &mut channel)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn forward_socks5_connection<H: Handler>(
    mut socket: TcpStream,
    handle: Arc<Handle<H>>,
) -> Result<(), String> {
    let mut greeting = [0_u8; 2];
    socket.read_exact(&mut greeting).await.map_err(|error| error.to_string())?;
    if greeting[0] != 5 {
        return Err("Only SOCKS5 is supported".to_string());
    }
    let mut methods = vec![0_u8; greeting[1] as usize];
    socket.read_exact(&mut methods).await.map_err(|error| error.to_string())?;
    if !methods.contains(&0) {
        socket.write_all(&[5, 0xff]).await.map_err(|error| error.to_string())?;
        return Err("SOCKS5 client does not support no-authentication".to_string());
    }
    socket.write_all(&[5, 0]).await.map_err(|error| error.to_string())?;

    let mut request = [0_u8; 4];
    socket.read_exact(&mut request).await.map_err(|error| error.to_string())?;
    if request[0] != 5 || request[1] != 1 {
        return Err("Only SOCKS5 CONNECT is supported".to_string());
    }
    let target_host = read_socks5_host(&mut socket, request[3]).await?;
    let mut port = [0_u8; 2];
    socket.read_exact(&mut port).await.map_err(|error| error.to_string())?;
    let target_port = u16::from_be_bytes(port);
    let origin = socket.local_addr().ok();
    let origin_host = origin.map(|address| address.ip().to_string()).unwrap_or_else(|| "127.0.0.1".to_string());
    let origin_port = origin.map(|address| address.port()).unwrap_or(0);
    let mut channel = handle
        .channel_open_direct_tcpip(target_host, target_port as u32, origin_host, origin_port as u32)
        .await
        .map_err(|error| format!("SSH SOCKS5 forward failed: {error}"))?
        .into_stream();
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
    match address_type {
        1 => {
            let mut address = [0_u8; 4];
            socket.read_exact(&mut address).await.map_err(|error| error.to_string())?;
            Ok(std::net::Ipv4Addr::from(address).to_string())
        }
        3 => {
            let mut length = [0_u8; 1];
            socket.read_exact(&mut length).await.map_err(|error| error.to_string())?;
            let mut name = vec![0_u8; length[0] as usize];
            socket.read_exact(&mut name).await.map_err(|error| error.to_string())?;
            String::from_utf8(name).map_err(|_| "Invalid SOCKS5 hostname".to_string())
        }
        4 => {
            let mut address = [0_u8; 16];
            socket.read_exact(&mut address).await.map_err(|error| error.to_string())?;
            Ok(std::net::Ipv6Addr::from(address).to_string())
        }
        _ => Err("Unsupported SOCKS5 address type".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Worker loop
// ─────────────────────────────────────────────────────────────────────────────

async fn update_tab_status_and_emit(app: &AppHandle, tab_id: &str, status: &str) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let connected = status == "connected";
    let mut summary = "连接已断开".to_string();
    let mut transcript = String::new();
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = status.to_string();
        }
    }
    {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.connected = connected;
            summary = session.summary.clone();
            transcript = session.terminal_transcript.clone();
        }
    }
    let payload = serde_json::json!({
        "tabId": tab_id.to_string(),
        "summary": summary,
        "transcript": transcript,
        "connected": connected,
    });
    let _ = app.emit("terminal:state", payload);

    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

/// Emit a terminal data chunk to the renderer and append it to the session
/// snapshot's `terminal_transcript` so later `terminal:state` / snapshot
/// refreshes surface the full history (handles the case where the renderer
/// missed the live `terminal:data` event, e.g. during a fast-fail connect).
async fn emit_terminal_data(app: &AppHandle, tab_id: &str, chunk: &str) {
    let _ = app.emit(
        "terminal:data",
        serde_json::json!({
            "tabId": tab_id,
            "chunk": chunk,
        }),
    );
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    if let Some(s) = sessions.get_mut(tab_id) {
        s.terminal_transcript.push_str(chunk);
        // Cap transcript to 200k chars (matches Electron's BoundedTextBuffer).
        if s.terminal_transcript.len() > 200_000 {
            let cut = s.terminal_transcript.len() - 180_000;
            s.terminal_transcript = s.terminal_transcript[cut..].to_string();
        }
    }
}

/// Mirrors Electron's `followShellCwd`: only a confirmed shell CWD update may
/// move the file panel, and only while the user has Follow terminal enabled.
async fn follow_shell_cwd(
    app: AppHandle,
    tab_id: String,
    cwd: String,
    sftp: Arc<RwLock<SftpSession>>,
    handle: Arc<Handle<ClientHandler>>,
    file_access_mode: String,
    sudo_user: Option<String>,
    sudo_password: Option<String>,
) {
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        let mut sessions = state.sessions.write().await;
        let Some(session) = sessions.get_mut(&tab_id) else {
            return;
        };
        if session.shell_cwd.as_deref() != Some(cwd.as_str()) || !session.follow_shell_cwd {
            return;
        }
        session.remote_files_loading = true;
    }
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }

    // The SFTP session belongs to the login user. Once `sudo -i` has started
    // a root shell, following CWD through that channel silently remains in
    // the old user's view. Electron switches to its sudo shell path here.
    let files = if file_access_mode == "root" {
        exec_list_dir_via_shell(&handle, &cwd, &sudo_user, &sudo_password).await
    } else {
        // russh-sftp's client is one request stream. Serialise access to it:
        // concurrent read locks let multiple list/delete/upload requests
        // interleave and eventually time out after app focus is restored.
        let mut sftp = sftp.write().await;
        list_dir(&mut sftp, &cwd).await
    };

    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    let Some(session) = sessions.get_mut(&tab_id) else {
        return;
    };
    session.remote_files_loading = false;
    if session.shell_cwd.as_deref() == Some(cwd.as_str()) && session.follow_shell_cwd {
        if let Ok(files) = files {
            session.remote_path = cwd;
            session.remote_files = files;
        }
    }
    drop(sessions);

    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

/// Flush the batch buffer to the renderer and append to the session
/// transcript in one step. Used by every flush path in the main event
/// loop (cmd exit, shell close, 16ms timer) so they stay consistent.
async fn flush_batch(batch: &mut Vec<u8>, app: &AppHandle, tab_id: &str) {
    if batch.is_empty() {
        return;
    }
    let chunk = String::from_utf8_lossy(batch).into_owned();
    batch.clear();
    emit_terminal_data(app, tab_id, &chunk).await;
}

fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
            {
                result.push(hex);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn track_cwd_and_user(chunk: &str, buffer: &mut String) -> (Option<String>, Option<String>) {
    buffer.push_str(chunk);
    if buffer.len() > 8192 {
        *buffer = buffer[buffer.len() - 4096..].to_string();
    }

    let mut cwd = None;
    let mut user = None;

    let re_cwd =
        regex::Regex::new(r"\x1b\]7;file://([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap();
    let re_user =
        regex::Regex::new(r"\x1b\]1337;RemoteUser=([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap();

    for cap in re_cwd.captures_iter(buffer) {
        let raw_path = &cap[1];
        if let Some(slash_idx) = raw_path.find('/') {
            let path_part = &raw_path[slash_idx..];
            cwd = Some(percent_decode(path_part));
        }
    }
    for cap in re_user.captures_iter(buffer) {
        user = Some(cap[1].to_string());
    }
    (cwd, user)
}

/// Remove CSI/OSC control sequences before inspecting a prompt. This mirrors
/// Electron's root-prompt heuristic without feeding visual escape codes into
/// the comparison.
fn visible_shell_text(value: &str) -> String {
    let csi = regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("constant CSI regex");
    let osc = regex::Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").expect("constant OSC regex");
    osc.replace_all(&csi.replace_all(value, ""), "").into_owned()
}

fn looks_like_root_prompt(value: &str) -> bool {
    visible_shell_text(value).trim_end().ends_with('#')
}

/// Track the interactive sudo exchange on the terminal channel. The password
/// stays worker-local and is never copied into a snapshot or emitted event.
fn capture_sudo_password_input(
    input: &str,
    awaiting_password: &mut bool,
    pending_password: &mut String,
    recent_input: &mut String,
    sudo_password: &mut Option<String>,
) -> bool {
    let mut changed = false;
    for ch in input.chars() {
        recent_input.push(ch);
        if recent_input.len() > 512 {
            let keep_from = recent_input.len() - 256;
            *recent_input = recent_input[keep_from..].to_string();
        }
        if !*awaiting_password {
            continue;
        }
        match ch {
            '\r' | '\n' => {
                if !pending_password.is_empty() {
                    changed = sudo_password.as_deref() != Some(pending_password.as_str());
                    *sudo_password = Some(std::mem::take(pending_password));
                }
                *awaiting_password = false;
            }
            '\u{3}' => {
                changed = sudo_password.take().is_some();
                pending_password.clear();
                *awaiting_password = false;
            }
            '\u{8}' | '\u{7f}' => {
                pending_password.pop();
            }
            _ if !ch.is_control() => pending_password.push(ch),
            _ => {}
        }
    }
    changed
}

fn track_sudo_prompt_from_terminal(
    output: &str,
    prompt_buffer: &mut String,
    awaiting_password: &mut bool,
    pending_password: &mut String,
    sudo_password: &mut Option<String>,
) -> bool {
    prompt_buffer.push_str(&visible_shell_text(output));
    if prompt_buffer.len() > 2048 {
        let keep_from = prompt_buffer.len() - 1024;
        *prompt_buffer = prompt_buffer[keep_from..].to_string();
    }
    let lower = prompt_buffer.to_ascii_lowercase();
    let auth_failed = lower.contains("sorry, try again")
        || lower.contains("incorrect password")
        || lower.contains("authentication failure");
    if auth_failed {
        *awaiting_password = false;
        pending_password.clear();
        prompt_buffer.clear();
        return sudo_password.take().is_some();
    }
    if lower.contains("password") || prompt_buffer.contains("密码") {
        *awaiting_password = true;
        pending_password.clear();
        // Consume this prompt; otherwise the historical word "password"
        // would mark every later terminal keystroke as a sudo password.
        prompt_buffer.clear();
    }
    false
}

/// Buffered output produced while injecting the internal CWD hook.
///
/// A POSIX PTY is allowed to split the command echo, the generated OSC marker
/// and the replacement prompt across packets. Do not release the buffer as
/// soon as the marker is observed: doing so leaks the tail of a long setup
/// command after `sudo -i` on Debian/bash.
struct ShellSetupEchoSuppression {
    buffer: String,
    started_at: Instant,
    visible_prefix_length: Option<usize>,
    marker_seen_at: Option<Instant>,
    preserve_visible_prefix: bool,
}

impl ShellSetupEchoSuppression {
    fn new(preserve_visible_prefix: bool) -> Self {
        Self {
            buffer: String::new(),
            started_at: Instant::now(),
            visible_prefix_length: None,
            marker_seen_at: None,
            preserve_visible_prefix,
        }
    }
}

const SHELL_SETUP_SETTLE_DELAY: Duration = Duration::from_millis(200);
const SHELL_SETUP_TIMEOUT: Duration = Duration::from_millis(1200);
const MAX_SHELL_SETUP_BUFFER_BYTES: usize = 16 * 1024;

fn shell_setup_release_deadline(pending: &Option<ShellSetupEchoSuppression>) -> Option<Instant> {
    pending.as_ref().map(|state| {
        state
            .marker_seen_at
            .map(|seen_at| seen_at + SHELL_SETUP_SETTLE_DELAY)
            .unwrap_or(state.started_at + SHELL_SETUP_TIMEOUT)
    })
}

fn finish_shell_setup_suppression(pending: &mut Option<ShellSetupEchoSuppression>) -> String {
    let Some(state) = pending.take() else {
        return String::new();
    };
    if !state.preserve_visible_prefix {
        return String::new();
    }
    state
        .visible_prefix_length
        .map(|length| state.buffer[..length].to_string())
        .unwrap_or_default()
}

/// Suppresses the echo and replacement prompt from an internal CWD-hook
/// injection. The bounded timeout fails closed: a malformed shell must not
/// expose the hidden command in the user's terminal transcript.
fn suppress_shell_setup_echo(
    pending: &mut Option<ShellSetupEchoSuppression>,
    chunk: &str,
) -> String {
    if pending.is_none() {
        return chunk.to_string();
    }

    let now = Instant::now();
    if shell_setup_release_deadline(pending).is_some_and(|deadline| now >= deadline) {
        return format!("{}{chunk}", finish_shell_setup_suppression(pending));
    }

    let state = pending.as_mut().expect("pending CWD hook suppression exists");

    state.buffer.push_str(chunk);
    const HOOK_MARKER: &str = "__tdcwd";
    let re_cwd = regex::Regex::new(r"\x1b\]7;file://[^\x07\x1b]*(?:\x07|\x1b\\)")
        .expect("constant OSC7 regex");

    if re_cwd.is_match(&state.buffer) {
        state.marker_seen_at.get_or_insert(now);
        if state.visible_prefix_length.is_none() {
            state.visible_prefix_length = Some(
                state
                    .buffer
                    .find("test -z \"${FISH_VERSION-}\"")
                    .or_else(|| state.buffer.find("__tdcwd(){"))
                    .or_else(|| state.buffer.find(HOOK_MARKER))
                    .unwrap_or(0),
            );
        }
    }

    if state.buffer.len() > MAX_SHELL_SETUP_BUFFER_BYTES {
        return finish_shell_setup_suppression(pending);
    }

    String::new()
}

/// Returns the POSIX shell CWD setup script for the given platform.
///
/// Mirrors Electron's `shellCwdSetupForPlatform`:
/// - `busybox` → compact ash-compatible one-liner (≤256 bytes to avoid
///   BusyBox line-editor truncation)
/// - `linux` → bash/zsh/posix-aware hook via PROMPT_COMMAND / precmd / PS1
/// - `windows` / unknown → `None` (fail-closed, no injection)
///
/// The injected hook defines `__tdcwd` which emits OSC7 (`file://<path>`) and
/// 1337 (`RemoteUser=<user>`) on every prompt, enabling CWD + sudo user
/// tracking without polling.
fn shell_cwd_setup_for_platform(platform: &str) -> Option<&'static str> {
    match platform {
        "busybox" => Some(BUSYBOX_SHELL_CWD_SETUP),
        "linux" => Some(SHELL_CWD_SETUP),
        _ => None,
    }
}

/// Linux shell CWD hook (bash / zsh / posix). Mirrors Electron's
/// `SHELL_CWD_SETUP` constant. Uses `test -z "${FISH_VERSION-}"` as a fish
/// guard so the hook is a no-op on fish (which has its own CWD reporting).
const SHELL_CWD_SETUP: &str = "test -z \"${FISH_VERSION-}\" && eval '__tdcwd() { printf \"\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007\" \"$(pwd -P 2>/dev/null)\" \"$(id -un 2>/dev/null)\"; }; if [ -n \"${ZSH_VERSION-}\" ]; then autoload -Uz add-zsh-hook 2>/dev/null; add-zsh-hook -D precmd __tdcwd 2>/dev/null; add-zsh-hook precmd __tdcwd 2>/dev/null; elif [ -n \"${BASH_VERSION-}\" ]; then case \"${PROMPT_COMMAND-}\" in *\"__tdcwd\"*) ;; *) PROMPT_COMMAND=\"__tdcwd${PROMPT_COMMAND:+;$PROMPT_COMMAND}\" ;; esac; else case \"${PS1-}\" in *\"__tdcwd\"*) ;; *) PS1=\"\\$(__tdcwd)${PS1-}\" ;; esac; fi; __tdcwd'";

/// BusyBox ash CWD hook. Kept under 256 bytes to avoid truncation in the
/// small interactive line-editing buffer. Mirrors Electron's
/// `BUSYBOX_SHELL_CWD_SETUP` constant.
const BUSYBOX_SHELL_CWD_SETUP: &str = "__tdcwd(){ printf '\\033]7;file://%s\\007\\033]1337;RemoteUser=%s\\007' \"$(pwd -P 2>/dev/null)\" \"$(id -un 2>/dev/null)\";};PS1='$(__tdcwd)'\"${PS1-}\";__tdcwd";

/// Normalize an encoding label to a canonical name understood by
/// `encoding_rs`. Mirrors Electron's `normalizeEncoding` alias table.
fn normalize_encoding(encoding: &str) -> &'static str {
    let normalized = encoding.trim().to_lowercase();
    match normalized.as_str() {
        "utf8" | "utf-8" | "" => "utf-8",
        "utf-8-bom" => "utf-8-bom",
        "utf16" | "utf-16" | "utf16le" | "utf-16le" => "utf-16le",
        "utf16be" | "utf-16be" => "utf-16be",
        "gb18030" => "gb18030",
        "gbk" => "gbk",
        "big5" | "cp950" => "big5",
        "euc-jp" | "eucjp" => "euc-jp",
        "shift-jis" | "shiftjis" | "shift_jis" | "sjis" => "shift_jis",
        "iso-2022-jp" => "iso-2022-jp",
        "euc-kr" | "euckr" | "cp949" => "euc-kr",
        "windows-1252" | "cp1252" => "windows-1252",
        "latin1" | "iso-8859-1" => "iso-8859-1",
        "windows-1251" | "cp1251" => "windows-1251",
        _ => "utf-8",
    }
}

/// Decode raw bytes into a string using the given encoding. Mirrors
/// Electron's `decodeBuffer` (iconv-lite + BOM stripping).
fn decode_bytes(buf: &[u8], encoding: &str) -> Result<String, String> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => {
            let mut s = String::from_utf8_lossy(buf).into_owned();
            // Strip UTF-8 BOM if present
            if s.starts_with('\u{feff}') {
                s = s[3..].to_string();
            }
            Ok(s)
        }
        "utf-8-bom" => {
            let start = if buf.starts_with(&[0xef, 0xbb, 0xbf]) { 3 } else { 0 };
            String::from_utf8(buf[start..].to_vec())
                .map_err(|e| format!("utf-8 decode failed: {}", e))
        }
        "utf-16le" => {
            let start = if buf.starts_with(&[0xff, 0xfe]) { 2 } else { 0 };
            decode_utf16(&buf[start..], true)
        }
        "utf-16be" => {
            let start = if buf.starts_with(&[0xfe, 0xff]) { 2 } else { 0 };
            decode_utf16(&buf[start..], false)
        }
        "gb18030" => Ok(encoding_rs::GB18030.decode(buf).0.into_owned()),
        "gbk" => Ok(encoding_rs::GBK.decode(buf).0.into_owned()),
        "big5" => Ok(encoding_rs::BIG5.decode(buf).0.into_owned()),
        "euc-jp" => Ok(encoding_rs::EUC_JP.decode(buf).0.into_owned()),
        "shift_jis" => Ok(encoding_rs::SHIFT_JIS.decode(buf).0.into_owned()),
        "iso-2022-jp" => Ok(encoding_rs::ISO_2022_JP.decode(buf).0.into_owned()),
        "euc-kr" => Ok(encoding_rs::EUC_KR.decode(buf).0.into_owned()),
        "windows-1252" => Ok(encoding_rs::WINDOWS_1252.decode(buf).0.into_owned()),
        "iso-8859-1" => Ok(encoding_rs::WINDOWS_1252.decode(buf).0.into_owned()),
        "windows-1251" => Ok(encoding_rs::WINDOWS_1251.decode(buf).0.into_owned()),
        _ => Ok(String::from_utf8_lossy(buf).into_owned()),
    }
}

/// Decode UTF-16 bytes (little-endian or big-endian) into a string.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("utf-16 data length is odd".to_string());
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units).map_err(|e| format!("utf-16 decode failed: {}", e))
}

/// Encode a string into bytes using the given encoding. Mirrors Electron's
/// `encodeText` (iconv-lite + BOM prefixing for utf-8-bom / utf-16le / utf-16be).
fn encode_text(content: &str, encoding: &str) -> Vec<u8> {
    let normalized = normalize_encoding(encoding);
    match normalized {
        "utf-8" => content.as_bytes().to_vec(),
        "utf-8-bom" => {
            let mut bytes = vec![0xef, 0xbb, 0xbf];
            bytes.extend_from_slice(content.as_bytes());
            bytes
        }
        "utf-16le" => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            bytes
        }
        "utf-16be" => {
            let mut bytes = vec![0xfe, 0xff];
            for unit in content.encode_utf16() {
                bytes.extend_from_slice(&unit.to_be_bytes());
            }
            bytes
        }
        "gb18030" => encoding_rs::GB18030.encode(content).0.into_owned(),
        "gbk" => encoding_rs::GBK.encode(content).0.into_owned(),
        "big5" => encoding_rs::BIG5.encode(content).0.into_owned(),
        "euc-jp" => encoding_rs::EUC_JP.encode(content).0.into_owned(),
        "shift_jis" => encoding_rs::SHIFT_JIS.encode(content).0.into_owned(),
        "iso-2022-jp" => encoding_rs::ISO_2022_JP.encode(content).0.into_owned(),
        "euc-kr" => encoding_rs::EUC_KR.encode(content).0.into_owned(),
        "windows-1252" => encoding_rs::WINDOWS_1252.encode(content).0.into_owned(),
        "iso-8859-1" => encoding_rs::WINDOWS_1252.encode(content).0.into_owned(),
        "windows-1251" => encoding_rs::WINDOWS_1251.encode(content).0.into_owned(),
        _ => content.as_bytes().to_vec(),
    }
}

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
fn load_jump_profile(app: &AppHandle, profile_id: &str) -> Result<Value, String> {
    let profiles = crate::storage::read_json_array(app, "profiles.json")
        .map_err(|e| format!("Failed to read profiles.json for jump host: {}", e))?;
    profiles
        .iter()
        .find(|p| p.get("id").and_then(|id| id.as_str()) == Some(profile_id))
        .cloned()
        .ok_or_else(|| format!("Jump Host profile '{}' not found", profile_id))
}

trait SshTransport: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> SshTransport for T {}

type BoxedSshTransport = Box<dyn SshTransport>;

fn new_client_handler(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
    trusted_fingerprint: Option<String>,
) -> ClientHandler {
    ClientHandler {
        app: app.clone(),
        tab_id: tab_id.to_string(),
        profile_id: profile_id.to_string(),
        host: host.to_string(),
        port,
        trusted_fingerprint,
    }
}

async fn connect_target_through_jump(
    jump_handle: &Handle<ClientHandler>,
    config: Arc<russh::client::Config>,
    handler: ClientHandler,
    host: &str,
    port: u16,
) -> Result<Handle<ClientHandler>, String> {
    let channel = jump_handle
        .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
        .await
        .map_err(|error| format!("Jump Host direct-tcpip failed: {error}"))?;
    russh::client::connect_stream(config, channel.into_stream(), handler)
        .await
        .map_err(|error| format!("SSH connect via jump host failed: {error}"))
}

/// Creates the raw transport used by russh. Profiles with a SOCKS5 or HTTP
/// CONNECT proxy must reach the target through that proxy before SSH begins
/// its handshake; passing the profile directly to `russh::connect` bypasses
/// proxy configuration entirely.
async fn connect_ssh_transport(profile: &Value, host: &str, port: u16) -> Result<BoxedSshTransport, String> {
    let proxy = profile.get("proxy").and_then(Value::as_object);
    let proxy_type = proxy
        .and_then(|value| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("none");

    if proxy_type == "none" {
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
    let proxy_port = proxy
        .and_then(|value| value.get("port"))
        .and_then(Value::as_u64)
        .filter(|value| (1..=u16::MAX as u64).contains(value))
        .ok_or_else(|| "Proxy port must be between 1 and 65535".to_string())? as u16;
    let username = proxy
        .and_then(|value| value.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let password = proxy
        .and_then(|value| value.get("password"))
        .and_then(Value::as_str)
        .unwrap_or("");

    match proxy_type {
        "socks5" => {
            let stream = if username.is_empty() {
                Socks5Stream::connect((proxy_host, proxy_port), (host, port))
                    .await
                    .map_err(|error| format!("SOCKS5 proxy connect failed: {error}"))?
            } else {
                Socks5Stream::connect_with_password(
                    (proxy_host, proxy_port),
                    (host, port),
                    username,
                    password,
                )
                .await
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

async fn connect_http_proxy(
    proxy_host: &str,
    proxy_port: u16,
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<TcpStream, String> {
    let mut stream = TcpStream::connect((proxy_host, proxy_port))
        .await
        .map_err(|error| format!("HTTP proxy connect failed: {error}"))?;
    let _ = stream.set_nodelay(true);
    let request = build_http_connect_request(host, port, username, password)?;
    stream
        .write_all(&request)
        .await
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
        let read = stream
            .read(&mut chunk)
            .await
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
    let status = status_line.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("HTTP proxy CONNECT failed: {status_line}"));
    }
    Ok(stream)
}

fn build_http_connect_request(host: &str, port: u16, username: &str, password: &str) -> Result<Vec<u8>, String> {
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
    let mut request = format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\nProxy-Connection: Keep-Alive\r\n");
    if !username.is_empty() {
        let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request.push_str(&format!("Proxy-Authorization: Basic {credentials}\r\n"));
    }
    request.push_str("\r\n");
    Ok(request.into_bytes())
}

async fn open_session(
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
) -> Result<Handle<ClientHandler>, String> {
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = profile.get("port").and_then(|p| p.as_i64()).unwrap_or(22) as u16;
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
    let trusted = profile
        .get("trustedHostFingerprint")
        .and_then(|f| f.as_str())
        .map(|s| s.to_string());
    eprintln!(
        "[SSH host-key] open_session tab={} profile_id='{}' trustedHostFingerprint_from_profile={:?}",
        tab_id,
        profile.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        trusted
    );

    let profile_id = profile
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();
    let mut config = russh::client::Config::default();
    config.inactivity_timeout = Some(Duration::from_secs(300));
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
        eprintln!(
            "[SSH jump] tab={} — resolving jump profile '{}'",
            tab_id, jpid
        );
        // Load the jump profile from disk (same directory as profiles.json)
        let jump_profile = load_jump_profile(app, &jpid)?;

        // Validate: jump must be a different SSH profile, and must not
        // itself have a jumpProfileId (no chained jumps).
        let jump_id = jump_profile.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if jump_id == profile.get("id").and_then(|v| v.as_str()).unwrap_or("") {
            return Err("Jump Host must reference a different profile".to_string());
        }
        if jump_profile.get("jumpProfileId").is_some() {
            return Err("Jump Host cannot itself reference another Jump Host".to_string());
        }

        eprintln!(
            "[SSH jump] tab={} — connecting to jump host '{}@{}:{}'",
            tab_id,
            jump_profile.get("username").and_then(|v| v.as_str()).unwrap_or(""),
            jump_profile.get("host").and_then(|v| v.as_str()).unwrap_or(""),
            jump_profile.get("port").and_then(|v| v.as_i64()).unwrap_or(22)
        );

        // Connect + authenticate to the jump host.
        // Box::pin is required because `open_session` is recursive (the jump
        // host itself could be resolved via another open_session call) and
        // Rust requires indirection for recursive async fns to avoid
        // infinitely-sized futures.
        let jump_handle = Box::pin(open_session(&jump_profile, app, tab_id)).await?;
        eprintln!("[SSH jump] tab={} — jump host connected, opening direct-tcpip to target", tab_id);

        let mut target_handle = connect_target_through_jump(
            &jump_handle,
            config.clone(),
            new_client_handler(app, tab_id, &profile_id, &host, port, trusted.clone()),
            &host,
            port,
        )
        .await?;
        match try_authenticate(&mut target_handle, &username, &auth_type, profile, app, tab_id).await? {
            AuthenticationResult::Authenticated => return Ok(target_handle),
            AuthenticationResult::NeedsFreshKeyboardInteractive => {
                // russh cannot switch from a rejected password/public-key
                // exchange to keyboard-interactive on the same handle. Open a
                // new channel through the already-authenticated jump host.
                let mut retry_handle = connect_target_through_jump(
                    &jump_handle,
                    config,
                    new_client_handler(app, tab_id, &profile_id, &host, port, trusted),
                    &host,
                    port,
                )
                .await?;
                if try_keyboard_interactive(
                    &mut retry_handle,
                    &username,
                    profile.get("password").and_then(Value::as_str).unwrap_or(""),
                    app,
                    tab_id,
                    &profile_id,
                    &host,
                    port,
                )
                .await?
                {
                    return Ok(retry_handle);
                }
            }
            AuthenticationResult::Rejected => {}
        }
        return Err("SSH Authentication failed (via jump host)".to_string());
    }

    let stream = connect_ssh_transport(profile, &host, port).await?;
    let mut handle = russh::client::connect_stream(
        config.clone(),
        stream,
        new_client_handler(app, tab_id, &profile_id, &host, port, trusted.clone()),
    )
        .await
        .map_err(|e| format!("SSH connect failed: {}", e))?;
    match try_authenticate(&mut handle, &username, &auth_type, profile, app, tab_id).await? {
        AuthenticationResult::Authenticated => Ok(handle),
        AuthenticationResult::NeedsFreshKeyboardInteractive => {
            // See the equivalent jump-host path above: reconnect before
            // keyboard-interactive fallback so russh never stalls on a
            // rejected authentication handle.
            let stream = connect_ssh_transport(profile, &host, port).await?;
            let mut retry_handle = russh::client::connect_stream(
                config,
                stream,
                new_client_handler(app, tab_id, &profile_id, &host, port, trusted),
            )
            .await
            .map_err(|error| format!("SSH reconnect for keyboard-interactive failed: {error}"))?;
            if try_keyboard_interactive(
                &mut retry_handle,
                &username,
                profile.get("password").and_then(Value::as_str).unwrap_or(""),
                app,
                tab_id,
                &profile_id,
                &host,
                port,
            )
            .await?
            {
                Ok(retry_handle)
            } else {
                Err("SSH Authentication failed".to_string())
            }
        }
        AuthenticationResult::Rejected => Err("SSH Authentication failed".to_string()),
    }
}

enum AuthenticationResult {
    Authenticated,
    /// Password/public-key authentication was rejected. A caller that owns
    /// the transport must reconnect before starting keyboard-interactive.
    NeedsFreshKeyboardInteractive,
    Rejected,
}

#[derive(Clone, Debug)]
struct KeyboardInteractivePrompt {
    prompt: String,
    echo: bool,
}

#[derive(Clone, Debug)]
struct KeyboardInteractiveRequest {
    name: String,
    instructions: String,
    prompts: Vec<KeyboardInteractivePrompt>,
}

async fn try_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    auth_type: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
) -> Result<AuthenticationResult, String> {
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_string();
    let port = profile.get("port").and_then(|p| p.as_i64()).unwrap_or(22) as u16;
    let profile_id = profile
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_string();
    match auth_type {
        "password" => {
            let password = profile.get("password").and_then(|p| p.as_str()).unwrap_or("");
            let res = handle
                .authenticate_password(username, password)
                .await
                .map_err(|e| e.to_string())?;
            Ok(if res.success() {
                AuthenticationResult::Authenticated
            } else {
                AuthenticationResult::NeedsFreshKeyboardInteractive
            })
        }
        "privateKey" => {
            let (key_content, passphrase) = if let Some(key_id) = profile.get("privateKeyId").and_then(|value| value.as_str()) {
                resolve_managed_private_key(app, tab_id, &profile_id, key_id).await?
            } else {
                let private_key_path = profile
                    .get("privateKeyPath")
                    .and_then(|p| p.as_str())
                    .unwrap_or("");
                let mut resolved = private_key_path.to_string();
                if resolved.starts_with("~/") || resolved == "~" {
                    if let Ok(home) = app.path().home_dir() {
                        let rest = if resolved == "~" { "" } else { &resolved[2..] };
                        resolved = home.join(rest).to_string_lossy().into_owned();
                    }
                }
                (
                    std::fs::read_to_string(&resolved).map_err(|error| error.to_string())?,
                    profile.get("passphrase").and_then(|value| value.as_str()).map(ToOwned::to_owned),
                )
            };

            let key_pair = russh::keys::decode_secret_key(&key_content, passphrase.as_deref())
                .map_err(|e| e.to_string())?;
            // Best-effort: pick the strongest RSA hash the server advertises.
            // For non-RSA keys, hash_alg is ignored by PrivateKeyWithHashAlg::new.
            let hash_alg: Option<russh::keys::HashAlg> = if key_pair.algorithm().is_rsa() {
                match handle.best_supported_rsa_hash().await {
                    Ok(Some(Some(h))) => Some(h),
                    _ => Some(russh::keys::HashAlg::Sha512),
                }
            } else {
                None
            };
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg);
            let res = handle
                .authenticate_publickey(username, key_with_hash)
                .await
                .map_err(|e| e.to_string())?;
            Ok(if res.success() {
                AuthenticationResult::Authenticated
            } else if profile.get("password").and_then(Value::as_str).is_some() {
                AuthenticationResult::NeedsFreshKeyboardInteractive
            } else {
                AuthenticationResult::Rejected
            })
        }
        "keyboard-interactive" => {
            let password = profile.get("password").and_then(Value::as_str).unwrap_or("");
            Ok(if try_keyboard_interactive(
                handle,
                username,
                password,
                app,
                tab_id,
                &profile_id,
                &host,
                port,
            )
            .await?
            {
                AuthenticationResult::Authenticated
            } else {
                AuthenticationResult::Rejected
            })
        }
        _ => {
            // agent
            let mut agent = russh::keys::agent::client::AgentClient::connect_env()
                .await
                .map_err(|e| e.to_string())?;
            let identities = agent
                .request_identities()
                .await
                .map_err(|e| e.to_string())?;
            for identity in identities {
                let pub_key = identity.public_key().into_owned();
                let res = handle
                    .authenticate_publickey_with(username, pub_key, None, &mut agent)
                    .await
                    .map_err(|e| e.to_string())?;
                if res.success() {
                    return Ok(AuthenticationResult::Authenticated);
                }
            }
            Ok(AuthenticationResult::Rejected)
        }
    }
}

async fn resolve_managed_private_key(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    key_id: &str,
) -> Result<(String, Option<String>), String> {
    let managed = crate::services::ssh_keys::resolve(app, key_id).map_err(|error| error.to_string())?;
    if !managed.key.encrypted {
        return Ok((managed.private_key, None));
    }

    let mut reason = "required";
    if let Some(saved) = managed.saved_passphrase {
        if russh::keys::decode_secret_key(&managed.private_key, Some(&saved)).is_ok() {
            return Ok((managed.private_key, Some(saved)));
        }
        crate::services::ssh_keys::set_passphrase(app, &managed.key.id, None)
            .map_err(|error| error.to_string())?;
        reason = "invalid-saved";
    }

    let response = request_key_passphrase(
        app,
        tab_id,
        profile_id,
        &managed.key.id,
        &managed.key.name,
        reason,
    )
    .await?
    .ok_or_else(|| "SSH key passphrase request canceled".to_string())?;
    if russh::keys::decode_secret_key(&managed.private_key, Some(&response.0)).is_err() {
        return Err("私钥口令不正确。".to_string());
    }
    if response.1 {
        crate::services::ssh_keys::set_passphrase(app, &managed.key.id, Some(response.0.clone()))
            .map_err(|error| error.to_string())?;
    }
    Ok((managed.private_key, Some(response.0)))
}

async fn request_key_passphrase(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    key_id: &str,
    key_name: &str,
    reason: &str,
) -> Result<Option<(String, bool)>, String> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<Value>();
    {
        let state = app.state::<crate::services::workspace::WorkspaceState>();
        state.pending_interactions.write().await.insert(request_id.clone(), tx);
    }
    app.emit(
        "ssh:interaction",
        serde_json::json!({
            "requestId": request_id,
            "kind": "key-passphrase",
            "tabId": tab_id,
            "profileId": profile_id,
            "keyId": key_id,
            "keyName": key_name,
            "reason": reason,
        }),
    )
    .map_err(|error| error.to_string())?;
    match rx.await {
        Ok(response) if !response.get("canceled").and_then(|value| value.as_bool()).unwrap_or(false) => {
            let passphrase = response
                .get("passphrase")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(passphrase.map(|value| (
                value,
                response.get("savePassphrase").and_then(|item| item.as_bool()).unwrap_or(false),
            )))
        }
        _ => Ok(None),
    }
}

async fn try_keyboard_interactive(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    password: &str,
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    host: &str,
    port: u16,
) -> Result<bool, String> {
    let app = app.clone();
    let tab_id = tab_id.to_string();
    let profile_id = profile_id.to_string();
    let host = host.to_string();
    try_keyboard_interactive_with_responder(handle, username, password, move |request| {
        let app = app.clone();
        let tab_id = tab_id.clone();
        let profile_id = profile_id.clone();
        let host = host.clone();
        async move {
            let request_id = uuid::Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel::<Value>();
            {
                let state = app.state::<crate::services::workspace::WorkspaceState>();
                let mut pending = state.pending_interactions.write().await;
                pending.insert(request_id.clone(), tx);
            }
            let _ = app.emit(
                "ssh:interaction",
                serde_json::json!({
                    "requestId": request_id,
                    "kind": "keyboard-interactive",
                    "tabId": tab_id,
                    "profileId": profile_id,
                    "host": host,
                    "port": port,
                    "name": request.name,
                    "instructions": request.instructions,
                    "prompts": request.prompts.into_iter().map(|prompt| serde_json::json!({
                        "prompt": prompt.prompt,
                        "echo": prompt.echo,
                    })).collect::<Vec<_>>(),
                }),
            );
            match rx.await {
                Ok(response)
                    if !response
                        .get("canceled")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false) => response.get("answers").and_then(|answers| {
                        answers.as_array().map(|answers| {
                            answers
                                .iter()
                                .map(|answer| answer.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                    }),
                _ => None,
            }
        }
    })
    .await
}

/// Run SSH keyboard-interactive authentication and ask a caller to supply
/// only prompts that cannot safely use the profile password. Keeping this
/// protocol loop separate from Tauri events makes its MFA behaviour directly
/// testable against a real SSH server implementation.
async fn try_keyboard_interactive_with_responder<H, F, Fut>(
    handle: &mut Handle<H>,
    username: &str,
    password: &str,
    mut request_answers: F,
) -> Result<bool, String>
where
    H: Handler,
    F: FnMut(KeyboardInteractiveRequest) -> Fut,
    Fut: Future<Output = Option<Vec<String>>>,
{
    let res = handle
        .authenticate_keyboard_interactive_start(username, None)
        .await
        .map_err(|e| e.to_string())?;

    let mut current = match res {
        russh::client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
        russh::client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
        russh::client::KeyboardInteractiveAuthResponse::InfoRequest {
            name,
            instructions,
            prompts,
        } => KeyboardInteractiveRequest {
            name,
            instructions,
            prompts: prompts
                .into_iter()
                .map(|prompt| KeyboardInteractivePrompt {
                    prompt: prompt.prompt,
                    echo: prompt.echo,
                })
                .collect(),
        },
    };

    // Multi-round OTP loop: a stored password is reused only for a
    // password-like prompt. MFA/OTP prompts are surfaced immediately, even
    // when a server sends password and second-factor prompts in one round.
    // This matches the operational behavior proven in meatshell and avoids
    // silently submitting the account password as an OTP.
    let mut password_used = false;
    for _ in 0..16 {
        let mut answers = vec![String::new(); current.prompts.len()];
        let mut pending_indexes = Vec::new();
        let mut pending_prompts = Vec::new();
        for (index, prompt) in current.prompts.iter().enumerate() {
            if !password_used && !password.is_empty() && is_password_prompt(&prompt.prompt) {
                answers[index] = password.to_string();
                password_used = true;
            } else {
                pending_indexes.push(index);
                pending_prompts.push(prompt.clone());
            }
        }

        if !pending_prompts.is_empty() {
            let Some(supplied_answers) = request_answers(KeyboardInteractiveRequest {
                name: current.name.clone(),
                instructions: current.instructions.clone(),
                prompts: pending_prompts,
            })
            .await
            else {
                return Ok(false);
            };
            if supplied_answers.len() != pending_indexes.len() {
                return Ok(false);
            }
            for (index, answer) in pending_indexes.into_iter().zip(supplied_answers) {
                answers[index] = answer;
            }
        }

        let res = handle
            .authenticate_keyboard_interactive_respond(answers)
            .await
            .map_err(|e| e.to_string())?;
        current = match res {
            russh::client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            russh::client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            russh::client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => KeyboardInteractiveRequest {
                name,
                instructions,
                prompts: prompts
                    .into_iter()
                    .map(|prompt| KeyboardInteractivePrompt {
                        prompt: prompt.prompt,
                        echo: prompt.echo,
                    })
                    .collect(),
            },
        };
    }
    Ok(false)
}

fn is_password_prompt(prompt: &str) -> bool {
    let normalized = prompt.to_lowercase();
    !looks_like_mfa_prompt(&normalized)
        && (normalized.contains("password") || normalized.contains("密码"))
}

fn looks_like_mfa_prompt(prompt: &str) -> bool {
    [
        "code",
        "otp",
        "mfa",
        "2fa",
        "factor",
        "duo",
        "verification",
        "verify",
        "token",
        "authenticator",
        "passcode",
        "one-time",
        "one time",
        "验证码",
        "动态",
        "令牌",
    ]
    .iter()
    .any(|needle| prompt.contains(needle))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    for i in 0..=haystack.len() - needle.len() {
        if haystack[i..i + needle.len()] == *needle {
            return Some(i);
        }
    }
    None
}

const SFTP_UNAVAILABLE_FALLBACK: &str =
    "SFTP 文件通道不可用；终端和 SSH 隧道仍可继续使用。请在服务器启用或修复 sftp subsystem 后重新连接。";

/// Open the SFTP subsystem on an already authenticated SSH handle.
///
/// `russh-sftp` deliberately does not send the subsystem request itself, so
/// this boundary is also where we can distinguish a file-channel failure from
/// a terminal-session failure.
/// SFTP 初始化每一步的最大等待时间。
///
/// 这非常关键：`open_sftp_session` 在 worker 主 select! 循环之前调用，
/// 任何一步阻塞都会让整个 worker 启动不了——cmd_rx 队列堆满后所有
/// `app_write_terminal` 调用全部永久阻塞，表现为终端无法输入、多窗口
/// 发送整体卡死、Cmd+Q 退出也退不掉。服务器拒绝 sftp subsystem 时
/// russh-sftp 内部超时往往很长（30s+），这里强制收口到 8 秒。
const SFTP_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// sudo 凭据验证超时。`exec_shell_file_command` 用 PTY 模式 exec，sudo
/// 密码错误时会重新 prompt 等待输入且不会自然退出，channel.wait() 永久
/// 阻塞。这里强制 10 秒收口，让前端 RootAccessModal 的 loading 状态能
/// 在合理时间内解除。
const SUDO_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);

/// SFTP / exec 文件操作超时。
///
/// 这非常关键：worker 主循环是单 task 顺序处理 cmd 的，一个 ListRemoteFiles
/// / ReadRemoteFile 卡住会阻塞整个 select! 循环，cmd_rx.recv() 不被 poll，
/// 新来的 WriteTerminal 命令堆积直到 channel 满（100），之后所有
/// app_write_terminal 超时丢弃——终端和悬浮窗都无法输入。
///
/// SFTP read_dir / open 在网络抖动或服务器 SFTP subsystem 失效时可能
/// 长时间不返回，必须强制收口。
const FILE_OPERATION_TIMEOUT: Duration = Duration::from_secs(15);

async fn open_sftp_session(handle: &Handle<ClientHandler>) -> Result<SftpSession, String> {
    let sftp_channel = timeout(SFTP_INIT_STEP_TIMEOUT, handle.channel_open_session())
        .await
        .map_err(|_| "SFTP init failed: 打开 channel 超时".to_string())?
        .map_err(|error| format!("无法打开 SFTP channel: {error}"))?;
    timeout(SFTP_INIT_STEP_TIMEOUT, sftp_channel.request_subsystem(true, "sftp"))
        .await
        .map_err(|_| "SFTP init failed: 请求 subsystem 超时".to_string())?
        .map_err(|error| format!("SFTP subsystem request failed: {error}"))?;
    timeout(SFTP_INIT_STEP_TIMEOUT, SftpSession::new(sftp_channel.into_stream()))
        .await
        .map_err(|_| "SFTP init failed: 协议握手超时".to_string())?
        .map_err(|error| format!("SFTP init failed: {error}"))
}

/// Convert a russh SFTP handshake error into an actionable, non-ambiguous
/// renderer message. A timeout here happens after the interactive shell is
/// established, so it must not be presented as a failed SSH login.
fn format_sftp_unavailable_reason(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        format!(
            "SFTP 子系统在初始化期间没有响应。SSH 终端已连接，服务器可能禁用或拒绝了 sftp subsystem；请在服务器启用/修复 SFTP 后重连。原始错误: {error}"
        )
    } else {
        format!(
            "SFTP 文件通道不可用（{error}）。SSH 终端和隧道仍可使用；请在服务器启用/修复 SFTP 后重连。"
        )
    }
}

fn sftp_unavailable_result<T>(reason: &str) -> Result<T, String> {
    Err(reason.to_string())
}

async fn run_worker_loop(
    tab_id: &str,
    profile: &Value,
    cmd_rx: &mut mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    cancellation: CancellationToken,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("127.0.0.1")
        .to_string();
    let port = profile.get("port").and_then(|p| p.as_i64()).unwrap_or(22) as u16;
    let username = profile
        .get("username")
        .and_then(|u| u.as_str())
        .unwrap_or("root")
        .to_string();

    // ── Main session (single SSH session multiplexes shell + SFTP + metrics) ─
    // Servers with strict MaxSessions reject parallel sessions, so we reuse
    // one authenticated handle for every channel. The handle is wrapped in
    // `Arc` so the background metrics task can share it with the main loop.
    let handle: Arc<Handle<ClientHandler>> = Arc::new(open_session(profile, app, tab_id).await?);

    // ── Shell channel ──────────────────────────────────────────────────────
    let mut shell_channel = handle
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    shell_channel
        .request_pty(
            true,
            "xterm-256color",
            80,
            24,
            0,
            0,
            &[(russh::Pty::TTY_OP_ISPEED, 115200), (russh::Pty::TTY_OP_OSPEED, 115200)],
        )
        .await
        .map_err(|e| e.to_string())?;
    shell_channel
        .request_shell(true)
        .await
        .map_err(|e| e.to_string())?;

    // ── Probe platform ─────────────────────────────────────────────────────
    let platform = super::system_metrics::probe_remote_platform(&handle).await;
    eprintln!(
        "[SSH metrics] tab={} platform='{}' — probe completed",
        tab_id, platform
    );

    // ── Inject shell CWD setup (POSIX only, fail-closed) ───────────────────
    // Mirrors Electron's `supportsPosixShellSetup()` + `injectShellSetup()`
    // double gate. Only `linux` / `busybox` get the OSC7/RemoteUser hook
    // injected; Windows / unknown are left untouched so we never push a
    // POSIX script into a non-POSIX shell.
    let mut pending_shell_setup_echo = None;
    if let Some(setup) = shell_cwd_setup_for_platform(&platform) {
        eprintln!(
            "[SSH shell-setup] tab={} platform='{}' — injecting CWD hook ({} bytes)",
            tab_id,
            platform,
            setup.len()
        );
        // An interactive shell only executes submitted input after CR/LF. The
        // previous port sent the script without a terminator, leaving it on
        // the prompt as visible text and preventing OSC7/RemoteUser updates.
        let setup_command = format!(" {setup}\r");
        if let Err(e) = shell_channel.data(setup_command.as_bytes()).await {
            eprintln!(
                "[SSH shell-setup] tab={} failed to write setup: {}",
                tab_id, e
            );
        } else {
            pending_shell_setup_echo = Some(ShellSetupEchoSuppression::new(true));
        }
        // The setup script's trailing `__tdcwd` call emits an OSC7 + RemoteUser
        // pair immediately; the main loop's `track_cwd_and_user` will pick it
        // up and sync the initial CWD into the session snapshot.
    } else {
        eprintln!(
            "[SSH shell-setup] tab={} platform='{}' — skipping setup (unsupported platform)",
            tab_id, platform
        );
    }

    update_tab_status_and_emit(app, tab_id, "connected").await;

    // Emit "connected" notice so the user sees confirmation in the terminal.
    // Mirrors Electron's `appendSystemMessage('连接主机成功\r\n')`.
    emit_terminal_data(app, tab_id, "连接主机成功\r\n").await;

    // ── Initialize session snapshot ────────────────────────────────────────
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    {
        let mut sessions = state.sessions.write().await;
        let existing_transcript = sessions
            .get(tab_id)
            .map(|s| s.terminal_transcript.clone())
            .unwrap_or_default();
        sessions.insert(
            tab_id.to_string(),
            crate::services::SessionSnapshot {
                profile_id: profile
                    .get("id")
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string(),
                access_host: format!("{}:{}", host, port),
                summary: format!("{}@{}", username, host),
                terminal_transcript: existing_transcript,
                remote_path: "/".to_string(),
                shell_cwd: Some("/".to_string()),
                follow_shell_cwd: true,
                remote_files_loading: false,
                remote_files: Vec::new(),
                sftp_unavailable_reason: None,
                file_access_mode: "user".to_string(),
                sudo_user: None,
                has_reusable_sudo_auth: false,
                login_user: profile
                    .get("username")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string()),
                shell_user: None,
                connected: true,
                system_metrics: None,
            },
        );
    }

    // ── SFTP subsystem ─────────────────────────────────────────────────────
    // russh-sftp 2.3 needs an explicit subsystem request before converting
    // the channel into its protocol stream. A failed SFTP negotiation must
    // not tear down an otherwise healthy SSH shell: Electron keeps terminal
    // and tunnel features available while exposing the file-channel error.
    let (sftp_arc, sftp_unavailable_reason) = match open_sftp_session(&handle).await {
        Ok(sftp) => {
            let sftp_arc = Arc::new(RwLock::new(sftp));
            let initial_files = {
                let sftp = sftp_arc.write().await;
                list_dir(&sftp, "/").await
            };
            match initial_files {
                Ok(files) => {
                    let mut sessions = state.sessions.write().await;
                    if let Some(session) = sessions.get_mut(tab_id) {
                        session.remote_files = files;
                    }
                }
                Err(error) => {
                    // A usable SFTP channel can still lack access to `/`.
                    emit_terminal_data(
                        app,
                        tab_id,
                        &format!("\r\n[files] 列出根目录失败: {error}\r\n"),
                    )
                    .await;
                }
            }
            (Some(sftp_arc), None)
        }
        Err(error) => {
            let reason = format_sftp_unavailable_reason(&error);
            eprintln!("[SSH SFTP] tab={tab_id} unavailable: {reason}");
            crate::services::logging::ssh_debug(app, tab_id, format!("SFTP unavailable: {reason}"));
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    session.sftp_unavailable_reason = Some(reason.clone());
                }
            }
            emit_terminal_data(app, tab_id, &format!("\r\n[files] {reason}\r\n")).await;
            (None, Some(reason))
        }
    };

    // Push the full snapshot (with files) to the renderer
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }

    // ── Spawn metrics collection task (single persistent channel) ─────────
    // Instead of opening a new exec channel every second (which adds variable
    // SSH overhead and makes the refresh cadence jittery), we open one
    // long-lived shell channel and pipe an infinite-loop script into it.
    // The remote side controls the 1s cadence via `sleep 1`, so data arrives
    // at a rock-steady interval regardless of SSH RTT.
    let metrics_shutdown = Arc::new(tokio::sync::Notify::new());
    let metrics_shutdown_clone = metrics_shutdown.clone();
    {
        let metrics_handle = Arc::clone(&handle);
        let metrics_app = app.clone();
        let metrics_tid = tab_id.to_string();
        let metrics_plat = platform.clone();
        let metrics_cancellation = cancellation.clone();
        tokio::spawn(async move {
            eprintln!(
                "[SSH metrics] task spawned tab={} plat='{}' — starting streaming collector",
                metrics_tid, metrics_plat
            );

            // Build the infinite-loop script. Each iteration emits a
            // delimited metrics block and sleeps for 1 second. We use a
            // unique marker so the stream parser can reliably slice blocks.
            let marker = "__FILETERM_METRICS_BLOCK__";
            let script_body = if metrics_plat == "windows" {
                // Windows: wrap the metrics script in a loop with Start-Sleep
                let metrics = super::system_metrics::build_windows_metrics_command();
                format!(
                    r#"
while ($true) {{
{0}
  Write-Output '{1}'
  Start-Sleep -Seconds 1
}}
"#,
                    metrics, marker
                )
            } else {
                // POSIX: wrap the metrics script in a while-true loop
                let raw = if metrics_plat == "busybox" {
                    "busybox"
                } else {
                    "linux"
                };
                let metrics = super::system_metrics::build_posix_metrics_command(raw);
                format!(
                    "{}\nwhile true; do\n{}\necho '{}'\nsleep 1\ndone\n",
                    "cd / >/dev/null 2>&1 || true", metrics, marker
                )
            };

            // Open one persistent shell channel for the entire session.
            let mut channel = match metrics_handle.channel_open_session().await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "[SSH metrics] tab={} failed to open channel: {}",
                        metrics_tid, e
                    );
                    return;
                }
            };

            // Use a shell so we can pipe the script via stdin and keep it running.
            if let Err(e) = channel.request_shell(true).await {
                eprintln!(
                    "[SSH metrics] tab={} failed to request shell: {}",
                    metrics_tid, e
                );
                return;
            }

            // Feed the script into the shell
            if let Err(e) = channel.data(script_body.as_bytes()).await {
                eprintln!(
                    "[SSH metrics] tab={} failed to write script: {}",
                    metrics_tid, e
                );
                return;
            }

            eprintln!(
                "[SSH metrics] tab={} streaming collector started — waiting for first block",
                metrics_tid
            );

            // Stream reader: accumulate data, split on the marker, parse
            // each complete block and emit it to the renderer.
            let mut buffer: Vec<u8> = Vec::new();
            let marker_bytes = marker.as_bytes();

            loop {
                tokio::select! {
                    biased;
                    _ = metrics_shutdown_clone.notified() => {
                        let _ = channel.close().await;
                        break;
                    }
                    _ = metrics_cancellation.cancelled() => {
                        let _ = channel.close().await;
                        break;
                    }
                    msg = channel.wait() => {
                        match msg {
                            Some(ChannelMsg::Data { data }) => {
                                buffer.extend_from_slice(data.as_ref());
                                // Drain all complete blocks from the buffer.
                                while let Some(idx) = find_subsequence(&buffer, marker_bytes) {
                                    // A malformed or unexpectedly large process list must not
                                    // monopolize the Tokio worker and freeze the native webview.
                                    // Keep one bounded metrics sample; the next marker resumes
                                    // normal streaming collection.
                                    if idx > 256 * 1024 {
                                        buffer.drain(..idx + marker_bytes.len());
                                        continue;
                                    }
                                    let block = String::from_utf8_lossy(&buffer[..idx]).into_owned();
                                    buffer.drain(..idx + marker_bytes.len());
                                    // Parse and emit this block
                                    let val = super::system_metrics::parse_system_metrics(
                                        &block,
                                        &metrics_plat,
                                    );
                                    let cpu_pct = val.get("cpuPercent").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                                    let mem_pct = val.get("memoryPercent").and_then(|v| v.as_f64()).unwrap_or(-1.0);
                                    if cpu_pct < 0.0 && mem_pct < 0.0 {
                                        // Probably garbage / incomplete block
                                        continue;
                                    }
                                    {
                                        let state = metrics_app
                                            .state::<crate::services::workspace::WorkspaceState>();
                                        let mut sessions = state.sessions.write().await;
                                        if let Some(s) = sessions.get_mut(&metrics_tid) {
                                            s.system_metrics = Some(merge_system_metrics_history(
                                                s.system_metrics.as_ref(),
                                                val.clone(),
                                                600,
                                            ));
                                        }
                                    }
                                    let payload = serde_json::json!({
                                        "tabId": metrics_tid,
                                        "systemMetrics": val,
                                        "mode": "append",
                                    });
                                    let _ = metrics_app.emit("workspace:sessionMetrics", payload);
                                }
                                // Cap buffer to prevent unbounded growth
                                if buffer.len() > 1_000_000 {
                                    buffer.drain(..buffer.len() - 500_000);
                                }
                            }
                            Some(ChannelMsg::ExtendedData { data, .. }) => {
                                buffer.extend_from_slice(data.as_ref());
                            }
                            Some(ChannelMsg::ExitStatus { .. }) | None => {
                                eprintln!(
                                    "[SSH metrics] tab={} channel closed",
                                    metrics_tid
                                );
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            let _ = channel.close().await;
            eprintln!(
                "[SSH metrics] tab={} task ended",
                metrics_tid
            );
        });
    }

    // ── Main event loop: terminal reads + command dispatch ─────────────────
    let mut cwd_buffer = String::new();
    let mut batch_buffer: Vec<u8> = Vec::new();
    let mut last_emit = Instant::now();

    // sudo / root-mode credentials — kept in worker-local state so they
    // never leak into SessionSnapshot (which is serialized to the renderer).
    let mut file_access_mode = "user".to_string();
    let mut sudo_user: Option<String> = None;
    let mut sudo_password: Option<String> = None;
    let mut sudo_prompt_buffer = String::new();
    let mut awaiting_sudo_password = false;
    let mut pending_sudo_password = String::new();
    let mut recent_terminal_input = String::new();
    // A new `sudo -i` shell discards the login shell's PROMPT_COMMAND.  Keep
    // Electron's two-second guard so a root prompt causes one safe reinject
    // of the OSC CWD/RemoteUser hook, not an injection loop.
    let mut last_shell_setup_injection = Instant::now() - Duration::from_secs(3);

    let mut tunnel_manager = TunnelManager::new(tab_id, app, Arc::clone(&handle));
    if let Some(rules) = profile.get("forwards").and_then(Value::as_array) {
        for raw_rule in rules {
            match serde_json::from_value::<SshTunnelRule>(raw_rule.clone()) {
                Ok(rule) => {
                    let should_start = rule.auto_start;
                    if let Err(error) = tunnel_manager.register(rule.clone(), false) {
                        emit_terminal_data(app, tab_id, &format!("[tunnel] 忽略无效规则: {error}\r\n")).await;
                    } else if should_start {
                        if let Err(error) = tunnel_manager.start(&rule.id).await {
                            emit_terminal_data(app, tab_id, &format!("[tunnel] 自动启动 {} 失败: {error}\r\n", rule.id)).await;
                        }
                    }
                }
                Err(error) => emit_terminal_data(app, tab_id, &format!("[tunnel] 解析规则失败: {error}\r\n")).await,
            }
        }
    }

    loop {
        // 16ms batch window for terminal output.
        let next_batch_deadline =
            tokio::time::Instant::from_std(last_emit + Duration::from_millis(16));

        tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                flush_batch(&mut batch_buffer, app, tab_id).await;
                metrics_shutdown.notify_waiters();
                tunnel_manager.stop_all().await;
                return Ok(());
            }
            // 1. Drain pending IPC commands first so user input never waits
            //    on the network. When the sender is dropped (reconnect /
            //    disconnect / close), `recv()` returns None and we must
            //    exit — otherwise the old worker keeps emitting
            //    `terminal:data` alongside the new worker, producing
            //    duplicated echo ("clear" → "clearclear") and double
            //    newlines.
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(cmd) => {
                        if let WorkerCmd::WriteTerminal(data) = &cmd {
                            if capture_sudo_password_input(
                                data,
                                &mut awaiting_sudo_password,
                                &mut pending_sudo_password,
                                &mut recent_terminal_input,
                                &mut sudo_password,
                            ) {
                                let mut sessions = state.sessions.write().await;
                                if let Some(session) = sessions.get_mut(tab_id) {
                                    session.has_reusable_sudo_auth = sudo_password.is_some();
                                }
                            }
                        }
                        let result = if let Some(sftp) = sftp_arc.as_ref() {
                            handle_worker_cmd(
                                cmd,
                                &handle,
                                &shell_channel,
                                sftp,
                                &mut file_access_mode,
                                &mut sudo_user,
                                &mut sudo_password,
                                tab_id,
                                app,
                                &state,
                                &mut tunnel_manager,
                            ).await
                        } else {
                            handle_worker_cmd_without_sftp(
                                cmd,
                                &handle,
                                &shell_channel,
                                &mut file_access_mode,
                                &mut sudo_user,
                                &mut sudo_password,
                                tab_id,
                                app,
                                &state,
                                &mut tunnel_manager,
                                sftp_unavailable_reason.as_deref().unwrap_or(SFTP_UNAVAILABLE_FALLBACK),
                            ).await
                        };
                        match result {
                            Ok(true) => {
                                // WorkerCmd::Disconnect requested — flush and exit.
                                flush_batch(&mut batch_buffer, app, tab_id).await;
                                metrics_shutdown.notify_waiters();
                                tunnel_manager.stop_all().await;
                                return Ok(());
                            }
                            Ok(false) => {}
                            Err(e) => {
                                eprintln!("[SSH Worker] cmd error for tab {}: {}", tab_id, e);
                            }
                        }
                    }
                    None => {
                        // Sender dropped — flush and exit cleanly.
                        flush_batch(&mut batch_buffer, app, tab_id).await;
                        metrics_shutdown.notify_waiters();
                        tunnel_manager.stop_all().await;
                        return Ok(());
                    }
                }
            }
            _ = async {
                if let Some(deadline) = shell_setup_release_deadline(&pending_shell_setup_echo) {
                    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            }, if pending_shell_setup_echo.is_some() => {
                let visible = finish_shell_setup_suppression(&mut pending_shell_setup_echo);
                if !visible.is_empty() {
                    batch_buffer.extend_from_slice(visible.as_bytes());
                }
            }
            // 2. Drain shell channel output.
            msg = shell_channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        let bytes = data.as_ref();
                        let text = String::from_utf8_lossy(bytes);
                        if track_sudo_prompt_from_terminal(
                            &text,
                            &mut sudo_prompt_buffer,
                            &mut awaiting_sudo_password,
                            &mut pending_sudo_password,
                            &mut sudo_password,
                        ) {
                            let mut sessions = state.sessions.write().await;
                            if let Some(session) = sessions.get_mut(tab_id) {
                                session.has_reusable_sudo_auth = false;
                            }
                        }
                        let (new_cwd, new_user) = track_cwd_and_user(&text, &mut cwd_buffer);
                        let mut cwd_to_follow = None;
                        let mut file_mode_switch: Option<(String, Option<String>)> = None;
                        if new_cwd.is_some() || new_user.is_some() {
                            let mut sessions = state.sessions.write().await;
                            if let Some(s) = sessions.get_mut(tab_id) {
                                if let Some(cwd) = new_cwd {
                                    if s.shell_cwd.as_deref() != Some(cwd.as_str()) {
                                        s.shell_cwd = Some(cwd.clone());
                                        if s.follow_shell_cwd {
                                            cwd_to_follow = Some(cwd);
                                        }
                                    }
                                }
                                if let Some(user) = &new_user {
                                    // 首次观察到 RemoteUser 时记录为 login_user
                                    // （若 profile.username 不可用则用观察值）。
                                    if s.login_user.is_none() {
                                        s.login_user = Some(user.clone());
                                    }
                                    // shell_user 始终更新为最新观察值
                                    if s.shell_user.as_deref() != Some(user.as_str()) {
                                        s.shell_user = Some(user.clone());
                                        // 对照 Electron resolveShellFileAccess：
                                        // shell user != login user ⇒ 自动切 root 视角
                                        // shell user == login user ⇒ 切回 user 视角
                                        let login = s.login_user.clone();
                                        if let Some(login_user) = login {
                                            if user.as_str() != login_user.as_str() {
                                                // sudo -i / su 切到其他用户：自动启用 root 视角
                                                if s.file_access_mode != "root" {
                                                    s.file_access_mode = "root".to_string();
                                                    // sudo_user 用观察到的 shell user（如 "root"）
                                                    s.sudo_user = Some(user.clone());
                                                    s.has_reusable_sudo_auth = sudo_password.is_some();
                                                    file_mode_switch = Some(("root".to_string(), Some(user.clone())));
                                                    // `sudo -i` normally keeps the same CWD. Refresh
                                                    // anyway so the file pane switches its access model
                                                    // instead of waiting for a subsequent `cd`.
                                                    cwd_to_follow = s.shell_cwd.clone();
                                                }
                                            } else if s.file_access_mode == "root" && !s.has_reusable_sudo_auth {
                                                // 切回登录用户：若没有用户显式配置的 sudo 凭据，
                                                // 自动切回 user 视角
                                                s.file_access_mode = "user".to_string();
                                                file_mode_switch = Some(("user".to_string(), None));
                                            }
                                        }
                                    }
                                }
                            }
                            drop(sessions);
                            // Keep worker-local auth/access state in lockstep
                            // before dispatching the follow task below.
                            if let Some((mode, su_user)) = file_mode_switch {
                                file_access_mode = mode;
                                sudo_user = su_user;
                            }
                            if let (Some(cwd), Some(sftp)) = (cwd_to_follow, sftp_arc.as_ref()) {
                                tokio::spawn(follow_shell_cwd(
                                    app.clone(),
                                    tab_id.to_string(),
                                    cwd,
                                    Arc::clone(sftp),
                                    Arc::clone(&handle),
                                    file_access_mode.clone(),
                                    sudo_user.clone(),
                                    sudo_password.clone(),
                                ));
                            } else if let Ok(snap) = crate::commands::get_workspace_snapshot(app.clone()).await {
                                let _ = app.emit("workspace:snapshot", snap);
                            }
                        }

                        let visible = suppress_shell_setup_echo(&mut pending_shell_setup_echo, &text);
                        if !visible.is_empty() {
                            batch_buffer.extend_from_slice(visible.as_bytes());
                        }

                        if pending_shell_setup_echo.is_none()
                            && shell_cwd_setup_for_platform(&platform).is_some()
                            && looks_like_root_prompt(&text)
                            && last_shell_setup_injection.elapsed() > Duration::from_secs(2)
                        {
                            let shell_is_root = state
                                .sessions
                                .read()
                                .await
                                .get(tab_id)
                                .and_then(|session| session.shell_user.as_deref())
                                == Some("root");
                            if !shell_is_root {
                                if let Some(setup) = shell_cwd_setup_for_platform(&platform) {
                                    last_shell_setup_injection = Instant::now();
                                    if shell_channel.data(format!(" {setup}\r").as_bytes()).await.is_ok() {
                                        pending_shell_setup_echo = Some(ShellSetupEchoSuppression::new(false));
                                    }
                                }
                            }
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        batch_buffer.extend_from_slice(data.as_ref());
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        // Shell closed → flush and disconnect.
                        flush_batch(&mut batch_buffer, app, tab_id).await;
                        metrics_shutdown.notify_waiters();
                        tunnel_manager.stop_all().await;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            // 3. Periodic flush if there is buffered output.
            _ = tokio::time::sleep_until(next_batch_deadline) => {
                if !batch_buffer.is_empty() {
                    flush_batch(&mut batch_buffer, app, tab_id).await;
                    last_emit = Instant::now();
                } else {
                    last_emit = Instant::now();
                }
            }
        }
    }
}

/// Returns `Ok(true)` when the worker should exit (Disconnect requested),
/// `Ok(false)` otherwise.
///
/// When a server accepts an SSH shell but refuses its `sftp` subsystem, keep
/// terminal and tunnel commands operational. Every file operation is replied
/// to immediately with the cached handshake failure instead of falling back
/// to shell commands or leaving the caller waiting on a nonexistent channel.
async fn handle_worker_cmd_without_sftp(
    cmd: WorkerCmd,
    handle: &Handle<ClientHandler>,
    shell_channel: &Channel<russh::client::Msg>,
    file_access_mode: &mut String,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &crate::services::workspace::WorkspaceState,
    tunnel_manager: &mut TunnelManager,
    unavailable_reason: &str,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::WriteTerminal(data) => {
            shell_channel
                .data(data.as_bytes())
                .await
                .map_err(|error| error.to_string())?;
            Ok(false)
        }
        WorkerCmd::ResizeTerminal { cols, rows, .. } => {
            shell_channel
                .window_change(cols, rows, 0, 0)
                .await
                .map_err(|error| error.to_string())?;
            Ok(false)
        }
        WorkerCmd::ListSshTunnels { respond_to } => {
            let _ = respond_to.send(tunnel_manager.list());
            Ok(false)
        }
        WorkerCmd::CreateSshTunnel { rule, respond_to } => {
            let result = match serde_json::from_value::<SshTunnelRule>(rule) {
                Ok(rule) => tunnel_manager.create(rule).await,
                Err(error) => Err(format!("Invalid tunnel rule: {error}")),
            };
            let _ = respond_to.send(result);
            Ok(false)
        }
        WorkerCmd::StartSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.start(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::StopSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.stop(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::DeleteSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.delete(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::ListRemoteFiles { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::ReadRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile { respond_to, .. }
        | WorkerCmd::CreateRemoteDirectory { respond_to, .. }
        | WorkerCmd::CreateRemoteFile { respond_to, .. }
        | WorkerCmd::CopyRemotePath { respond_to, .. }
        | WorkerCmd::MoveRemotePath { respond_to, .. }
        | WorkerCmd::RenameRemotePath { respond_to, .. }
        | WorkerCmd::DeleteRemotePath { respond_to, .. }
        | WorkerCmd::ChangeRemotePermissions { respond_to, .. }
        | WorkerCmd::UploadLocalFile { respond_to, .. }
        | WorkerCmd::DownloadRemoteFile { respond_to, .. }
        | WorkerCmd::ReplaceRemoteFile { respond_to, .. }
        | WorkerCmd::RemoveRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::StatRemoteFile { respond_to, .. } => {
            let _ = respond_to.send(sftp_unavailable_result(unavailable_reason));
            Ok(false)
        }
        WorkerCmd::SetRemoteFileAccessMode {
            mode,
            sudo_user: new_sudo_user,
            sudo_password: new_sudo_password,
            respond_to,
        } => {
            // root 模式走 exec channel（handle.channel_open_session().exec()），
            // 不依赖 SFTP subsystem。即使用户的服务器拒绝 SFTP（Timeout /
            // disabled），root 视角的文件操作仍可通过 sudo + exec 完成。
            // 对照 Electron verifyRootFileAccess 先验证凭据。
            let prev_sudo_user = sudo_user.clone();
            let prev_sudo_password = sudo_password.clone();
            let prev_mode = file_access_mode.clone();

            *sudo_user = new_sudo_user.clone();
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd);
                }
            } else {
                *sudo_password = None;
            }

            if mode == "root" {
                let verify = exec_shell_file_command(handle, "true", sudo_user, sudo_password).await;
                if let Err(err) = verify {
                    *file_access_mode = prev_mode;
                    *sudo_user = prev_sudo_user;
                    *sudo_password = prev_sudo_password;
                    let _ = respond_to.send(Err(err));
                    return Ok(false);
                }
            }

            *file_access_mode = mode.clone();
            let has_reusable = sudo_password.is_some();
            let su_user = sudo_user.clone();
            let mut sessions = state.sessions.write().await;
            if let Some(s) = sessions.get_mut(tab_id) {
                s.file_access_mode = mode;
                s.sudo_user = su_user;
                s.has_reusable_sudo_auth = has_reusable;
            }
            let _ = respond_to.send(Ok(()));
            Ok(false)
        }
        WorkerCmd::Disconnect => {
            let _ = app.emit(
                "terminal:data",
                serde_json::json!({
                    "tabId": tab_id,
                    "chunk": "",
                }),
            );
            Ok(true)
        }
    }
}

/// Returns `Ok(true)` when the worker should exit (Disconnect requested),
/// `Ok(false)` otherwise.
///
/// 文件操作（List/Read/Write/Upload/Download/...）通过 `tokio::spawn` 分发到
/// 独立任务执行，主循环立即返回继续处理终端输入。这样单个慢速 SFTP 操作
/// 不会阻塞 `cmd_rx` 接收新的 `WriteTerminal` 命令——这是用户反馈"点上传
/// 后终端和文件都卡住"问题的根本修复。
#[allow(clippy::too_many_arguments)]
async fn handle_worker_cmd(
    cmd: WorkerCmd,
    handle: &Arc<Handle<ClientHandler>>,
    shell_channel: &Channel<russh::client::Msg>,
    sftp: &Arc<RwLock<SftpSession>>,
    file_access_mode: &mut String,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    tunnel_manager: &mut TunnelManager,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::WriteTerminal(data) => {
            let bytes = data.as_bytes().to_vec();
            shell_channel
                .data(&bytes[..])
                .await
                .map_err(|e| e.to_string())?;
            Ok(false)
        }
        WorkerCmd::ResizeTerminal { cols, rows, .. } => {
            shell_channel
                .window_change(cols, rows, 0, 0)
                .await
                .map_err(|e| e.to_string())?;
            Ok(false)
        }
        WorkerCmd::ListSshTunnels { respond_to } => {
            let _ = respond_to.send(tunnel_manager.list());
            Ok(false)
        }
        WorkerCmd::CreateSshTunnel { rule, respond_to } => {
            let result = match serde_json::from_value::<SshTunnelRule>(rule) {
                Ok(rule) => tunnel_manager.create(rule).await,
                Err(error) => Err(format!("Invalid tunnel rule: {error}")),
            };
            let _ = respond_to.send(result);
            Ok(false)
        }
        WorkerCmd::StartSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.start(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::StopSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.stop(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::DeleteSshTunnel { rule_id, respond_to } => {
            let _ = respond_to.send(tunnel_manager.delete(&rule_id).await);
            Ok(false)
        }
        WorkerCmd::StatRemoteFile { path, respond_to } => {
            // stat 也可能因 SFTP 卡住而阻塞，spawn 避免影响主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    stat_root_remote_file(&handle, &path, &su, &sp).await
                } else {
                    let sftp_guard = sftp.write().await;
                    match sftp_guard.metadata(&path).await {
                        Ok(metadata) if metadata.is_dir() => Ok(None),
                        Ok(metadata) => Ok(Some(TransferFileStat {
                            size: metadata.size.unwrap_or(0),
                            modified_at: metadata.mtime.map(|value| value as u64 * 1000),
                        })),
                        Err(error) if error.to_string().to_lowercase().contains("no such file") => Ok(None),
                        Err(error) => Err(error.to_string()),
                    }
                };
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::UploadLocalFile {
            local_path,
            remote_path,
            resume_offset,
            transfer_id,
            cancel,
            respond_to,
        } => {
            // 上传可能持续数分钟，必须 spawn 到独立任务否则会阻塞整个 worker
            // 主循环，导致终端输入和文件浏览全部卡住。
            let sftp = Arc::clone(sftp);
            let app = app.clone();
            tokio::spawn(async move {
                // root 与 user 模式走相同的 SFTP 上传路径（root 模式的 atomic
                // commit 由后续 ReplaceRemoteFile 完成）。
                let sftp_guard = sftp.write().await;
                let result = upload_local_file(
                    &sftp_guard,
                    &local_path,
                    &remote_path,
                    resume_offset,
                    &transfer_id,
                    cancel,
                    &app,
                ).await;
                drop(sftp_guard);
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::DownloadRemoteFile {
            remote_path,
            local_path,
            resume_offset,
            transfer_id,
            cancel,
            respond_to,
        } => {
            // 下载同样可能持续数分钟，必须 spawn。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let app = app.clone();
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    download_root_remote_file(
                        &handle,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                        &su,
                        &sp,
                    ).await
                } else {
                    let sftp_guard = sftp.write().await;
                    download_remote_file(
                        &sftp_guard,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                    ).await
                };
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::ReplaceRemoteFile {
            partial_path,
            destination_path,
            respond_to,
        } => {
            // root 模式下需要 exec sudo mv，可能因 sudo 验证或大文件 rename 慢，
            // spawn 避免阻塞主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    replace_root_remote_file(&handle, &partial_path, &destination_path, &su, &sp).await
                } else {
                    let sftp_guard = sftp.write().await;
                    replace_remote_file(&sftp_guard, &partial_path, &destination_path).await
                };
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::RemoveRemoteFile { path, respond_to } => {
            // 单文件删除通常很快，但 SFTP 通道可能因前序操作卡住，spawn 避免
            // 阻塞主循环。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(
                        &handle,
                        &format!("rm -f -- {}", shell_quote(&path)),
                        &su,
                        &sp,
                    )).await {
                        Ok(inner) => inner.map(|_| ()),
                        Err(_) => Err(format!("删除{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, async {
                        match sftp_guard.remove_file(&path).await {
                            Ok(()) => Ok(()),
                            Err(error) if error.to_string().to_lowercase().contains("no such file") => Ok(()),
                            Err(error) => Err(error.to_string()),
                        }
                    }).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("删除{}超时", path)),
                    }
                };
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::ListRemoteFiles { path, respond_to } => {
            // spawn 避免阻塞主循环；timeout 防止 SFTP 卡住时任务永久挂起。
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_list_dir_via_shell(&handle, &path, &su, &sp)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程文件夹{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, list_dir(&sftp_guard, &path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("打开远程目录{}超时", path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::ReadRemoteFile { path, encoding, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_read_file_via_shell(&handle, &path, &encoding, &su, &sp)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, read_file(&sftp_guard, &path, &encoding)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile { path, content, encoding, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_write_file_via_shell(&handle, &path, &content, &encoding, &su, &sp)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, write_file(&sftp_guard, &path, &content, &encoding)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteDirectory { parent_path, name, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(
                        &handle,
                        &format!("mkdir -p {}", shell_quote(&full_path)),
                        &su,
                        &sp,
                    )).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, create_dir(&sftp_guard, &full_path)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteFile { parent_path, name, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_write_file_via_shell(&handle, &full_path, "", "utf-8", &su, &sp)).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, write_file(&sftp_guard, &full_path, "", "utf-8")).await {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CopyRemotePath { target_path, destination_path, target_type, respond_to } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let dest_dir = std::path::Path::new(&destination_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let cp_cmd = if target_type == "folder" { "cp -R" } else { "cp" };
                let cmd_str = format!(
                    "mkdir -p {} && {} {} {}",
                    shell_quote(&dest_dir),
                    cp_cmd,
                    shell_quote(&target_path),
                    shell_quote(&destination_path)
                );
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(&handle, &cmd_str, &su, &sp)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                } else {
                    match timeout(FILE_OPERATION_TIMEOUT, super::system_metrics::exec_command(&handle, &cmd_str)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::MoveRemotePath { target_path, destination_path, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(
                        &handle,
                        &format!("mv {} {}", shell_quote(&target_path), shell_quote(&destination_path)),
                        &su,
                        &sp,
                    )).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, sftp_guard.rename(&target_path, &destination_path)).await {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::RenameRemotePath { target_path, new_name, respond_to } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let parent = std::path::Path::new(&target_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let dest = format!("{}/{}", parent.trim_end_matches('/'), new_name);
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(
                        &handle,
                        &format!("mv {} {}", shell_quote(&target_path), shell_quote(&dest)),
                        &su,
                        &sp,
                    )).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, sftp_guard.rename(&target_path, &dest)).await {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::DeleteRemotePath { target_path, target_type, respond_to } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let cmd_str = if target_type == "folder" {
                    format!("rm -rf {}", shell_quote(&target_path))
                } else {
                    format!("rm -f {}", shell_quote(&target_path))
                };
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(&handle, &cmd_str, &su, &sp)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                } else {
                    match timeout(FILE_OPERATION_TIMEOUT, super::system_metrics::exec_command(&handle, &cmd_str)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::ChangeRemotePermissions { target_path, permissions, recursive, apply_to, respond_to } => {
            // Mirrors Electron's `changeRemotePermissions`:
            // - `apply_to='all'` → `chmod -R` for recursive, plain `chmod` otherwise
            // - `apply_to='files'` → `chmod <mode> <path>` + `find <path> -type f -exec chmod <mode> {} +`
            // - `apply_to='directories'` → `chmod <mode> <path>` + `find <path> -type d -exec chmod <mode> {} +`
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let mode_str = format!("{:o}", permissions);
                let cmd_str = if !recursive {
                    format!("chmod {} {}", mode_str, shell_quote(&target_path))
                } else {
                    match apply_to.as_str() {
                        "files" => format!(
                            "chmod {} {} && find {} -type f -exec chmod {} {} +",
                            mode_str, shell_quote(&target_path),
                            shell_quote(&target_path), mode_str, "{}"
                        ),
                        "directories" => format!(
                            "chmod {} {} && find {} -type d -exec chmod {} {} +",
                            mode_str, shell_quote(&target_path),
                            shell_quote(&target_path), mode_str, "{}"
                        ),
                        _ => format!("chmod -R {} {}", mode_str, shell_quote(&target_path)),
                    }
                };
                let res = if fam == "root" {
                    match timeout(FILE_OPERATION_TIMEOUT, exec_shell_file_command(&handle, &cmd_str, &su, &sp)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                } else {
                    let wrapped = format!("sh -lc {}", shell_quote(&cmd_str));
                    match timeout(FILE_OPERATION_TIMEOUT, super::system_metrics::exec_command(&handle, &wrapped)).await {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::SetRemoteFileAccessMode {
            mode,
            sudo_user: new_sudo_user,
            sudo_password: new_sudo_password,
            respond_to,
        } => {
            // 对照 Electron verifyRootFileAccess：切到 root 前先验证 sudo 凭据
            // 可用，失败则回滚状态并返回错误，让用户在弹窗里立即看到反馈，
            // 而不是等到第一次文件操作才失败（用户会以为"root 切换没接入"）。
            let prev_sudo_user = sudo_user.clone();
            let prev_sudo_password = sudo_password.clone();
            let prev_mode = file_access_mode.clone();

            *sudo_user = new_sudo_user.clone();
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd);
                }
                // empty password ⇒ keep existing (cache reuse)
            } else {
                // No password provided — fall back to `sudo -n`.
                *sudo_password = None;
            }

            if mode == "root" {
                // 先验证 sudo 凭据，失败则回滚
                let verify = exec_shell_file_command(handle, "true", sudo_user, sudo_password).await;
                if let Err(err) = verify {
                    // 回滚到切换前的状态
                    *file_access_mode = prev_mode;
                    *sudo_user = prev_sudo_user;
                    *sudo_password = prev_sudo_password;
                    let _ = respond_to.send(Err(err));
                    return Ok(false);
                }
            }

            *file_access_mode = mode.clone();
            let has_reusable = sudo_password.is_some();
            let su_user = sudo_user.clone();
            let mut sessions = state.sessions.write().await;
            if let Some(s) = sessions.get_mut(tab_id) {
                s.file_access_mode = mode;
                s.sudo_user = su_user;
                s.has_reusable_sudo_auth = has_reusable;
            }
            let _ = respond_to.send(Ok(()));
            Ok(false)
        }
        WorkerCmd::Disconnect => {
            // Signal the worker loop to exit.
            let _ = app.emit(
                "terminal:data",
                serde_json::json!({
                    "tabId": tab_id,
                    "chunk": "",
                }),
            );
            Ok(true)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SFTP helpers (russh-sftp 2.x)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn list_dir(sftp: &SftpSession, dir_path: &str) -> Result<Vec<Value>, String> {
    let entries = sftp
        .read_dir(dir_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    // SFTP servers commonly omit `..` from read_dir. Keep the file pane
    // navigation consistent with Electron by creating the parent row ourselves.
    if let Some(parent_path) = parent_remote_path(dir_path) {
        items.push(serde_json::json!({
            "name": "..",
            "path": parent_path,
            "type": "folder",
            "size": "-",
            "modified": "",
            "permission": "",
            "ownerGroup": "",
        }));
    }
    for entry in entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let full_path = entry.path();
        let stat = entry.metadata();
        let perm_bits = stat.permissions.unwrap_or(0);
        let is_dir = stat.is_dir();
        let is_link = stat.is_symlink();
        let file_type = if is_dir {
            "folder"
        } else if is_link {
            "symlink"
        } else {
            "file"
        };
        let size_str = if is_dir {
            "-".to_string()
        } else {
            format_bytes(stat.size.unwrap_or(0))
        };
        let modified = format_unix_ts(stat.mtime.unwrap_or(0) as i64);
        let permission = format_perm(perm_bits, is_dir, is_link);
        let uid = stat.uid.unwrap_or(0);
        let gid = stat.gid.unwrap_or(0);
        items.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": file_type,
            "size": size_str,
            "modified": modified,
            "permission": permission,
            "ownerGroup": format!("{}/{}", uid, gid),
        }));
    }
    items.sort_by(|a, b| {
        let af = a["type"].as_str() == Some("folder");
        let bf = b["type"].as_str() == Some("folder");
        bf.cmp(&af).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        })
    });
    Ok(items)
}

fn parent_remote_path(dir_path: &str) -> Option<String> {
    let normalized = dir_path.trim_end_matches('/');
    if normalized.is_empty() || normalized == "/" {
        return None;
    }

    match normalized.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(normalized[..index].to_string()),
        None => Some("/".to_string()),
    }
}

async fn read_file(sftp: &SftpSession, path: &str, encoding: &str) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut f = sftp
        .open(path)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
    decode_bytes(&buf, encoding)
}

async fn write_file(sftp: &SftpSession, path: &str, content: &str, encoding: &str) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let bytes = encode_text(content, encoding);
    let mut f = sftp
        .create(path)
        .await
        .map_err(|e| e.to_string())?;
    f.write_all(&bytes)
        .await
        .map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn create_dir(sftp: &SftpSession, path: &str) -> Result<(), String> {
    match sftp.metadata(path).await {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => return Err(format!("远端路径不是目录: {path}")),
        Err(_) => {}
    }
    sftp.create_dir(path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

const TRANSFER_CANCELED: &str = "transfer canceled";

async fn ensure_transfer_parent_dir(sftp: &SftpSession, path: &str) -> Result<(), String> {
    let parent = parent_remote_path(path).unwrap_or_else(|| "/".to_string());
    if parent == "/" {
        return Ok(());
    }
    let mut current = String::new();
    for segment in parent.split('/').filter(|segment| !segment.is_empty()) {
        current.push('/');
        current.push_str(segment);
        match sftp.metadata(&current).await {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(format!("传输目标父路径不是目录: {current}")),
            Err(_) => {
                sftp.create_dir(&current)
                    .await
                    .map_err(|error| format!("无法创建远端传输目录 {current}: {error}"))?;
            }
        }
    }
    Ok(())
}

async fn read_local_transfer_chunk(
    file: &mut tokio::fs::File,
    buffer: &mut [u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<usize, String> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
        result = file.read(buffer) => result.map_err(|error| error.to_string()),
    }
}

async fn read_remote_transfer_chunk(
    file: &mut russh_sftp::client::fs::File,
    buffer: &mut [u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<usize, String> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
        result = file.read(buffer) => result.map_err(|error| error.to_string()),
    }
}

async fn write_remote_transfer_chunk(
    file: &mut russh_sftp::client::fs::File,
    bytes: &[u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
        result = file.write_all(bytes) => result.map_err(|error| error.to_string()),
    }
}

async fn write_local_transfer_chunk(
    file: &mut tokio::fs::File,
    bytes: &[u8],
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<(), String> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
        result = file.write_all(bytes) => result.map_err(|error| error.to_string()),
    }
}

async fn upload_local_file(
    sftp: &SftpSession,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    let metadata = tokio::fs::metadata(local_path).await.map_err(|error| error.to_string())?;
    let total = metadata.len();
    if resume_offset > total {
        return Err("上传断点大于源文件".to_string());
    }
    ensure_transfer_parent_dir(sftp, remote_path).await?;
    let mut source = tokio::fs::File::open(local_path).await.map_err(|error| error.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    let flags = if resume_offset == 0 {
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
    } else {
        OpenFlags::WRITE | OpenFlags::CREATE
    };
    let mut destination = sftp
        .open_with_flags(remote_path, flags)
        .await
        .map_err(|error| error.to_string())?;
    destination
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    let mut transferred = resume_offset;
    let mut buffer = vec![0_u8; 64 * 1024];
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let read = read_local_transfer_chunk(&mut source, &mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        write_remote_transfer_chunk(&mut destination, &buffer[..read], &cancel).await?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    destination.flush().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn download_remote_file(
    sftp: &SftpSession,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    let metadata = sftp.metadata(remote_path).await.map_err(|error| error.to_string())?;
    let total = metadata.size.unwrap_or(0);
    if resume_offset > total {
        return Err("下载断点大于源文件".to_string());
    }
    let mut source = sftp.open(remote_path).await.map_err(|error| error.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    if resume_offset == 0 {
        options.truncate(true);
    }
    let mut destination = options.open(local_path).await.map_err(|error| error.to_string())?;
    destination
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    let mut transferred = resume_offset;
    let mut buffer = vec![0_u8; 64 * 1024];
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let read = read_remote_transfer_chunk(&mut source, &mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        write_local_transfer_chunk(&mut destination, &buffer[..read], &cancel).await?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    destination.flush().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn replace_remote_file(sftp: &SftpSession, partial_path: &str, destination_path: &str) -> Result<(), String> {
    let backup_path = format!("{destination_path}.fileterm-backup-{}", uuid::Uuid::new_v4());
    let moved_destination = match sftp.metadata(destination_path).await {
        Ok(_) => {
            sftp.rename(destination_path, &backup_path)
                .await
                .map_err(|error| format!("无法备份远端目标文件: {error}"))?;
            true
        }
        Err(_) => false,
    };
    if let Err(error) = sftp.rename(partial_path, destination_path).await {
        if moved_destination {
            let _ = sftp.rename(&backup_path, destination_path).await;
        }
        return Err(error.to_string());
    }
    if moved_destination {
        let _ = sftp.remove_file(&backup_path).await;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// sudo / root-mode helpers (exec channel + `sudo -S` / `sudo -n`)
// ─────────────────────────────────────────────────────────────────────────────

async fn stat_root_remote_file(
    handle: &Handle<ClientHandler>,
    path: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Option<TransferFileStat>, String> {
    let output = exec_shell_file_command(
        handle,
        &format!("stat -c '%s|%Y' -- {}", shell_quote(path)),
        sudo_user,
        sudo_password,
    )
    .await?;
    let Some((size, modified_at)) = output.trim().lines().next().and_then(|line| line.split_once('|')) else {
        return Ok(None);
    };
    let size = size.trim().parse::<u64>().map_err(|_| "无法解析 root 文件大小".to_string())?;
    let modified_at = modified_at.trim().parse::<u64>().ok().map(|value| value * 1000);
    Ok(Some(TransferFileStat { size, modified_at }))
}

async fn replace_root_remote_file(
    handle: &Handle<ClientHandler>,
    partial_path: &str,
    destination_path: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let parent = std::path::Path::new(destination_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let command = format!(
        "set -e\nmkdir -p {}\nif [ -L {} ]; then\n  cat -- {} > {}\n  rm -f -- {}\nelse\n  if [ -e {} ]; then\n    chown --reference={} -- {} 2>/dev/null || true\n    chmod --reference={} -- {} 2>/dev/null || true\n  fi\n  mv -f -- {} {}\nfi",
        shell_quote(&parent),
        shell_quote(destination_path),
        shell_quote(partial_path),
        shell_quote(destination_path),
        shell_quote(partial_path),
        shell_quote(destination_path),
        shell_quote(destination_path),
        shell_quote(partial_path),
        shell_quote(destination_path),
        shell_quote(partial_path),
        shell_quote(partial_path),
        shell_quote(destination_path),
    );
    exec_shell_file_command(handle, &command, sudo_user, sudo_password)
        .await
        .map(|_| ())
}

async fn download_root_remote_file(
    handle: &Handle<ClientHandler>,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let source = stat_root_remote_file(handle, remote_path, sudo_user, sudo_password)
        .await?
        .ok_or_else(|| "root 下载源文件不存在或无法读取".to_string())?;
    if resume_offset > source.size {
        return Err("root 下载断点大于源文件".to_string());
    }
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| error.to_string())?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    if resume_offset == 0 {
        options.truncate(true);
    }
    let mut local = options.open(local_path).await.map_err(|error| error.to_string())?;
    local
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;

    let shell_command = if resume_offset == 0 {
        format!("cat -- {}", shell_quote(remote_path))
    } else {
        format!("tail -c +{} -- {}", resume_offset + 1, shell_quote(remote_path))
    };
    let user = sudo_user.as_deref().unwrap_or("root");
    let command = if sudo_password.is_some() {
        format!("sudo -S -p '' -u {} sh -lc {}", shell_quote(user), shell_quote(&shell_command))
    } else {
        format!("sudo -n -u {} sh -lc {}", shell_quote(user), shell_quote(&shell_command))
    };
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(password) = sudo_password {
        channel
            .data(format!("{password}\n").as_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }

    let mut transferred = resume_offset;
    let mut stderr = String::new();
    crate::services::transfers::report_progress(app, transfer_id, transferred, source.size).await;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
            message = channel.wait() => message,
        };
        match next {
            Some(ChannelMsg::Data { data }) => {
                let bytes = data.as_ref();
                tokio::select! {
                    _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
                    result = local.write_all(bytes) => result.map_err(|error| error.to_string())?,
                }
                transferred += bytes.len() as u64;
                crate::services::transfers::report_progress(app, transfer_id, transferred, source.size).await;
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                if stderr.len() < 4096 {
                    stderr.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
            }
            Some(ChannelMsg::ExitStatus { .. }) | None => break,
            _ => {}
        }
    }
    local.flush().await.map_err(|error| error.to_string())?;
    if transferred != source.size {
        let suffix = if stderr.trim().is_empty() {
            String::new()
        } else {
            format!("：{}", stderr.trim())
        };
        return Err(format!("root 下载未完成（{transferred}/{} bytes）{suffix}", source.size));
    }
    Ok(())
}

/// POSIX shell quoting: wrap in single quotes, escape embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Run a shell command via the exec channel, with sudo when credentials are
/// present. Returns the combined stdout. Detects sudo auth failures and
/// returns an error so the caller can clear cached credentials.
async fn exec_shell_file_command(
    handle: &Handle<ClientHandler>,
    command: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let user = sudo_user.as_deref().unwrap_or("root");
    // Electron only uses `-n` when no password is available. `sudo -n -S`
    // rejects stdin authentication on several sudo versions, making a valid
    // password look like a timeout. With a password, `-S -p ''` consumes one
    // line from stdin; the outer timeout still bounds a retrying sudo prompt.
    let full_cmd = if sudo_password.is_some() {
        format!(
            "sudo -S -p '' -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(command)
        )
    } else {
        format!(
            "sudo -n -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(command)
        )
    };

    // 整个 exec 包超时：PTY 模式下 sudo 错误密码可能 retry 多次，channel
    // 不会自然退出。超时后返回错误，前端 loading 能在 10 秒内解除。
    let output = if let Some(pwd) = sudo_password {
        let stdin = format!("{}\n", pwd);
        match timeout(SUDO_VERIFY_TIMEOUT, super::system_metrics::exec_command_with_stdin(handle, &full_cmd, &stdin)).await {
            Ok(inner) => inner?,
            Err(_) => return Err("sudo 验证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string()),
        }
    } else {
        match timeout(SUDO_VERIFY_TIMEOUT, super::system_metrics::exec_command(handle, &full_cmd)).await {
            Ok(inner) => inner?,
            Err(_) => return Err("sudo 验证超时：服务器未在 10 秒内响应".to_string()),
        }
    };

    let lower = output.to_lowercase();
    if lower.contains("incorrect password")
        || lower.contains("authentication failure")
        || lower.contains("a password is required")
        || lower.contains("no password was provided")
        || lower.contains("sudo: permission denied")
        || lower.contains("sorry, try again")
    {
        return Err(
            "sudo 认证失败：密码错误或未授予 sudo 权限".to_string(),
        );
    }
    Ok(output)
}

/// List a directory via `find -printf` under sudo (GNU coreutils, BusyBox).
async fn exec_list_dir_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Vec<Value>, String> {
    let cmd = format!(
        "find {} -maxdepth 1 -mindepth 1 -printf '%y|%s|%T@|%u:%g|%m|%f\\n' 2>/dev/null",
        shell_quote(path)
    );
    let output = exec_shell_file_command(handle, &cmd, sudo_user, sudo_password).await?;
    let path_norm = path.trim_end_matches('/');

    let mut items = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\n');
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() < 6 {
            continue;
        }
        let type_char = parts[0].chars().next().unwrap_or('f');
        let is_dir = type_char == 'd';
        let is_link = type_char == 'l';
        let size_value = parts[1].parse::<u64>().unwrap_or(0);
        let size_str = if is_dir {
            "-".to_string()
        } else {
            format_bytes(size_value)
        };
        let mtime: i64 = parts[2].split('.').next().unwrap_or("0").parse().unwrap_or(0);
        let owner_group = parts[3].to_string();
        let perm_octal = u32::from_str_radix(parts[4], 8).unwrap_or(0o644);
        let name = parts[5].to_string();
        if name == "." || name == ".." {
            continue;
        }

        let file_type = if is_dir {
            "folder"
        } else if is_link {
            "symlink"
        } else {
            "file"
        };
        let permission = format_perm(perm_octal, is_dir, is_link);
        let full_path = if path_norm.is_empty() || path_norm == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", path_norm, name)
        };
        let modified = format_unix_ts(mtime);

        items.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": file_type,
            "size": size_str,
            "modified": modified,
            "permission": permission,
            "ownerGroup": owner_group,
        }));
    }
    items.sort_by(|a, b| {
        let af = a["type"].as_str() == Some("folder");
        let bf = b["type"].as_str() == Some("folder");
        bf.cmp(&af).then_with(|| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        })
    });
    Ok(items)
}

/// Read a file via `sudo cat` + base64 (binary-safe over the exec channel).
/// Decodes the result using the given encoding (mirrors Electron's
/// `readRemoteFileViaShell` + `decodeBuffer`).
async fn exec_read_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    encoding: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let cmd = format!("base64 {}", shell_quote(path));
    let output = exec_shell_file_command(handle, &cmd, sudo_user, sudo_password).await?;
    let trimmed: String = output.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&trimmed)
        .map_err(|e| format!("base64 decode failed: {}", e))?;
    decode_bytes(&bytes, encoding)
}

/// Write a file via `sudo tee` + base64 (binary-safe). Encodes the content
/// using the given encoding before base64-wrapping (mirrors Electron's
/// `writeRemoteFileViaShell` + `encodeText`).
async fn exec_write_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    content: &str,
    encoding: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let bytes = encode_text(content, encoding);
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let cmd = format!("base64 -d | tee {} > /dev/null", shell_quote(path));
    let user = sudo_user.as_deref().unwrap_or("root");
    let full_cmd = if sudo_password.is_some() {
        format!(
            "sudo -S -p '' -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(&cmd)
        )
    } else {
        format!(
            "sudo -n -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(&cmd)
        )
    };
    let stdin = if let Some(pwd) = sudo_password {
        format!("{}\n{}\n", pwd, encoded)
    } else {
        format!("{}\n", encoded)
    };
    let output = super::system_metrics::exec_command_with_stdin(handle, &full_cmd, &stdin).await?;
    let lower = output.to_lowercase();
    if lower.contains("incorrect password") || lower.contains("authentication failure") {
        return Err("sudo authentication failed".to_string());
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────────────────────────────────────

pub fn format_unix_ts(secs: i64) -> String {
    if secs == 0 {
        return String::from("1970-01-01T00:00:00Z");
    }
    let mut remaining = secs / 86400;
    let time_secs = secs % 86400;
    let (h, m, s) = (time_secs / 3600, (time_secs % 3600) / 60, time_secs % 60);
    let mut year = 1970i32;
    loop {
        let dy = if leap(year) { 366 } else { 365 };
        if remaining < dy {
            break;
        }
        remaining -= dy;
        year += 1;
    }
    let md: [i64; 12] = if leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &days in &md {
        if remaining < days {
            break;
        }
        remaining -= days;
        month += 1;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        remaining + 1,
        h,
        m,
        s
    )
}

fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn format_bytes(size: u64) -> String {
    if size == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut unit_index = 0;
    while value >= 1000.0 && unit_index < units.len() - 1 {
        value /= 1000.0;
        unit_index += 1;
    }
    let digits = if value >= 10.0 || unit_index == 0 { 0 } else { 1 };
    format!("{:.*} {}", digits, value, units[unit_index])
}

fn format_perm(perm: u32, is_dir: bool, is_link: bool) -> String {
    let tc = if is_link {
        'l'
    } else if is_dir {
        'd'
    } else {
        '-'
    };
    let bits = perm & 0o777;
    let mut s = String::with_capacity(10);
    s.push(tc);
    for shift in [6u32, 3, 0] {
        let oct = (bits >> shift) & 7;
        s.push(if oct & 4 != 0 { 'r' } else { '-' });
        s.push(if oct & 2 != 0 { 'w' } else { '-' });
        s.push(if oct & 1 != 0 { 'x' } else { '-' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{
        build_http_connect_request, format_sftp_unavailable_reason, is_password_prompt,
        looks_like_mfa_prompt, parent_remote_path, forward_local_connection, forward_socks5_connection,
        capture_sudo_password_input, looks_like_root_prompt, remote_bind_host_matches,
        suppress_shell_setup_echo, track_sudo_prompt_from_terminal, ShellSetupEchoSuppression,
        SHELL_SETUP_SETTLE_DELAY,
        try_keyboard_interactive_with_responder, tunnel_bind_address, validate_tunnel_rule,
        KeyboardInteractiveRequest, SshTunnelRule,
    };
    use std::borrow::Cow;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use russh::keys::PrivateKey;
    use russh::{client, server};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::time::{timeout, Duration};

    #[cfg(unix)]
    struct OpenSshFixture {
        root: std::path::PathBuf,
        remote_dir: std::path::PathBuf,
        client_key: std::path::PathBuf,
        port: u16,
        process: std::process::Child,
    }

    #[cfg(unix)]
    impl Drop for OpenSshFixture {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    fn current_test_username() -> String {
        std::env::var("USER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                String::from_utf8(
                    std::process::Command::new("id")
                        .arg("-un")
                        .output()
                        .expect("could not determine the current test user")
                        .stdout,
                )
                .expect("current test user was not UTF-8")
                .trim()
                .to_string()
            })
    }

    #[cfg(unix)]
    fn start_openssh_fixture() -> OpenSshFixture {
        const SSHD: &str = "/usr/sbin/sshd";
        const SSH_KEYGEN: &str = "/usr/bin/ssh-keygen";
        assert!(
            std::path::Path::new(SSHD).exists() && std::path::Path::new(SSH_KEYGEN).exists(),
            "real OpenSSH verification requires {SSHD} and {SSH_KEYGEN}"
        );

        let root = std::env::temp_dir().join(format!("fileterm-tauri-sshd-{}", uuid::Uuid::new_v4()));
        let remote_dir = root.join("remote");
        std::fs::create_dir_all(&remote_dir).unwrap();
        let host_key = root.join("host-key");
        let client_key = root.join("client-key");
        let authorized_keys = root.join("authorized_keys");
        for key in [&host_key, &client_key] {
            let result = std::process::Command::new(SSH_KEYGEN)
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(key)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        std::fs::copy(client_key.with_extension("pub"), &authorized_keys).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&authorized_keys, std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }

        let port_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = port_listener.local_addr().unwrap().port();
        drop(port_listener);
        let config = root.join("sshd_config");
        std::fs::write(
            &config,
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nPubkeyAuthentication yes\nAllowTcpForwarding yes\nUsePAM no\nUseDNS no\nLogLevel ERROR\nSubsystem sftp internal-sftp\n",
                host_key.display(),
                root.join("sshd.pid").display(),
                authorized_keys.display(),
            ),
        )
        .unwrap();
        let process = std::process::Command::new(SSHD)
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        OpenSshFixture {
            root,
            remote_dir,
            client_key,
            port,
            process,
        }
    }

    #[cfg(unix)]
    async fn wait_for_openssh(port: u16) {
        for _ in 0..40 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("local OpenSSH fixture did not begin listening");
    }

    #[cfg(unix)]
    async fn read_http_headers(socket: &mut tokio::net::TcpStream) -> String {
        let mut headers = Vec::new();
        let mut byte = [0_u8; 1];
        while !headers.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert_eq!(count, 1, "proxy client closed before completing CONNECT headers");
            headers.push(byte[0]);
        }
        String::from_utf8(headers).unwrap()
    }

    #[cfg(unix)]
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

    #[cfg(unix)]
    async fn authenticate_openssh_fixture(
        fixture: &OpenSshFixture,
        profile: &serde_json::Value,
    ) -> client::Handle<AcceptTestServerKey> {
        let stream = super::connect_ssh_transport(profile, "127.0.0.1", fixture.port)
            .await
            .unwrap();
        let mut handle = client::connect_stream(
            Arc::new(client::Config::default()),
            stream,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let key = russh::keys::decode_secret_key(
            &std::fs::read_to_string(&fixture.client_key).unwrap(),
            None,
        )
        .unwrap();
        let authenticated = handle
            .authenticate_publickey(
                current_test_username(),
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None),
            )
            .await
            .unwrap();
        assert!(authenticated.success());
        handle
    }

    struct AcceptTestServerKey;

    impl client::Handler for AcceptTestServerKey {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &russh::keys::PublicKey,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    struct KeyboardInteractiveMfaServer {
        responses: Arc<Mutex<Vec<String>>>,
    }

    impl server::Handler for KeyboardInteractiveMfaServer {
        type Error = russh::Error;

        async fn auth_keyboard_interactive<'a>(
            &'a mut self,
            _user: &str,
            _submethods: &str,
            response: Option<server::Response<'a>>,
        ) -> Result<server::Auth, Self::Error> {
            if let Some(response) = response {
                let received = response
                    .map(|answer| String::from_utf8_lossy(&answer).into_owned())
                    .collect::<Vec<_>>();
                *self.responses.lock().unwrap() = received.clone();
                return Ok(if received == ["saved-password", "246810"] {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                });
            }
            Ok(server::Auth::Partial {
                name: Cow::Borrowed("FileTerm MFA fixture"),
                instructions: Cow::Borrowed("Enter password and second factor"),
                prompts: Cow::Owned(vec![
                    (Cow::Borrowed("Password: "), false),
                    (Cow::Borrowed("OTP code: "), false),
                ]),
            })
        }
    }

    #[test]
    fn suppresses_fragmented_cwd_setup_echo_after_its_marker_settles() {
        let mut pending = Some(ShellSetupEchoSuppression::new(true));

        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                "Debian GNU/Linux\r\nuser@host:~$ test -z \"${FISH_VERSION-}\" && eval '__tdcwd(){ printf"
            ),
            ""
        );

        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " '\\033]7;file:///home/user\\007'; }; __tdcwd\r\n\u{1b}]7;file:///home/user\u{7}user@host:~$ ",
            ),
            ""
        );

        pending.as_mut().unwrap().marker_seen_at = Some(Instant::now() - SHELL_SETUP_SETTLE_DELAY);
        let visible = suppress_shell_setup_echo(
            &mut pending,
            "root@host:~# ",
        );

        assert_eq!(visible, "Debian GNU/Linux\r\nuser@host:~$ root@host:~# ");
        assert!(pending.is_none());
    }

    #[test]
    fn detects_root_prompt_after_terminal_colours_are_removed() {
        assert!(looks_like_root_prompt("\u{1b}[01;31mroot@host\u{1b}[0m:# "));
        assert!(!looks_like_root_prompt("user@host:$ "));
    }

    #[test]
    fn terminal_sudo_password_cache_is_cleared_after_auth_failure() {
        let mut prompt_buffer = String::new();
        let mut awaiting = false;
        let mut pending = String::new();
        let mut recent = String::new();
        let mut cached = None;

        assert!(!track_sudo_prompt_from_terminal(
            "[sudo] user 的密码：",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending,
            &mut cached,
        ));
        assert!(awaiting);
        assert!(capture_sudo_password_input(
            "wrong\r",
            &mut awaiting,
            &mut pending,
            &mut recent,
            &mut cached,
        ));
        assert_eq!(cached.as_deref(), Some("wrong"));
        assert!(track_sudo_prompt_from_terminal(
            "Sorry, try again.\r\n",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending,
            &mut cached,
        ));
        assert!(cached.is_none());
        assert!(!awaiting);
    }

    #[test]
    fn creates_parent_rows_only_below_remote_root() {
        assert_eq!(parent_remote_path("/"), None);
        assert_eq!(parent_remote_path("/home"), Some("/".to_string()));
        assert_eq!(parent_remote_path("/home/stoffel/下载/"), Some("/home/stoffel".to_string()));
    }

    #[test]
    fn builds_authenticated_http_connect_request_with_ipv6_authority() {
        let request = String::from_utf8(
            build_http_connect_request("2001:db8::1", 22, "alice", "secret").unwrap(),
        )
        .unwrap();

        assert!(request.starts_with("CONNECT [2001:db8::1]:22 HTTP/1.1\r\n"));
        assert!(request.contains("Host: [2001:db8::1]:22\r\n"));
        assert!(request.contains("Proxy-Authorization: Basic YWxpY2U6c2VjcmV0\r\n"));
    }

    #[test]
    fn rejects_http_connect_header_injection() {
        assert!(build_http_connect_request("host\r\nInjected: x", 22, "", "").is_err());
    }

    #[test]
    fn reports_sftp_timeout_without_mislabeling_the_ssh_shell() {
        let message = format_sftp_unavailable_reason("SFTP init failed: Timeout");

        assert!(message.contains("SFTP 子系统"));
        assert!(message.contains("SSH 终端已连接"));
        assert!(message.contains("sftp subsystem"));
    }

    #[test]
    fn only_reuses_saved_password_for_password_prompts() {
        assert!(is_password_prompt("Password: "));
        assert!(looks_like_mfa_prompt("Verification code: "));
        assert!(!is_password_prompt("Verification code: "));
        assert!(!is_password_prompt("OTP token: "));
    }

    #[tokio::test]
    async fn real_ssh_mfa_server_keeps_saved_password_out_of_otp_answer() {
        let responses = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut server_config = server::Config::default();
        server_config.inactivity_timeout = None;
        server_config.auth_rejection_time = Duration::from_millis(1);
        server_config.keys.push(
            PrivateKey::random(
                &mut rand::rng(),
                russh::keys::ssh_key::Algorithm::Ed25519,
            )
            .unwrap(),
        );
        let server_responses = responses.clone();
        let server_task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(server_config),
                socket,
                KeyboardInteractiveMfaServer {
                    responses: server_responses,
                },
            )
            .await
            .unwrap();
            // Dropping the test client ends the SSH stream with EOF; that is
            // the expected lifecycle outcome after successful authentication.
            let _ = running.await;
        });

        let mut handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let requests = Arc::new(Mutex::new(Vec::<KeyboardInteractiveRequest>::new()));
        let requested_prompts = requests.clone();
        let authenticated = try_keyboard_interactive_with_responder(
            &mut handle,
            "alice",
            "saved-password",
            move |request| {
                let requested_prompts = requested_prompts.clone();
                async move {
                    requested_prompts.lock().unwrap().push(request);
                    Some(vec!["246810".to_string()])
                }
            },
        )
        .await
        .unwrap();

        assert!(authenticated);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].prompts.len(), 1);
        assert_eq!(requests[0].prompts[0].prompt, "OTP code: ");
        drop(requests);
        assert_eq!(responses.lock().unwrap().as_slice(), ["saved-password", "246810"]);

        drop(handle);
        timeout(Duration::from_secs(2), server_task)
            .await
            .expect("MFA fixture did not release its SSH socket")
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_accepts_tauri_auth_exec_sftp_and_platform_probe() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;

        let profile = serde_json::json!({ "proxy": { "type": "none" } });
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;

        let command = crate::sessions::system_metrics::exec_command(
            &handle,
            "printf 'tauri-openssh-exec'",
        )
        .await
        .unwrap();
        assert_eq!(command, "tauri-openssh-exec");

        let platform = crate::sessions::system_metrics::probe_remote_platform(&handle).await;
        #[cfg(target_os = "linux")]
        assert_eq!(platform, "linux");
        #[cfg(target_os = "macos")]
        assert_eq!(platform, "unknown", "macOS metrics collection is not implemented yet");

        let channel = handle.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .unwrap();
        let remote_file = fixture.remote_dir.join("tauri-sftp.txt");
        let remote_file = remote_file.to_string_lossy().into_owned();
        sftp.create(&remote_file).await.unwrap();
        sftp.write(&remote_file, b"tauri-openssh-sftp").await.unwrap();
        assert_eq!(sftp.read(&remote_file).await.unwrap(), b"tauri-openssh-sftp");
        sftp.close().await.unwrap();

        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });
        let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_address = local_listener.local_addr().unwrap();
        let mut local_client = tokio::net::TcpStream::connect(local_address).await.unwrap();
        let (local_socket, _) = local_listener.accept().await.unwrap();
        let tunnel_handle = Arc::new(handle);
        let tunnel_rule = SshTunnelRule {
            id: "real-openssh-local".to_string(),
            name: "real-openssh-local".to_string(),
            kind: "local".to_string(),
            bind_host: "127.0.0.1".to_string(),
            bind_port: local_address.port(),
            target_host: Some("127.0.0.1".to_string()),
            target_port: Some(target_address.port()),
            auto_start: false,
        };
        let bridge = tokio::spawn({
            let tunnel_handle = tunnel_handle.clone();
            async move { forward_local_connection(local_socket, tunnel_handle, &tunnel_rule).await }
        });
        local_client.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        local_client.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(local_client);
        timeout(Duration::from_secs(2), target).await.unwrap().unwrap();
        timeout(Duration::from_secs(2), bridge)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        let dynamic_target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dynamic_target_address = dynamic_target_listener.local_addr().unwrap();
        let dynamic_target = tokio::spawn(async move {
            let (mut socket, _) = dynamic_target_listener.accept().await.unwrap();
            let mut request = [0_u8; 5];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"socks");
            socket.write_all(b"proxy").await.unwrap();
        });
        let dynamic_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dynamic_address = dynamic_listener.local_addr().unwrap();
        let mut dynamic_client = tokio::net::TcpStream::connect(dynamic_address).await.unwrap();
        let (dynamic_socket, _) = dynamic_listener.accept().await.unwrap();
        let dynamic_bridge = tokio::spawn({
            let tunnel_handle = tunnel_handle.clone();
            async move { forward_socks5_connection(dynamic_socket, tunnel_handle).await }
        });
        dynamic_client.write_all(&[5, 1, 0]).await.unwrap();
        let mut selected = [0_u8; 2];
        dynamic_client.read_exact(&mut selected).await.unwrap();
        assert_eq!(&selected, &[5, 0]);
        dynamic_client
            .write_all(&[
                5,
                1,
                0,
                1,
                127,
                0,
                0,
                1,
                (dynamic_target_address.port() >> 8) as u8,
                dynamic_target_address.port() as u8,
            ])
            .await
            .unwrap();
        let mut connected = [0_u8; 10];
        dynamic_client.read_exact(&mut connected).await.unwrap();
        assert_eq!(&connected[..2], &[5, 0]);
        dynamic_client.write_all(b"socks").await.unwrap();
        let mut dynamic_response = [0_u8; 5];
        dynamic_client.read_exact(&mut dynamic_response).await.unwrap();
        assert_eq!(&dynamic_response, b"proxy");
        drop(dynamic_client);
        timeout(Duration::from_secs(2), dynamic_target)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(2), dynamic_bridge)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_authenticates_through_tauri_http_proxy_transport() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let target_port = fixture.port;
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let request = read_http_headers(&mut client).await;
            assert!(request.starts_with(&format!(
                "CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\n"
            )));
            assert!(request.contains("Proxy-Authorization: Basic cHJveHktdXNlcjpwcm94eS1wYXNz\r\n"));
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(("127.0.0.1", target_port))
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
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;
        let output = crate::sessions::system_metrics::exec_command(
            &handle,
            "printf 'tauri-openssh-http-proxy'",
        )
        .await
        .unwrap();
        assert_eq!(output, "tauri-openssh-http-proxy");
        drop(handle);
        timeout(Duration::from_secs(2), proxy)
            .await
            .expect("HTTP proxy transport did not release")
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_authenticates_through_tauri_socks5_proxy_transport() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let target_port = fixture.port;
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let (host, port) = read_socks5_connect_request(&mut client).await;
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, target_port);
            client
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(("127.0.0.1", target_port))
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
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;
        let output = crate::sessions::system_metrics::exec_command(
            &handle,
            "printf 'tauri-openssh-socks5-proxy'",
        )
        .await
        .unwrap();
        assert_eq!(output, "tauri-openssh-socks5-proxy");
        drop(handle);
        timeout(Duration::from_secs(2), proxy)
            .await
            .expect("SOCKS5 proxy transport did not release")
            .unwrap();
    }

    #[test]
    fn validates_tunnel_rules_and_normalizes_cross_platform_bind_addresses() {
        let valid = SshTunnelRule {
            id: "local-db".to_string(),
            name: "database".to_string(),
            kind: "local".to_string(),
            bind_host: "127.0.0.1".to_string(),
            bind_port: 15432,
            target_host: Some("db.internal".to_string()),
            target_port: Some(5432),
            auto_start: false,
        };
        assert!(validate_tunnel_rule(&valid).is_ok());
        assert_eq!(tunnel_bind_address("*", 1080).unwrap(), "0.0.0.0:1080");
        assert_eq!(tunnel_bind_address("::1", 1080).unwrap(), "[::1]:1080");

        let invalid = SshTunnelRule { target_port: None, ..valid };
        assert!(validate_tunnel_rule(&invalid).is_err());
    }

    #[test]
    fn remote_forward_matches_exact_and_wildcard_bind_hosts() {
        assert!(remote_bind_host_matches("127.0.0.1", "127.0.0.1"));
        assert!(!remote_bind_host_matches("127.0.0.1", "10.0.0.4"));
        assert!(remote_bind_host_matches("0.0.0.0", "10.0.0.4"));
        assert!(remote_bind_host_matches("::", "2001:db8::4"));
    }
}
