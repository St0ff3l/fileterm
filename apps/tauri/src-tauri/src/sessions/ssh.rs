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
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use base64::Engine;
use russh::client::{Handle, Handler};
use russh::keys::PrivateKeyWithHashAlg;
use russh::{Channel, ChannelMsg, ChannelWriteHalf, Disconnect, Sig};
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::Metadata as SftpMetadata;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{OpenFlags, StatusCode};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{
    copy_bidirectional, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::time::timeout;
use tokio_socks::tcp::Socks5Stream;
use tokio_util::sync::CancellationToken;

use super::{TransferFileStat, WorkerCmd};
use crate::services::WorkspaceTabStatus;

const DEFAULT_SSH_KEY_FILES: [&str; 4] = ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];
const SSH_INTERACTION_TIMEOUT: Duration = Duration::from_secs(300);
// A TCP connection, SSH protocol handshake, or password-auth reply can remain
// pending indefinitely on a broken server or middlebox. Keep each startup
// stage bounded so the workspace moves out of `connecting` and the user can
// retry instead of seeing a permanently reconnecting terminal.
const SSH_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_PASSWORD_AUTH_TIMEOUT: Duration = Duration::from_secs(30);
/// HTTP/SOCKS5 代理单步 IO 超时。代理服务器或中间网络卡住时，TCP 连接、
/// CONNECT 请求写入、响应逐字节读取都不能让外层 30s 超时全部消耗在
/// 单次 read 上——慢速代理可以每 29s 发一个字节拖满整个阶段。8s 覆盖
/// 正常代理 RTT，超时后立即给出明确错误。
const PROXY_IO_TIMEOUT: Duration = Duration::from_secs(8);
/// A remote PTY write must not pin the SSH worker forever when the server has
/// stopped consuming the channel or the channel window is exhausted.
const TERMINAL_WRITE_TIMEOUT: Duration = Duration::from_secs(2);
/// SIGINT is an out-of-band emergency path. Keep its wait short so a broken
/// SSH transport cannot make Ctrl+C look like a frozen desktop app.
const TERMINAL_INTERRUPT_TIMEOUT: Duration = Duration::from_millis(500);
/// PTY window-change (resize) requests share the SSH channel request path with
/// shell data. A stuck transport must not be allowed to pin the worker loop
/// when the renderer simply wants to inform the server of new cols/rows; treat
/// resize as best-effort and let the next cmd cycle proceed.
const TERMINAL_RESIZE_TIMEOUT: Duration = Duration::from_millis(500);
/// Hard ceiling for the per-tab terminal output batch buffer. Under sustained
/// high-throughput output (e.g. `pacman-key --populate`) the 16ms flush timer
/// can lose fairness to the shell reader branch; this guard forces a flush so
/// memory does not balloon and `emit_terminal_data` does not grow a multi-MB
/// chunk in one shot.
const TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD: usize = 64 * 1024;

type SshShellWriteHalf = ChannelWriteHalf<russh::client::Msg>;

async fn write_shell_data(
    writer: &SshShellWriteHalf,
    data: impl Into<Vec<u8>>,
) -> Result<(), String> {
    timeout(TERMINAL_WRITE_TIMEOUT, writer.data_bytes(data.into()))
        .await
        .map_err(|_| "SSH terminal write timed out".to_string())?
        .map_err(|error| error.to_string())
}

fn contains_interrupt_byte(data: &str) -> bool {
    data.as_bytes().contains(&0x03)
}

/// Trim a rolling string buffer to its last `keep` bytes without splitting a
/// multi-byte UTF-8 character. Plain byte-index slicing (`s[len - keep..]`)
/// panics when the cut lands inside a CJK character or a U+FFFD replacement
/// char emitted by `from_utf8_lossy`. Inside a spawned tokio task such a
/// panic silently kills the SSH worker / output pump: the JoinHandle is
/// dropped, no state update reaches the renderer, and the terminal looks
/// frozen with Ctrl+C dead — the exact "跑脚本卡住" report. Every rolling
/// buffer on the terminal hot path must go through this helper.
fn trim_string_front(value: &mut String, keep: usize) {
    if value.len() <= keep {
        return;
    }
    let mut start = value.len() - keep;
    while start < value.len() && !value.is_char_boundary(start) {
        start += 1;
    }
    value.drain(..start);
}

async fn wait_for_ssh_stage<T>(
    stage: &str,
    deadline: Duration,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let timeout_label = if deadline.as_secs() > 0 {
        format!("{} seconds", deadline.as_secs())
    } else {
        format!("{} ms", deadline.as_millis())
    };
    timeout(deadline, operation)
        .await
        .map_err(|_| format!("{stage} timed out after {timeout_label}"))?
}

fn resource_monitoring_enabled(profile: &Value) -> bool {
    profile
        .get("enableResourceMonitoring")
        .and_then(Value::as_bool)
        != Some(false)
}

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
    mut terminal_input_rx: mpsc::UnboundedReceiver<String>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    tokio::spawn(async move {
        let tid = tab_id.clone();
        crate::services::logging::session(&app, "INFO", "ssh", &tid, "worker started");
        // The initial "连接主机...\r\n" notice is already in the session
        // snapshot's `terminal_transcript` (set by `app_open_profile`), so
        // the renderer hydrates it via `bootText` — no need to emit it here.
        // Emitting here would race the renderer's listener registration.
        //
        // 监督层：run_worker_loop 过去直接 await 在这个 spawn 里，循环内任何
        // panic（例如热路径上的 String 字节切片切到多字节字符内部）都会无声
        // 杀死任务——JoinHandle 无人 await，没有日志、没有状态更新，renderer
        // 永远显示"已连接"，终端冻结且 Ctrl+C 无效。现在把循环放进独立任务
        // 并 await 其 JoinHandle：panic 转成 JoinError 后走下面统一的失败
        // 收尾路径（错误日志 + transcript 提示 + 状态广播 + 自动重连判断）。
        // 关闭链路只用 CancellationToken（无 abort），所以 JoinError 一定
        // 是 panic，不是正常取消。
        //
        // panic 位置由 logging::install_panic_hook 在 panic 发生时即写入文件
        // 日志（scope=panic），这里只负责把 JoinError 分类后落到 per-tab 日志
        // 和 transcript，便于和 panic hook 那行交叉定位。
        let worker_app = app.clone();
        let worker_cancellation = cancellation.clone();
        let worker_profile = profile.clone();
        let run_result = tokio::spawn(async move {
            run_worker_loop(
                &tab_id,
                &worker_profile,
                &mut cmd_rx,
                &mut terminal_input_rx,
                &worker_app,
                worker_cancellation,
            )
            .await
        })
        .await
        .unwrap_or_else(|join_error| {
            // JoinError.Display 不带源码位置，所以这里只输出分类 + 系统消息；
            // 真正的 panic 位置在 panic hook 写的那行里。
            let kind = if join_error.is_cancelled() {
                "cancelled"
            } else if join_error.is_panic() {
                "panic"
            } else {
                "aborted"
            };
            Err(format!("worker task {kind}: {join_error}"))
        });
        if cancellation.is_cancelled() {
            crate::services::logging::session(&app, "INFO", "ssh", &tid, "worker cancelled");
            return;
        }
        let final_status = match run_result {
            Ok(()) => {
                crate::services::logging::session(
                    &app,
                    "INFO",
                    "ssh",
                    &tid,
                    "worker exited cleanly",
                );
                emit_terminal_data(&app, &tid, "连接已断开\r\n").await;
                WorkspaceTabStatus::Closed
            }
            Err(e) => {
                crate::services::logging::session(
                    &app,
                    "ERROR",
                    "ssh",
                    &tid,
                    format!("worker failed: {e}"),
                );
                emit_terminal_data(&app, &tid, &format!("连接失败: {}\r\n", e)).await;
                WorkspaceTabStatus::Error
            }
        };
        update_tab_status_and_emit(&app, &tid, final_status).await;

        // ── Auto-reconnect with 2000ms delay ───────────────────────────────
        // Mirrors Electron's `workspace-service.ts` autoReconnectingTabs:
        // if the profile's `reconnectMode === 'auto'`, schedule a reconnect
        // after 2 seconds. The guard set prevents re-entrant triggers while
        // a reconnect is already pending.
        // Read the live session policy instead of the worker's startup copy.
        // The connection editor can change reconnectMode while this worker is
        // still alive, and the next disconnect must use that new policy.
        let reconnect_mode = {
            let state = app.state::<crate::services::workspace::WorkspaceState>();
            let sessions = state.sessions.read().await;
            let mode = sessions
                .get(&tid)
                .and_then(|session| session.reconnect_mode.clone())
                .or_else(|| crate::services::workspace::reconnect_mode_for_profile(&profile));
            mode.unwrap_or_else(|| "none".to_string())
        };
        if reconnect_mode == "auto" {
            crate::services::logging::session(
                &app,
                "INFO",
                "ssh",
                &tid,
                "auto-reconnect scheduled delay_ms=2000",
            );
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Re-check: tab may have been closed or already reconnected by
            // the user during the delay.
            let state = app.state::<crate::services::workspace::WorkspaceState>();
            let should_reconnect = {
                let tabs = state.tabs.read().await;
                let sessions = state.sessions.read().await;
                let tab_exists = tabs.iter().any(|t| t.id == tid);
                let session_connected = sessions.get(&tid).map(|s| s.connected).unwrap_or(false);
                tab_exists && !session_connected
            };

            if should_reconnect {
                crate::services::logging::session(
                    &app,
                    "INFO",
                    "ssh",
                    &tid,
                    "auto-reconnect firing",
                );
                // Trigger reconnect via the same path the renderer uses.
                let _ = crate::commands::app_reconnect_tab(app.clone(), tid.clone()).await;
            } else {
                crate::services::logging::session(
                    &app,
                    "DEBUG",
                    "ssh",
                    &tid,
                    "auto-reconnect canceled",
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
        crate::services::logging::session(
            &self.app,
            "DEBUG",
            "ssh",
            &self.tab_id,
            format!(
                "host-key verification host={} port={}",
                self.host, self.port
            ),
        );
        // Short-circuit: if the profile already trusts this exact
        // fingerprint, accept without prompting. This is the common path
        // after the user previously chose "accept-and-save".
        if let Some(known) = &self.trusted_fingerprint {
            if known == &fp {
                crate::services::logging::session(
                    &self.app,
                    "INFO",
                    "ssh",
                    &self.tab_id,
                    "host-key matched saved fingerprint",
                );
                return Ok(true);
            }
            crate::services::logging::session(
                &self.app,
                "WARN",
                "ssh",
                &self.tab_id,
                "host-key mismatch; requesting user verification",
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
                let library_mutation = self
                    .app
                    .state::<crate::services::workspace::WorkspaceState>()
                    .library_mutation
                    .clone();
                let _guard = library_mutation.lock().await;
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
        let state = self
            .app
            .state::<crate::services::workspace::WorkspaceState>();
        let target = {
            let forwards = state.remote_forwards.read().await;
            forwards
                .get(&self.tab_id)
                .and_then(|rules| {
                    rules.iter().find(|rule| {
                        rule.bind_port == connected_port
                            && remote_bind_host_matches(&rule.bind_host, connected_address)
                    })
                })
                .cloned()
        };

        let Some(target) = target else {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        };

        reply.accept().await;
        let tab_id = self.tab_id.clone();
        let app = self.app.clone();
        tokio::spawn(async move {
            let result = async {
                // 加 timeout：远端转发的目标 host 卡住时 TcpStream::connect
                // 会永久 await，spawn task 不退出，远端发起方也一直等。
                // 10 秒覆盖正常 RTT，超时后清理 task 让远端拿到连接重置。
                let mut local = timeout(
                    Duration::from_secs(10),
                    TcpStream::connect((&*target.target_host, target.target_port)),
                )
                .await
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "remote forward target connect timed out",
                    )
                })??;
                let mut remote = channel.into_stream();
                copy_bidirectional(&mut local, &mut remote).await?;
                Ok::<(), std::io::Error>(())
            }
            .await;
            if let Err(error) = result {
                crate::services::logging::session(
                    &app,
                    "WARN",
                    "tunnel",
                    &tab_id,
                    format!("remote forward connection failed: {error}"),
                );
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
        let actual_port = timeout(
            SSH_TUNNEL_OP_TIMEOUT,
            self.handle
                .tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32),
        )
        .await
        .map_err(|_| {
            "Remote tunnel request timed out: 服务器未在 5 秒内响应 tcpip_forward".to_string()
        })?
        .map_err(|error| format!("Remote tunnel request failed: {error}"))?;
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

async fn update_tab_status_and_emit(app: &AppHandle, tab_id: &str, status: WorkspaceTabStatus) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let connected = status.is_connected();
    let mut summary = "连接已断开".to_string();
    let mut transcript = String::new();
    {
        let mut tabs = state.tabs.write().await;
        if let Some(tab) = tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = status;
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
/// missed the live terminal stream, e.g. during a fast-fail connect).
async fn emit_terminal_data(app: &AppHandle, tab_id: &str, chunk: &str) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    state.publish_terminal_output(tab_id, chunk);
    let mut sessions = state.sessions.write().await;
    if let Some(s) = sessions.get_mut(tab_id) {
        s.terminal_transcript.push_str(chunk);
        // Cap transcript to 200k chars (matches Electron's BoundedTextBuffer).
        // 必须走 char 边界安全裁剪：transcript 含中文与 U+FFFD，直接字节
        // 切片会 panic 并杀死 output pump，终端输出永久冻结。
        trim_string_front(&mut s.terminal_transcript, 180_000);
    }
}

/// Mirrors Electron's `followShellCwd`: only a confirmed shell CWD update may
/// move the file panel, and only while the user has Follow terminal enabled.
#[allow(clippy::too_many_arguments)] // Protocol/session context is intentionally explicit at this async boundary.
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
    let files = match timeout(FILE_OPERATION_TIMEOUT, async {
        if file_access_mode == "root" {
            exec_list_dir_via_shell(&handle, &cwd, &sudo_user, &sudo_password).await
        } else {
            // russh-sftp's client is one request stream. Serialise access to it:
            // concurrent read locks let multiple list/delete/upload requests
            // interleave and eventually time out after app focus is restored.
            // The timeout covers both waiting for the lock and SFTP read_dir.
            let sftp = sftp.write().await;
            list_dir(&sftp, &cwd).await
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err(format!("跟随终端目录 {cwd} 超时")),
    };

    let follow_error = files.as_ref().err().cloned();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    let mut sessions = state.sessions.write().await;
    let Some(session) = sessions.get_mut(&tab_id) else {
        return;
    };
    session.remote_files_loading = false;
    if session.shell_cwd.as_deref() == Some(cwd.as_str()) && session.follow_shell_cwd {
        if let Ok(files) = files {
            session.remote_path = cwd.clone();
            session.remote_files = files;
        }
    }
    drop(sessions);

    if let Some(error) = follow_error {
        crate::services::logging::ssh_debug(
            &app,
            &tab_id,
            format!("CWD follow failed for {cwd}: {error}"),
        );
    } else {
        crate::services::logging::ssh_debug(&app, &tab_id, format!("CWD follow applied: {cwd}"));
    }

    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

/// Flush the batch buffer to the terminal output pump channel.
///
/// 非阻塞：用 `try_send` 把 chunk 推到 bounded channel，由独立的 pump
/// task 异步消费并推送到 renderer。通道满时丢弃 chunk 并限频记录——终端
/// 输出是尽力而为的，丢几帧不影响功能，但 worker 主循环的 select! 必须
/// 立即返回以保证 Ctrl+C 路径畅通。
fn flush_batch(
    batch: &mut Vec<u8>,
    output_tx: &tokio::sync::mpsc::Sender<String>,
    app: &AppHandle,
    tab_id: &str,
) {
    if batch.is_empty() {
        return;
    }
    let chunk = String::from_utf8_lossy(batch).into_owned();
    batch.clear();
    if let Err(tokio::sync::mpsc::error::TrySendError::Full(_)) = output_tx.try_send(chunk) {
        // 通道满说明 pump task 跟不上（renderer IPC 或 RwLock 竞争）。
        // 丢弃 chunk 避免阻塞主循环。限频日志避免在极端高吞吐下刷屏。
        crate::services::logging::session(
            app,
            "WARN",
            "ssh",
            tab_id,
            "terminal output pump saturated, dropping chunk",
        );
    }
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
    static CWD_OSC_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\x1b\]7;file://([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap()
    });
    static USER_OSC_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\x1b\]1337;RemoteUser=([^\x07\x1b]*)(?:\x07|\x1b\\)").unwrap()
    });

    buffer.push_str(chunk);
    if buffer.len() > 8192 {
        // 滚动窗口裁剪必须 char 边界安全：buffer 里有原始终端输出
        // （含中文），切片切到多字节字符内部会 panic 杀死 worker。
        trim_string_front(buffer, 4096);
    }

    let mut cwd = None;
    let mut user = None;

    for cap in CWD_OSC_PATTERN.captures_iter(buffer) {
        let raw_path = &cap[1];
        if let Some(slash_idx) = raw_path.find('/') {
            let path_part = &raw_path[slash_idx..];
            cwd = Some(percent_decode(path_part));
        }
    }
    for cap in USER_OSC_PATTERN.captures_iter(buffer) {
        user = Some(cap[1].to_string());
    }
    (cwd, user)
}

/// Map the identity reported by the interactive shell to the file pane access
/// model. Cached sudo credentials are deliberately not part of this decision:
/// they make a future root switch reusable, but they do not mean the current
/// shell is still privileged after `exit` returns to the login user.
fn resolve_shell_file_access(login_user: &str, shell_user: &str) -> (&'static str, Option<String>) {
    let login_user = login_user.trim();
    let shell_user = shell_user.trim();
    if login_user.is_empty() || shell_user.is_empty() || login_user == shell_user {
        ("user", None)
    } else {
        ("root", Some(shell_user.to_string()))
    }
}

/// Remove CSI/OSC control sequences before inspecting a prompt. This mirrors
/// Electron's root-prompt heuristic without feeding visual escape codes into
/// the comparison.
///
/// The regexes are pre-compiled: `visible_shell_text` is on the shell data
/// hot path (called per chunk for sudo prompt tracking and root prompt
/// detection), and re-compiling them per chunk burned enough CPU to
/// noticeably stretch `terminal_input_rx` polling latency under
/// high-throughput output (e.g. `pacman-key --populate`).
static VISIBLE_SHELL_CSI_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("constant CSI regex"));
static VISIBLE_SHELL_OSC_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").expect("constant OSC regex")
});

fn visible_shell_text(value: &str) -> String {
    let stripped = VISIBLE_SHELL_CSI_RE.replace_all(value, "");
    VISIBLE_SHELL_OSC_RE.replace_all(&stripped, "").into_owned()
}

fn looks_like_root_prompt(value: &str) -> bool {
    visible_shell_text(value).trim_end().ends_with('#')
}

fn looks_like_shell_prompt(value: &str) -> bool {
    let visible = visible_shell_text(value);
    let prompt = visible.trim_end();
    prompt.ends_with('$') || prompt.ends_with('#') || prompt.ends_with('%') || prompt.ends_with('>')
}

/// 在等待 shell 第一个 prompt 期间，把 chunk 里"prompt 尾部"从 forward 文本
/// 里剥离出来——只 forward banner 部分（保留原始 escape 序列和颜色），prompt
/// 部分由调用方暂存到 `shell_prompt_buffer` 用于触发 setup 注入。
///
/// 这样 shell 启动期间输出的 prompt 不会立即显示给用户；setup 注入成功后
/// suppress 接管，新 prompt 由 suppress 释放时统一 forward，用户只看到一个
/// prompt。群晖 DSM 的 /etc/profile 等启动脚本可能在第一个 prompt 之后还
/// 异步执行命令并输出新 prompt，这些都会被暂存而非 forward。
///
/// 切分在原始 chunk 上进行：从末尾往前找第一个 prompt 结尾符（$ / # / % / >），
/// 再从该位置往前找行首（跳过 escape 序列），行首之前是 banner（forward），
/// 之后是 prompt 尾部（暂存）。找不到则整个 chunk 作为 banner forward。
fn split_prompt_tail_for_setup_wait(chunk: &str) -> (String, String) {
    let bytes = chunk.as_bytes();
    let mut prompt_end_idx: Option<usize> = None;
    // 从末尾往前找第一个 prompt 结尾符，遇到换行则停（说明最后一行不是 prompt）
    for i in (0..bytes.len()).rev() {
        let c = bytes[i] as char;
        if c == '$' || c == '#' || c == '%' || c == '>' {
            prompt_end_idx = Some(i);
            break;
        }
        if c == '\n' {
            break;
        }
    }
    let Some(end_idx) = prompt_end_idx else {
        return (chunk.to_string(), String::new());
    };
    // 从 prompt 结尾符往前找行首：跳过同行所有字符直到遇到换行或 chunk 开头。
    // escape 序列（CSI/OSC）如果出现在 prompt 行内（比如彩色 prompt），会被
    // 一起划入 prompt 尾部暂存，不会丢失——暂存的 prompt 尾部不 forward，
    // setup 注入后由 shell 输出的新 prompt（含颜色）替代。
    let mut line_start = end_idx;
    while line_start > 0 && bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let banner = chunk[..line_start].to_string();
    let prompt_tail = chunk[line_start..].to_string();
    (banner, prompt_tail)
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
            // 用户输入可含 CJK，滚动窗口必须 char 边界安全，否则此分支
            // panic 会无声杀死 worker（输入通道随之失效，Ctrl+C 无响应）。
            trim_string_front(recent_input, 256);
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

fn coalesce_terminal_input(
    mut first: String,
    receiver: &mut mpsc::UnboundedReceiver<String>,
) -> String {
    while let Ok(next) = receiver.try_recv() {
        first.push_str(&next);
    }
    first
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
        // shell 输出含中文时直接字节切片会 panic 杀死 worker，
        // 滚动窗口必须 char 边界安全。
        trim_string_front(prompt_buffer, 1024);
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
        // setup 成功执行（检测到 OSC marker）后，shell 会输出新 prompt。
        // 第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存（不 forward），
        // 所以这里释放新 prompt——让用户看到一个完整 prompt，而不是空白。
        if state.marker_seen_at.is_some() {
            // buffer 里同时含 setup echo、OSC marker 和新 prompt。用 OSC7 正则
            // 找到最后一个 marker 的结束位置，释放它之后的部分（新 prompt），
            // 吞掉 setup echo 和 marker。marker 后可能直接接 prompt（无换行），
            // 所以不能用 rfind('\n') 切分。
            if let Some(mat) = SHELL_SETUP_OSC7_RE.find(&state.buffer) {
                let after_marker = &state.buffer[mat.end()..];
                if looks_like_shell_prompt(after_marker) {
                    return after_marker.to_string();
                }
            }
            // 新 prompt 还没到（慢设备，settle/timeout 到期仍未见）：补换行
            // 让晚到的新 prompt 从新行开始。
            return "\r\n".to_string();
        }
        return String::new();
    }
    state
        .visible_prefix_length
        .map(|length| state.buffer[..length].to_string())
        .unwrap_or_default()
}

// Pre-compiled OSC7 matcher used by `suppress_shell_setup_echo` while it
// inspects buffered shell-setup output. Compiled once instead of per chunk.
static SHELL_SETUP_OSC7_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\x1b\]7;file://[^\x07\x1b]*(?:\x07|\x1b\\)").expect("constant OSC7 regex")
});

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

    let state = pending
        .as_mut()
        .expect("pending CWD hook suppression exists");

    state.buffer.push_str(chunk);
    const HOOK_MARKER: &str = "__tdcwd";

    if SHELL_SETUP_OSC7_RE.is_match(&state.buffer) {
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
        // marker 已看到后，setup 命令执行完 shell 会输出新 prompt。一旦新 prompt
        // 到达（OSC marker 之后的部分匹配 prompt 结尾），立即结束 suppress 并
        // 释放新 prompt。第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存
        // （不 forward），所以这里释放新 prompt 让用户看到一个完整 prompt。
        // 慢设备（群晖）新 prompt 可能晚于 settle delay 到达，固定窗口兜不住；
        // 改为检测到 prompt 就提前结束，无论快慢设备都只显示一个 prompt。
        // 仅 preserve_visible_prefix == false（首次注入）路径生效；sudo 重注入
        // 路径需要保留 visible prefix，仍走 settle delay 释放。
        if !state.preserve_visible_prefix {
            if let Some(mat) = SHELL_SETUP_OSC7_RE.find(&state.buffer) {
                let after_marker = &state.buffer[mat.end()..];
                if looks_like_shell_prompt(after_marker) {
                    return finish_shell_setup_suppression(pending);
                }
            }
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
/// - `linux` / `darwin` → bash/zsh/posix-aware hook via PROMPT_COMMAND /
///   precmd / PS1 (macOS bash/zsh support the same hooks as Linux)
/// - `windows` / unknown → `None` (fail-closed, no injection)
///
/// The injected hook defines `__tdcwd` which emits OSC7 (`file://<path>`) and
/// 1337 (`RemoteUser=<user>`) on every prompt, enabling CWD + sudo user
/// tracking without polling.
fn shell_cwd_setup_for_platform(platform: &str) -> Option<&'static str> {
    match platform {
        "busybox" => Some(BUSYBOX_SHELL_CWD_SETUP),
        "linux" | "darwin" => Some(SHELL_CWD_SETUP),
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
            let start = if buf.starts_with(&[0xef, 0xbb, 0xbf]) {
                3
            } else {
                0
            };
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
    if !bytes.len().is_multiple_of(2) {
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
    let channel = wait_for_ssh_stage(
        "SSH jump-host channel setup",
        SSH_HANDSHAKE_TIMEOUT,
        async {
            jump_handle
                .channel_open_direct_tcpip(host, port as u32, "127.0.0.1", 0)
                .await
                .map_err(|error| format!("Jump Host direct-tcpip failed: {error}"))
        },
    )
    .await?;
    wait_for_ssh_stage(
        "SSH handshake via jump host",
        SSH_HANDSHAKE_TIMEOUT,
        async {
            russh::client::connect_stream(config, channel.into_stream(), handler)
                .await
                .map_err(|error| format!("SSH connect via jump host failed: {error}"))
        },
    )
    .await
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
        .get("password")
        .and_then(Value::as_str)
        .unwrap_or("")
        .is_empty()
    {
        return Some("missing-password");
    }
    None
}

async fn ensure_password_credentials(
    profile: &mut Value,
    app: &AppHandle,
    tab_id: &str,
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

    let response = match timeout(SSH_INTERACTION_TIMEOUT, rx).await {
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
) -> Result<Handle<ClientHandler>, String> {
    let mut effective_profile = profile.clone();
    ensure_password_credentials(&mut effective_profile, app, tab_id).await?;
    let profile = &effective_profile;
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
    let config = russh::client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        // Keepalive：NAT/firewall 会静默掐掉空闲 TCP 连接，用户下次操作时
        // 才发现"连接已断"——这种半开连接在 archiso chroot 跑长脚本时
        // 特别常见（脚本几分钟不出数据，NAT 表项过期）。每 30 秒发一个
        // keepalive 包，连续 3 次无响应就主动断开，让 worker 立刻走
        // 重连路径而不是沉默地 await 一个死连接。参考 meatshell 的
        // 30s interval + electerm 的 keepaliveCountMax 设计。
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
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
        let jump_handle = Box::pin(open_session(&jump_profile, app, tab_id)).await?;
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
        let target_result: Result<Handle<ClientHandler>, String> = async {
            let mut target_handle = connect_target_through_jump(
                &jump_handle,
                config.clone(),
                new_client_handler(app, tab_id, &profile_id, &host, port, trusted.clone()),
                &host,
                port,
            )
            .await?;
            match try_authenticate(
                &mut target_handle,
                &username,
                &auth_type,
                profile,
                app,
                tab_id,
            )
            .await?
            {
                AuthenticationResult::Authenticated => Ok(target_handle),
                AuthenticationResult::NeedsFreshKeyboardInteractive => {
                    // russh cannot switch from a rejected password/public-key
                    // exchange to keyboard-interactive on the same handle.
                    // Disconnect the old handle, then open a new channel
                    // through the already-authenticated jump host.
                    let _ = timeout(
                        Duration::from_secs(3),
                        target_handle.disconnect(
                            Disconnect::ByApplication,
                            "switching to keyboard-interactive",
                            "en",
                        ),
                    )
                    .await;
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
                        profile
                            .get("password")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
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
                        let _ = timeout(
                            Duration::from_secs(3),
                            retry_handle.disconnect(
                                Disconnect::ByApplication,
                                "keyboard-interactive authentication failed",
                                "en",
                            ),
                        )
                        .await;
                        Err("SSH Authentication failed (via jump host)".to_string())
                    }
                }
                AuthenticationResult::Rejected => {
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
        }
        .await;

        match target_result {
            Ok(handle) => return Ok(handle),
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
        SSH_TRANSPORT_TIMEOUT,
        connect_ssh_transport(profile, &host, port),
    )
    .await?;
    let mut handle = wait_for_ssh_stage("SSH protocol handshake", SSH_HANDSHAKE_TIMEOUT, async {
        russh::client::connect_stream(
            config.clone(),
            stream,
            new_client_handler(app, tab_id, &profile_id, &host, port, trusted.clone()),
        )
        .await
        .map_err(|error| format!("SSH connect failed: {error}"))
    })
    .await?;
    match try_authenticate(&mut handle, &username, &auth_type, profile, app, tab_id).await? {
        AuthenticationResult::Authenticated => Ok(handle),
        AuthenticationResult::NeedsFreshKeyboardInteractive => {
            // See the equivalent jump-host path above: reconnect before
            // keyboard-interactive fallback so russh never stalls on a
            // rejected authentication handle.
            let _ = timeout(
                Duration::from_secs(3),
                handle.disconnect(
                    Disconnect::ByApplication,
                    "switching to keyboard-interactive",
                    "en",
                ),
            )
            .await;
            let stream = wait_for_ssh_stage(
                "SSH transport reconnection",
                SSH_TRANSPORT_TIMEOUT,
                connect_ssh_transport(profile, &host, port),
            )
            .await?;
            let mut retry_handle =
                wait_for_ssh_stage("SSH protocol re-handshake", SSH_HANDSHAKE_TIMEOUT, async {
                    russh::client::connect_stream(
                        config,
                        stream,
                        new_client_handler(app, tab_id, &profile_id, &host, port, trusted),
                    )
                    .await
                    .map_err(|error| {
                        format!("SSH reconnect for keyboard-interactive failed: {error}")
                    })
                })
                .await?;
            if try_keyboard_interactive(
                &mut retry_handle,
                &username,
                profile
                    .get("password")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
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
                let _ = timeout(
                    Duration::from_secs(3),
                    retry_handle.disconnect(
                        Disconnect::ByApplication,
                        "keyboard-interactive authentication failed",
                        "en",
                    ),
                )
                .await;
                Err("SSH Authentication failed".to_string())
            }
        }
        AuthenticationResult::Rejected => {
            let _ = timeout(
                Duration::from_secs(3),
                handle.disconnect(Disconnect::ByApplication, "authentication rejected", "en"),
            )
            .await;
            Err("SSH Authentication failed".to_string())
        }
    }
}

enum AuthenticationResult {
    Authenticated,
    /// Password/public-key authentication was rejected. A caller that owns
    /// the transport must reconnect before starting keyboard-interactive.
    NeedsFreshKeyboardInteractive,
    Rejected,
}

fn default_ssh_key_paths(home_directory: &Path) -> Vec<PathBuf> {
    DEFAULT_SSH_KEY_FILES
        .iter()
        .map(|file_name| home_directory.join(".ssh").join(file_name))
        .collect()
}

async fn authenticate_private_key_content(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    key_content: &str,
    passphrase: Option<&str>,
    app: &AppHandle,
    tab_id: &str,
) -> Result<bool, String> {
    let key_pair = russh::keys::decode_secret_key(key_content, passphrase)
        .map_err(|error| error.to_string())?;
    // Best-effort: pick the strongest RSA hash the server advertises. For
    // non-RSA keys, hash_alg is ignored by PrivateKeyWithHashAlg::new.
    // 加 timeout：best_supported_rsa_hash 在服务器不响应时可能永久 await，
    // 而 authenticate_private_key_content 在 open_session 阶段调用，卡住
    // 会让 worker 永远起不来。使用 SSH_PASSWORD_AUTH_TIMEOUT 与密码认证
    // 对齐，保持一致的认证阶段超时语义。
    let hash_alg: Option<russh::keys::HashAlg> = if key_pair.algorithm().is_rsa() {
        match wait_for_ssh_stage(
            "SSH RSA hash negotiation",
            SSH_PASSWORD_AUTH_TIMEOUT,
            async {
                handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|e| e.to_string())
            },
        )
        .await
        {
            Ok(Some(Some(hash))) => Some(hash),
            Ok(_) => Some(russh::keys::HashAlg::Sha512),
            Err(error) => {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "ssh",
                    tab_id,
                    format!("RSA hash negotiation failed, falling back to Sha512: {error}"),
                );
                Some(russh::keys::HashAlg::Sha512)
            }
        }
    } else {
        None
    };
    let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash_alg);
    let result = wait_for_ssh_stage(
        "SSH public key authentication",
        SSH_PASSWORD_AUTH_TIMEOUT,
        async {
            handle
                .authenticate_publickey(username, key_with_hash)
                .await
                .map_err(|error| error.to_string())
        },
    )
    .await?;
    crate::services::logging::session(
        app,
        "INFO",
        "ssh",
        tab_id,
        format!(
            "public key authentication completed success={}",
            result.success()
        ),
    );
    Ok(result.success())
}

async fn try_system_authenticate(
    handle: &mut Handle<ClientHandler>,
    username: &str,
    profile: &Value,
    app: &AppHandle,
    tab_id: &str,
) -> Result<AuthenticationResult, String> {
    let mut candidate_found = false;
    let mut authentication_attempted = false;
    let mut candidate_errors = Vec::new();

    // Agent support is Unix-only in russh, but a missing/broken agent must not
    // prevent the default-key fallback (including on Windows).
    // 加 timeout：AgentClient::connect_env / request_identities /
    // authenticate_publickey_with 在 SSH agent 卡住（unix socket 阻塞、
    // agent 进程 hang）时会永久 await，而本函数在 open_session 阶段
    // 调用，卡住会让 worker 永远起不来。
    #[cfg(unix)]
    match wait_for_ssh_stage("SSH agent connect", SSH_PASSWORD_AUTH_TIMEOUT, async {
        russh::keys::agent::client::AgentClient::connect_env()
            .await
            .map_err(|e| e.to_string())
    })
    .await
    {
        Ok(mut agent) => {
            candidate_found = true;
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "SSH agent connected, listing identities",
            );
            match wait_for_ssh_stage(
                "SSH agent list identities",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async { agent.request_identities().await.map_err(|e| e.to_string()) },
            )
            .await
            {
                Ok(identities) => {
                    crate::services::logging::session(
                        app,
                        "INFO",
                        "ssh",
                        tab_id,
                        format!("SSH agent returned {} identities", identities.len()),
                    );
                    for identity in identities {
                        authentication_attempted = true;
                        let public_key = identity.public_key().into_owned();
                        match wait_for_ssh_stage(
                            "SSH agent public key authentication",
                            SSH_PASSWORD_AUTH_TIMEOUT,
                            async {
                                handle
                                    .authenticate_publickey_with(
                                        username, public_key, None, &mut agent,
                                    )
                                    .await
                                    .map_err(|error| error.to_string())
                            },
                        )
                        .await
                        {
                            Ok(result) if result.success() => {
                                return Ok(AuthenticationResult::Authenticated)
                            }
                            Ok(_) => {}
                            Err(error) => candidate_errors.push(error),
                        }
                    }
                }
                Err(error) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        format!("SSH agent list identities failed: {error}"),
                    );
                    candidate_errors.push(error);
                }
            }
        }
        Err(error) => {
            // agent 不可用很常见（Windows、无 agent 的 Linux），只在 DEBUG
            // 级别记录，避免日志噪音。但超时（30s）需要 WARN 提醒用户。
            if error.contains("timed out") {
                crate::services::logging::session(
                    app,
                    "WARN",
                    "ssh",
                    tab_id,
                    format!("SSH agent connect timed out: {error}"),
                );
            }
            candidate_errors.push(error);
        }
    }

    let home_directory = app.path().home_dir().map_err(|error| error.to_string())?;
    let passphrase = profile.get("passphrase").and_then(Value::as_str);
    for path in default_ssh_key_paths(&home_directory) {
        let key_content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                candidate_found = true;
                candidate_errors.push(error.to_string());
                continue;
            }
        };
        candidate_found = true;
        match authenticate_private_key_content(
            handle,
            username,
            &key_content,
            passphrase,
            app,
            tab_id,
        )
        .await
        {
            Ok(true) => return Ok(AuthenticationResult::Authenticated),
            Ok(false) => authentication_attempted = true,
            Err(error) => candidate_errors.push(error),
        }
    }

    if !candidate_found {
        return Err("No SSH agent or default private key found on this computer".to_string());
    }
    if !authentication_attempted && !candidate_errors.is_empty() {
        return Err(format!(
            "Unable to load SSH agent/default private key: {}",
            candidate_errors.remove(0)
        ));
    }
    Ok(AuthenticationResult::Rejected)
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
            let password = profile
                .get("password")
                .and_then(|p| p.as_str())
                .unwrap_or("");
            if password.is_empty() {
                return Err("SSH password is missing".to_string());
            }
            // Some embedded SSH servers do not reply to a direct password
            // request until the client has first sent the RFC-standard
            // `none` probe. Electron's ssh2 client always performs this
            // negotiation before trying the saved password. Mirror that
            // sequence here for compatibility with those servers.
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "password authentication method negotiation started",
            );
            let negotiation = wait_for_ssh_stage(
                "SSH authentication method negotiation",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async {
                    handle
                        .authenticate_none(username)
                        .await
                        .map_err(|error| error.to_string())
                },
            )
            .await?;
            if negotiation.success() {
                crate::services::logging::session(
                    app,
                    "INFO",
                    "ssh",
                    tab_id,
                    "SSH server accepted none authentication",
                );
                return Ok(AuthenticationResult::Authenticated);
            }
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                "password authentication started",
            );
            let res = wait_for_ssh_stage(
                "SSH password authentication",
                SSH_PASSWORD_AUTH_TIMEOUT,
                async {
                    handle
                        .authenticate_password(username, password)
                        .await
                        .map_err(|error| error.to_string())
                },
            )
            .await?;
            crate::services::logging::session(
                app,
                "INFO",
                "ssh",
                tab_id,
                format!(
                    "password authentication response received success={}",
                    res.success()
                ),
            );
            Ok(if res.success() {
                AuthenticationResult::Authenticated
            } else {
                AuthenticationResult::NeedsFreshKeyboardInteractive
            })
        }
        "privateKey" => {
            let (key_content, passphrase) = if let Some(key_id) =
                profile.get("privateKeyId").and_then(|value| value.as_str())
            {
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
                    profile
                        .get("passphrase")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                )
            };

            let authenticated = authenticate_private_key_content(
                handle,
                username,
                &key_content,
                passphrase.as_deref(),
                app,
                tab_id,
            )
            .await?;
            Ok(if authenticated {
                AuthenticationResult::Authenticated
            } else if profile.get("password").and_then(Value::as_str).is_some() {
                AuthenticationResult::NeedsFreshKeyboardInteractive
            } else {
                AuthenticationResult::Rejected
            })
        }
        "keyboard-interactive" => {
            let password = profile
                .get("password")
                .and_then(Value::as_str)
                .unwrap_or("");
            Ok(
                if try_keyboard_interactive(
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
                },
            )
        }
        _ => try_system_authenticate(handle, username, profile, app, tab_id).await,
    }
}

async fn resolve_managed_private_key(
    app: &AppHandle,
    tab_id: &str,
    profile_id: &str,
    key_id: &str,
) -> Result<(String, Option<String>), String> {
    let managed =
        crate::services::ssh_keys::resolve(app, key_id).map_err(|error| error.to_string())?;
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
        state
            .pending_interactions
            .write()
            .await
            .insert(request_id.clone(), tx);
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
        Ok(response)
            if !response
                .get("canceled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false) =>
        {
            let passphrase = response
                .get("passphrase")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Ok(passphrase.map(|value| {
                (
                    value,
                    response
                        .get("savePassphrase")
                        .and_then(|item| item.as_bool())
                        .unwrap_or(false),
                )
            }))
        }
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_arguments)] // Authentication prompts need the full connection identity for safe UI routing.
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
                        .unwrap_or(false) =>
                {
                    response.get("answers").and_then(|answers| {
                        answers.as_array().map(|answers| {
                            answers
                                .iter()
                                .map(|answer| answer.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                    })
                }
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
    // SSH 协议层交互加 timeout：authenticate_keyboard_interactive_start /
    // respond 在服务器不响应时可能永久 await，而本函数在 open_session
    // 阶段调用，卡住会让 worker 永远起不来。request_answers 等待用户
    // 输入 MFA，不加 timeout。
    let res = wait_for_ssh_stage(
        "SSH keyboard-interactive start",
        SSH_PASSWORD_AUTH_TIMEOUT,
        async {
            handle
                .authenticate_keyboard_interactive_start(username, None)
                .await
                .map_err(|e| e.to_string())
        },
    )
    .await?;

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

        let res = wait_for_ssh_stage(
            "SSH keyboard-interactive respond",
            SSH_PASSWORD_AUTH_TIMEOUT,
            async {
                handle
                    .authenticate_keyboard_interactive_respond(answers)
                    .await
                    .map_err(|e| e.to_string())
            },
        )
        .await?;
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

/// Shell channel 建立阶段的单步超时。`channel_open_session` /
/// `request_pty` / `request_shell` 任一卡住都会让 worker 永远起不来——
/// 表现为"连接主机"loading 永不结束，所有后续命令（包括 Ctrl+C）都
/// 进不了 cmd_rx。服务器在 PTY 协商阶段卡住（罕见但确实发生过，尤其
/// 是某些嵌入式 dropbear / 网络设备）时，russh 默认无超时，会一直
/// await。8 秒与 SFTP_INIT_STEP_TIMEOUT 对齐，足够覆盖正常 RTT 与
/// 一次重试，同时不让用户对着 loading 望穿秋水。
const SHELL_INIT_STEP_TIMEOUT: Duration = Duration::from_secs(8);

/// `probe_remote_platform` 总超时。该函数在 worker 主循环之前调用，
/// 内部最多尝试 4 次 exec_command（POSIX + 3 个 Windows probe），每次
/// 都用 `channel.wait()` 循环读取，没有内层 timeout。如果服务器在 exec
/// 模式下卡住（不返回 EOF/Close），整个 probe 会永久 await，worker
/// 永远起不来，所有后续命令（含 Ctrl+C）都进不了 cmd_rx。20 秒覆盖
/// 最坏情况下的 4 次串行尝试 + RTT，超时后回落到 "unknown" 平台，
/// shell CWD 注入会被 fail-closed 门控跳过，不影响终端基本可用性。
const PLATFORM_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// SSH 隧道控制操作（tcpip_forward / cancel_tcpip_forward）的单步超时。
/// 这两个调用在 `handle_worker_cmd` 的 inline await 路径上，服务器卡住
/// 时会直接阻塞 worker 主循环，导致终端 select! 无法响应 Ctrl+C。
/// 5 秒覆盖正常 RTT 与一次重试，超时后让用户拿到明确错误而不是沉默
/// 地 hang 住整个会话。
const SSH_TUNNEL_OP_TIMEOUT: Duration = Duration::from_secs(5);

/// sudo 凭据验证超时。`exec_shell_file_command` 用 PTY 模式 exec，sudo
/// 密码错误时会重新 prompt 等待输入且不会自然退出，channel.wait() 永久
/// 阻塞。这里强制 10 秒收口，让前端 RootAccessModal 的 loading 状态能
/// 在合理时间内解除。
const SUDO_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
/// Inline `SetRemoteFileAccessMode` verification budget. The full
/// `SUDO_VERIFY_TIMEOUT` (10s) is appropriate for spawned file operations,
/// but `SetRemoteFileAccessMode` runs inline on the worker loop — waiting
/// the full 10 seconds would freeze `terminal_input_rx` polling and make
/// Ctrl+C unresponsive while the user waits for the root-mode toggle to
/// finish. 1.5s is enough for a healthy sudo round-trip; slower responses
/// surface as a user-visible error instead of a frozen terminal.
const ROOT_ACCESS_VERIFY_TIMEOUT: Duration = Duration::from_millis(1500);

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
    timeout(
        SFTP_INIT_STEP_TIMEOUT,
        sftp_channel.request_subsystem(true, "sftp"),
    )
    .await
    .map_err(|_| "SFTP init failed: 请求 subsystem 超时".to_string())?
    .map_err(|error| format!("SFTP subsystem request failed: {error}"))?;
    timeout(
        SFTP_INIT_STEP_TIMEOUT,
        SftpSession::new(sftp_channel.into_stream()),
    )
    .await
    .map_err(|_| "SFTP init failed: 协议握手超时".to_string())?
    .map_err(|error| format!("SFTP init failed: {error}"))
}

type SharedSftpSession = Arc<RwLock<SftpSession>>;
type TransferSftpSlot = Arc<Mutex<Option<SharedSftpSession>>>;

fn is_sftp_not_found(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status(status) if status.status_code == StatusCode::NoSuchFile
    ) || error
        .to_string()
        .to_ascii_lowercase()
        .contains("no such file")
}

async fn acquire_transfer_sftp(
    handle: &Handle<ClientHandler>,
    primary: &SharedSftpSession,
    slot: &TransferSftpSlot,
    app: &AppHandle,
    tab_id: &str,
) -> SharedSftpSession {
    let mut slot_guard = slot.lock().await;
    if let Some(session) = slot_guard.as_ref() {
        return Arc::clone(session);
    }
    match open_sftp_session(handle).await {
        Ok(session) => {
            let session = Arc::new(RwLock::new(session));
            *slot_guard = Some(Arc::clone(&session));
            crate::services::logging::session(
                app,
                "INFO",
                "sftp",
                tab_id,
                "dedicated transfer channel opened",
            );
            session
        }
        Err(error) => {
            crate::services::logging::session(
                app,
                "WARN",
                "sftp",
                tab_id,
                format!("dedicated transfer channel unavailable; using browse channel: {error}"),
            );
            Arc::clone(primary)
        }
    }
}

async fn invalidate_transfer_sftp(
    session: &SharedSftpSession,
    primary: &SharedSftpSession,
    slot: &TransferSftpSlot,
) {
    if Arc::ptr_eq(session, primary) {
        return;
    }
    let mut slot_guard = slot.lock().await;
    if slot_guard
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, session))
    {
        *slot_guard = None;
    }
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
    terminal_input_rx: &mut mpsc::UnboundedReceiver<String>,
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
    let handle: Arc<Handle<ClientHandler>> = match open_session(profile, app, tab_id).await {
        Ok(h) => Arc::new(h),
        Err(error) => {
            crate::services::logging::session(
                app,
                "ERROR",
                "ssh",
                tab_id,
                format!("open_session failed: {error}"),
            );
            return Err(error);
        }
    };
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "SSH session established");

    // ── Shell channel ──────────────────────────────────────────────────────
    // 三步都加 timeout：服务器在 PTY 协商阶段卡住（嵌入式 dropbear /
    // 网络设备偶发）时 russh 默认无超时，会永久 await，worker 永远起
    // 不来，所有后续命令（含 Ctrl+C）都进不了 cmd_rx。
    let shell_channel = match timeout(SHELL_INIT_STEP_TIMEOUT, handle.channel_open_session()).await
    {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            let msg = format!("无法打开 shell channel: {e}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 channel_open_session".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    };
    match timeout(
        SHELL_INIT_STEP_TIMEOUT,
        shell_channel.request_pty(
            true,
            "xterm-256color",
            80,
            24,
            0,
            0,
            &[
                (russh::Pty::TTY_OP_ISPEED, 115200),
                (russh::Pty::TTY_OP_OSPEED, 115200),
            ],
        ),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let msg = format!("request_pty failed: {err}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 request_pty".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    }
    match timeout(SHELL_INIT_STEP_TIMEOUT, shell_channel.request_shell(true)).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            let msg = format!("request_shell failed: {err}");
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
        Err(_) => {
            let msg = "Shell channel 建立超时：服务器未响应 request_shell".to_string();
            crate::services::logging::session(app, "ERROR", "ssh", tab_id, &msg);
            return Err(msg);
        }
    }
    crate::services::logging::session(app, "INFO", "ssh", tab_id, "shell channel ready");
    let (mut shell_reader, shell_writer) = shell_channel.split();
    let shell_writer = Arc::new(shell_writer);

    // Normal terminal bytes are serialized here so a slow SSH channel cannot
    // block the session event loop. Ctrl+C bypasses this queue below via the
    // SSH SIGINT request and also keeps its raw 0x03 byte as a fallback.
    let (terminal_write_tx, mut terminal_write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let terminal_writer = Arc::clone(&shell_writer);
    let terminal_writer_cancellation = cancellation.clone();
    let terminal_writer_app = app.clone();
    let terminal_writer_tab_id = tab_id.to_string();
    let _terminal_writer_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = terminal_writer_cancellation.cancelled() => break,
                data = terminal_write_rx.recv() => {
                    let Some(data) = data else { break };
                    if let Err(error) = write_shell_data(&terminal_writer, data).await {
                        crate::services::logging::session(
                            &terminal_writer_app,
                            "WARN",
                            "ssh",
                            &terminal_writer_tab_id,
                            format!("terminal write failed: {error}"),
                        );
                    }
                }
            }
        }
    });

    // ── Probe platform ─────────────────────────────────────────────────────
    // 加 timeout：probe 内部最多 4 次串行 exec_command，每次都用
    // channel.wait() 循环读取且无内层 timeout。服务器在 exec 模式下卡住
    // 时整个 probe 会永久 await，worker 永远起不来。超时后回落到
    // "unknown"，shell CWD 注入会被 fail-closed 门控跳过，终端仍可用。
    let platform = match timeout(
        PLATFORM_PROBE_TIMEOUT,
        super::system_metrics::probe_remote_platform(&handle),
    )
    .await
    {
        Ok(p) => p,
        Err(_) => {
            crate::services::logging::session(
                app,
                "WARN",
                "metrics",
                tab_id,
                "platform probe timed out, falling back to unknown",
            );
            "unknown".to_string()
        }
    };
    crate::services::logging::session(
        app,
        "INFO",
        "metrics",
        tab_id,
        format!("platform probe completed platform={platform}"),
    );

    // ── Inject shell CWD setup (POSIX only, fail-closed) ───────────────────
    // Mirrors Electron's `supportsPosixShellSetup()` + `injectShellSetup()`
    // double gate. Only `linux` / `busybox` get the OSC7/RemoteUser hook
    // injected; Windows / unknown are left untouched so we never push a
    // POSIX script into a non-POSIX shell.
    let mut pending_shell_setup_echo = None;
    let mut shell_setup_waiting_for_prompt = shell_cwd_setup_for_platform(&platform).is_some();
    let mut shell_prompt_buffer = String::new();
    if let Some(setup) = shell_cwd_setup_for_platform(&platform) {
        crate::services::logging::session(
            app,
            "DEBUG",
            "ssh",
            tab_id,
            format!(
                "shell setup waiting for prompt platform={platform} bytes={}",
                setup.len()
            ),
        );
    } else {
        crate::services::logging::session(
            app,
            "DEBUG",
            "ssh",
            tab_id,
            format!("shell setup skipped platform={platform}"),
        );
    }

    update_tab_status_and_emit(app, tab_id, WorkspaceTabStatus::Connected).await;

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
        let existing_reconnect_mode = sessions
            .get(tab_id)
            .and_then(|session| session.reconnect_mode.clone());
        let existing_remote_path = sessions
            .get(tab_id)
            .map(|session| session.remote_path.clone())
            .unwrap_or_else(|| {
                crate::services::workspace::initial_remote_path_for_profile(profile)
            });
        let existing_shell_cwd = sessions
            .get(tab_id)
            .and_then(|session| session.shell_cwd.clone());
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
                remote_path: existing_remote_path,
                shell_cwd: existing_shell_cwd,
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
                capabilities: crate::services::workspace::ConnectionCapabilities::for_session_type(
                    "ssh",
                ),
                reconnect_mode: existing_reconnect_mode
                    .or_else(|| crate::services::workspace::reconnect_mode_for_profile(profile)),
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
            crate::services::logging::session(app, "INFO", "sftp", tab_id, "SFTP session ready");
            let sftp_arc = Arc::new(RwLock::new(sftp));
            let initial_remote_path = {
                let sessions = state.sessions.read().await;
                sessions
                    .get(tab_id)
                    .map(|session| session.remote_path.clone())
                    .unwrap_or_else(|| {
                        crate::services::workspace::initial_remote_path_for_profile(profile)
                    })
            };
            // A server can accept the SFTP subsystem and then stop replying
            // to read_dir. Do not await the initial directory load before the
            // terminal select loop: otherwise Ctrl+C reaches IPC but cannot be
            // consumed until the SFTP request returns. The bound includes both
            // the lock wait and read_dir; the task publishes its own snapshot.
            {
                let mut sessions = state.sessions.write().await;
                if let Some(session) = sessions.get_mut(tab_id) {
                    session.remote_files_loading = true;
                }
            }
            let initial_sftp = Arc::clone(&sftp_arc);
            let initial_app = app.clone();
            let initial_tab_id = tab_id.to_string();
            tokio::spawn(async move {
                let initial_files = match timeout(FILE_OPERATION_TIMEOUT, async {
                    let sftp = initial_sftp.write().await;
                    list_dir(&sftp, &initial_remote_path).await
                })
                .await
                {
                    Ok(result) => result,
                    Err(_) => Err(format!("列出远程目录 {initial_remote_path} 超时")),
                };

                let initial_error = initial_files.as_ref().err().cloned();
                let state = initial_app.state::<crate::services::workspace::WorkspaceState>();
                if let Some(session) = state.sessions.write().await.get_mut(&initial_tab_id) {
                    session.remote_files_loading = false;
                    if let Ok(files) = initial_files {
                        session.remote_files = files;
                    }
                }

                if let Some(error) = initial_error {
                    crate::services::logging::session(
                        &initial_app,
                        "WARN",
                        "sftp",
                        &initial_tab_id,
                        format!("initial directory listing failed: {error}"),
                    );
                    // A usable SFTP channel can still lack access to the
                    // profile's configured starting directory.
                    emit_terminal_data(
                        &initial_app,
                        &initial_tab_id,
                        &format!("\r\n[files] 列出目录 {initial_remote_path} 失败: {error}\r\n"),
                    )
                    .await;
                }

                if let Ok(snapshot) =
                    crate::commands::get_workspace_snapshot(initial_app.clone()).await
                {
                    let _ = initial_app.emit("workspace:snapshot", snapshot);
                }
            });
            (Some(sftp_arc), None)
        }
        Err(error) => {
            let reason = format_sftp_unavailable_reason(&error);
            crate::services::logging::session(
                app,
                "WARN",
                "sftp",
                tab_id,
                format!("unavailable: {reason}"),
            );
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
    let transfer_sftp_slot: TransferSftpSlot = Arc::new(Mutex::new(None));

    // Push the full snapshot (with files) to the renderer
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
    if sftp_arc.is_some() {
        let cleanup_app = app.clone();
        let cleanup_tab_id = tab_id.to_string();
        tokio::spawn(async move {
            if let Err(error) = crate::services::transfers::retry_pending_cleanup_for_tab(
                &cleanup_app,
                &cleanup_tab_id,
            )
            .await
            {
                crate::services::logging::warn(
                    &cleanup_app,
                    &format!("transfer:{cleanup_tab_id}"),
                    format!("pending cleanup retry failed: {error}"),
                );
            }
        });
    }

    // ── Spawn metrics collection task (single persistent channel) ─────────
    // Instead of opening a new exec channel every second (which adds variable
    // SSH overhead and makes the refresh cadence jittery), we open one
    // long-lived shell channel and pipe an infinite-loop script into it.
    // The remote side controls the 1s cadence via `sleep 1`, so data arrives
    // at a rock-steady interval regardless of SSH RTT.
    let metrics_shutdown = Arc::new(tokio::sync::Notify::new());
    if resource_monitoring_enabled(profile) {
        let metrics_shutdown_clone = metrics_shutdown.clone();
        let metrics_handle = Arc::clone(&handle);
        let metrics_app = app.clone();
        let metrics_tid = tab_id.to_string();
        let metrics_plat = platform.clone();
        let metrics_cancellation = cancellation.clone();
        tokio::spawn(async move {
            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                format!("collector starting platform={metrics_plat}"),
            );

            // Build the infinite-loop script. Each iteration emits a
            // delimited metrics block and sleeps for 1 second. We use a
            // unique marker so the stream parser can reliably slice blocks.
            let marker = "__FILETERM_METRICS_BLOCK__";
            let (windows_command, script_body) = if metrics_plat == "windows" {
                let command =
                    match super::system_metrics::build_windows_streaming_metrics_exec_command() {
                        Ok(command) => command,
                        Err(error) => {
                            crate::services::logging::ssh_debug(
                                &metrics_app,
                                &metrics_tid,
                                format!("Windows streaming metrics command build failed: {error}"),
                            );
                            return;
                        }
                    };
                (Some(command), None)
            } else {
                // POSIX: wrap the metrics script in a while-true loop
                let raw = if metrics_plat == "busybox" {
                    "busybox"
                } else {
                    "linux"
                };
                let metrics = super::system_metrics::build_posix_metrics_command(raw);
                let script = format!(
                    "{}\nwhile true; do\n{}\necho '{}'\nsleep 1\ndone\n",
                    "cd / >/dev/null 2>&1 || true", metrics, marker
                );
                (None, Some(script))
            };

            // Open one persistent shell channel for the entire session.
            // 加 timeout：服务器 MaxSessions 满或网络抖动时这一步会卡住，
            // 不加超时 metrics task 会永久 await，虽然不阻塞主循环，但
            // 用户看不到系统监控数据且 worker 不会自动重试。
            let mut channel = match timeout(
                SHELL_INIT_STEP_TIMEOUT,
                metrics_handle.channel_open_session(),
            )
            .await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    crate::services::logging::session(
                        &metrics_app,
                        "ERROR",
                        "metrics",
                        &metrics_tid,
                        format!("open channel failed: {e}"),
                    );
                    return;
                }
                Err(_) => {
                    crate::services::logging::session(
                        &metrics_app,
                        "ERROR",
                        "metrics",
                        &metrics_tid,
                        "open channel timed out",
                    );
                    return;
                }
            };

            // Windows OpenSSH on this host stalls when a large script is sent
            // through stdin. Match Electron's transport: gzip + base64 keeps
            // the loader below cmd.exe's safe command-line budget, while the
            // decoded script runs as one persistent PowerShell process.
            let collector_start = if let Some(command) = windows_command.as_deref() {
                timeout(SHELL_INIT_STEP_TIMEOUT, channel.exec(true, command)).await
            } else {
                timeout(SHELL_INIT_STEP_TIMEOUT, channel.request_shell(true)).await
            };
            let collector_start = match collector_start {
                Ok(inner) => inner,
                Err(_) => {
                    crate::services::logging::session(
                        &metrics_app,
                        "ERROR",
                        "metrics",
                        &metrics_tid,
                        "start collector timed out",
                    );
                    return;
                }
            };
            if let Err(e) = collector_start {
                crate::services::logging::session(
                    &metrics_app,
                    "ERROR",
                    "metrics",
                    &metrics_tid,
                    format!("start collector failed: {e}"),
                );
                return;
            }

            if let Some(script) = script_body.as_deref() {
                // 写脚本也加 timeout：Windows OpenSSH 在大脚本场景偶发 stall，
                // 不加超时会让 metrics task 永久卡在 data() 调用上。
                match timeout(SHELL_INIT_STEP_TIMEOUT, channel.data(script.as_bytes())).await {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        crate::services::logging::session(
                            &metrics_app,
                            "ERROR",
                            "metrics",
                            &metrics_tid,
                            format!("write collector script failed: {e}"),
                        );
                        return;
                    }
                    Err(_) => {
                        crate::services::logging::session(
                            &metrics_app,
                            "ERROR",
                            "metrics",
                            &metrics_tid,
                            "write collector script timed out",
                        );
                        return;
                    }
                }
            }

            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                "collector started; waiting for first sample",
            );

            // Stream reader: accumulate data, split on the marker, parse
            // each complete block and emit it to the renderer.
            let mut buffer: Vec<u8> = Vec::new();
            let marker_bytes = marker.as_bytes();
            let mut sample_count = 0_u64;

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
                                    sample_count += 1;
                                    if sample_count == 1 {
                                        crate::services::logging::session(
                                            &metrics_app,
                                            "INFO",
                                            "metrics",
                                            &metrics_tid,
                                            format!("first sample cpu_percent={cpu_pct:.1} memory_percent={mem_pct:.1}"),
                                        );
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
                                crate::services::logging::session(&metrics_app, "WARN", "metrics", &metrics_tid, "collector channel closed");
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            let _ = channel.close().await;
            crate::services::logging::session(
                &metrics_app,
                "INFO",
                "metrics",
                &metrics_tid,
                "collector stopped",
            );
        });
    } else {
        let mut sessions = state.sessions.write().await;
        if let Some(session) = sessions.get_mut(tab_id) {
            session.system_metrics = None;
        }
        crate::services::logging::session(
            app,
            "INFO",
            "metrics",
            tab_id,
            "collection disabled by profile",
        );
    }

    // ── Main event loop: terminal reads + command dispatch ─────────────────
    let mut cwd_buffer = String::new();
    let mut batch_buffer: Vec<u8> = Vec::new();
    let mut last_emit = Instant::now();

    // Terminal output pump: 解耦 worker 主循环与 renderer IPC 推送。
    // flush_batch 用 try_send 把 chunk 推到这个 bounded channel，独立的
    // pump task 异步消费并调 emit_terminal_data（含 channel.send + RwLock
    // 写）。这样高吞吐输出（pacman-key --populate）时 worker 主循环的
    // select! 永远不会被 IPC 推送或 RwLock 竞争阻塞，Ctrl+C 路径始终
    // 畅通。通道满时丢弃旧 chunk（终端输出是尽力而为的，丢几帧不影响
    // 功能，但 Ctrl+C 必须响应）。容量 128 覆盖 16ms × 8MB/s 的峰值。
    let (terminal_output_tx, mut terminal_output_rx) = tokio::sync::mpsc::channel::<String>(128);
    let pump_app = app.clone();
    let pump_tab_id = tab_id.to_string();
    let _pump_handle = tokio::spawn(async move {
        while let Some(chunk) = terminal_output_rx.recv().await {
            emit_terminal_data(&pump_app, &pump_tab_id, &chunk).await;
        }
    });

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
    let mut auto_start_tunnel_ids = Vec::new();
    if let Some(rules) = profile.get("forwards").and_then(Value::as_array) {
        for raw_rule in rules {
            match serde_json::from_value::<SshTunnelRule>(raw_rule.clone()) {
                Ok(rule) => {
                    let should_start = rule.auto_start;
                    if let Err(error) = tunnel_manager.register(rule.clone(), false) {
                        emit_terminal_data(
                            app,
                            tab_id,
                            &format!("[tunnel] 忽略无效规则: {error}\r\n"),
                        )
                        .await;
                    } else if should_start {
                        auto_start_tunnel_ids.push(rule.id);
                    }
                }
                Err(error) => {
                    emit_terminal_data(app, tab_id, &format!("[tunnel] 解析规则失败: {error}\r\n"))
                        .await
                }
            }
        }
    }
    // Keep potentially slow tunnel control operations out of the terminal
    // worker. The queue preserves command order (for example Start → Stop)
    // while its own task absorbs server-side request/cancel waits.
    let (tunnel_command_tx, tunnel_command_rx) = mpsc::unbounded_channel();
    tokio::spawn(run_tunnel_command_loop(tunnel_manager, tunnel_command_rx));
    for rule_id in auto_start_tunnel_ids {
        let (respond_to, response_rx) = oneshot::channel();
        enqueue_tunnel_command(
            &tunnel_command_tx,
            TunnelCommand::Start {
                rule_id: rule_id.clone(),
                respond_to,
            },
        );
        let auto_tunnel_app = app.clone();
        let auto_tunnel_tab_id = tab_id.to_string();
        tokio::spawn(async move {
            match response_rx.await {
                Ok(Err(error)) => {
                    emit_terminal_data(
                        &auto_tunnel_app,
                        &auto_tunnel_tab_id,
                        &format!("[tunnel] 自动启动 {rule_id} 失败: {error}\r\n"),
                    )
                    .await;
                }
                Err(_) => {
                    crate::services::logging::session(
                        &auto_tunnel_app,
                        "WARN",
                        "tunnel",
                        &auto_tunnel_tab_id,
                        format!("auto-start response dropped id={rule_id}"),
                    );
                }
                Ok(Ok(_)) => {}
            }
        });
    }

    loop {
        // 16ms batch window for terminal output.
        let next_batch_deadline =
            tokio::time::Instant::from_std(last_emit + Duration::from_millis(16));

        tokio::select! {
            _ = cancellation.cancelled() => {
                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                metrics_shutdown.notify_waiters();
                return Ok(());
            }
            input = terminal_input_rx.recv() => {
                let Some(data) = input else {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                    metrics_shutdown.notify_waiters();
                    return Ok(());
                };
                let data = coalesce_terminal_input(data, terminal_input_rx);
                if capture_sudo_password_input(
                    &data,
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
                if contains_interrupt_byte(&data) {
                    // Fire-and-forget: the SIGINT request used to be awaited
                    // inline for up to TERMINAL_INTERRUPT_TIMEOUT (500ms).
                    // Under high-throughput shell output that 500ms stalled
                    // the next `select!` iteration, so a second Ctrl+C press
                    // was effectively swallowed. Spinning the signal off to
                    // its own task lets the main loop immediately poll
                    // `terminal_input_rx` again for follow-up interrupts.
                    let sigint_writer = Arc::clone(&shell_writer);
                    let sigint_app = app.clone();
                    let sigint_tab_id = tab_id.to_string();
                    tokio::spawn(async move {
                        match timeout(
                            TERMINAL_INTERRUPT_TIMEOUT,
                            sigint_writer.signal(Sig::INT),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                crate::services::logging::session(
                                    &sigint_app,
                                    "WARN",
                                    "ssh",
                                    &sigint_tab_id,
                                    format!("terminal SIGINT request failed: {error}"),
                                );
                            }
                            Err(_) => {
                                crate::services::logging::session(
                                    &sigint_app,
                                    "WARN",
                                    "ssh",
                                    &sigint_tab_id,
                                    "terminal SIGINT request timed out",
                                );
                            }
                        }
                    });
                }
                terminal_write_tx
                    .send(data.into_bytes())
                    .map_err(|_| "Terminal writer stopped".to_string())?;
            }
            // Commands and shell output intentionally share Tokio's fair
            // selection. Making this branch unconditionally preferred lets a
            // stream of Enter keypresses starve both shell reads and the 16ms
            // output flush, so the terminal appears to freeze and then jumps.
            // When the sender is dropped (reconnect / disconnect / close),
            // `recv()` returns None and we must exit — otherwise the old
            // worker keeps publishing terminal output alongside the new worker.
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
                                &shell_writer,
                                sftp,
                                &transfer_sftp_slot,
                                &mut file_access_mode,
                                &mut sudo_user,
                                &mut sudo_password,
                                tab_id,
                                app,
                                &state,
                                &tunnel_command_tx,
                            ).await
                        } else {
                            handle_worker_cmd_without_sftp(
                                cmd,
                                &handle,
                                &shell_writer,
                                &mut file_access_mode,
                                &mut sudo_user,
                                &mut sudo_password,
                                tab_id,
                                &state,
                                &tunnel_command_tx,
                                sftp_unavailable_reason.as_deref().unwrap_or(SFTP_UNAVAILABLE_FALLBACK),
                            ).await
                        };
                        match result {
                            Ok(true) => {
                                // WorkerCmd::Disconnect requested — flush and exit.
                                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                                metrics_shutdown.notify_waiters();
                                return Ok(());
                            }
                            Ok(false) => {}
                            Err(e) => {
                                crate::services::logging::session(app, "WARN", "ssh", tab_id, format!("command failed: {e}"));
                            }
                        }
                    }
                    None => {
                        // Sender dropped — flush and exit cleanly.
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
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
            msg = shell_reader.wait() => {
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
                        let mut session_state_changed = false;
                        if new_cwd.is_some() || new_user.is_some() {
                            let mut sessions = state.sessions.write().await;
                            if let Some(s) = sessions.get_mut(tab_id) {
                                if let Some(cwd) = new_cwd {
                                    if s.shell_cwd.as_deref() != Some(cwd.as_str()) {
                                        crate::services::logging::ssh_debug(
                                            app,
                                            tab_id,
                                            format!("Shell CWD reported: {cwd}"),
                                        );
                                        s.shell_cwd = Some(cwd.clone());
                                        session_state_changed = true;
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
                                        session_state_changed = true;
                                        // 对照 Electron resolveShellFileAccess：
                                        // shell user != login user ⇒ 自动切 root 视角
                                        // shell user == login user ⇒ 切回 user 视角
                                        let login = s.login_user.clone();
                                        if let Some(login_user) = login {
                                            let (target_mode, observed_sudo_user) =
                                                resolve_shell_file_access(&login_user, user);
                                            if s.file_access_mode != target_mode {
                                                s.file_access_mode = target_mode.to_string();
                                                if let Some(observed_sudo_user) = observed_sudo_user {
                                                    // sudo -i / su 切到其他用户：用实际 shell
                                                    // 身份作为 root 文件视角的目标用户。
                                                    s.sudo_user = Some(observed_sudo_user.clone());
                                                    s.has_reusable_sudo_auth =
                                                        sudo_password.is_some();
                                                    file_mode_switch = Some((
                                                        target_mode.to_string(),
                                                        Some(observed_sudo_user),
                                                    ));
                                                } else {
                                                    // `exit` 回到登录用户时必须立即恢复 user
                                                    // 视角。保留 sudo_user / 密码缓存只用于下次
                                                    // 手动切 root，不得让工具栏继续显示 root。
                                                    file_mode_switch = Some((
                                                        target_mode.to_string(),
                                                        s.sudo_user.clone(),
                                                    ));
                                                }
                                                // 身份变化即使没有伴随 CWD 变化也要刷新当前
                                                // 目录，确保列表内容和访问模型同步切换。
                                                cwd_to_follow = s.shell_cwd.clone();
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
                            } else if session_state_changed {
                                // 解耦：get_workspace_snapshot 会读整个 sessions
                                // RwLock + 序列化所有 tab 数据，在 shell output 分支
                                // 内同步 await 会阻塞 select! 轮询 terminal_input_rx。
                                // CWD/user 变化频率有限，spawn 到后台不阻塞主循环。
                                let snap_app = app.clone();
                                tokio::spawn(async move {
                                    if let Ok(snap) =
                                        crate::commands::get_workspace_snapshot(snap_app.clone())
                                            .await
                                    {
                                        let _ = snap_app.emit("workspace:snapshot", snap);
                                    }
                                });
                            }
                        }

                        let visible = suppress_shell_setup_echo(&mut pending_shell_setup_echo, &text);
                        // shell_setup_waiting_for_prompt 期间，shell 启动输出的 prompt
                        // 尾部不能立即 forward——否则群晖等设备会显示多个重复 prompt
                        // （shell 启动脚本可能执行命令后再次输出 prompt）。把 prompt 尾部
                        // 剥离暂存到 shell_prompt_buffer，只 forward banner 部分；setup
                        // 注入成功后由 suppress 接管，新 prompt 统一释放。
                        let (forward_text, prompt_tail) = if shell_setup_waiting_for_prompt {
                            split_prompt_tail_for_setup_wait(&visible)
                        } else {
                            (visible, String::new())
                        };
                        if !forward_text.is_empty() {
                            batch_buffer.extend_from_slice(forward_text.as_bytes());
                            // Hard ceiling: under sustained high-throughput output the
                            // 16ms flush timer can lose fairness to this branch; force
                            // a flush so memory stays bounded and the next emit does
                            // not grow a multi-MB chunk in one shot.
                            if batch_buffer.len() >= TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD {
                                flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                                last_emit = Instant::now();
                            }
                        }

                        if shell_setup_waiting_for_prompt {
                            shell_prompt_buffer.push_str(&visible_shell_text(&prompt_tail));
                            if shell_prompt_buffer.len() > 4096 {
                                // char 边界安全裁剪，避免 panic 杀死 worker。
                                trim_string_front(&mut shell_prompt_buffer, 2048);
                            }
                        }

                        if shell_setup_waiting_for_prompt
                            && looks_like_shell_prompt(&shell_prompt_buffer)
                        {
                            shell_setup_waiting_for_prompt = false;
                            shell_prompt_buffer.clear();
                            if let Some(setup) = shell_cwd_setup_for_platform(&platform) {
                                last_shell_setup_injection = Instant::now();
                                let setup_command = format!(" {setup}\r");
                                match write_shell_data(&shell_writer, setup_command.into_bytes()).await {
                                    Ok(()) => {
                                        // setup 注入成功，suppress 接管后续 echo 和新 prompt。
                                        pending_shell_setup_echo =
                                            Some(ShellSetupEchoSuppression::new(false));
                                    }
                                    Err(error) => {
                                        // setup 写入失败：fail-open，把暂存的 prompt 尾部
                                        // forward 出去，避免用户看不到任何 prompt。
                                        if !prompt_tail.is_empty() {
                                            batch_buffer.extend_from_slice(prompt_tail.as_bytes());
                                        }
                                        crate::services::logging::session(app, "WARN", "ssh", tab_id, format!("shell setup write failed: {error}"));
                                    }
                                }
                            }
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
                                    if write_shell_data(&shell_writer, format!(" {setup}\r").into_bytes()).await.is_ok() {
                                        pending_shell_setup_echo = Some(ShellSetupEchoSuppression::new(false));
                                    }
                                }
                            }
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, .. }) => {
                        batch_buffer.extend_from_slice(data.as_ref());
                        if batch_buffer.len() >= TERMINAL_BATCH_BUFFER_FLUSH_THRESHOLD {
                            flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                            last_emit = Instant::now();
                        }
                    }
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                        // Shell closed → flush and disconnect.
                        flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
                        metrics_shutdown.notify_waiters();
                        return Ok(());
                    }
                    _ => {}
                }
            }
            // 3. Periodic flush if there is buffered output.
            _ = tokio::time::sleep_until(next_batch_deadline) => {
                if !batch_buffer.is_empty() {
                    flush_batch(&mut batch_buffer, &terminal_output_tx, app, tab_id);
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
#[allow(clippy::too_many_arguments)] // Worker state is borrowed separately to avoid a second mutable aggregate.
async fn handle_worker_cmd_without_sftp(
    cmd: WorkerCmd,
    handle: &Handle<ClientHandler>,
    shell_writer: &SshShellWriteHalf,
    file_access_mode: &mut String,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    tab_id: &str,
    state: &crate::services::workspace::WorkspaceState,
    tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
    unavailable_reason: &str,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::WriteTerminal(data) => {
            write_shell_data(shell_writer, data.into_bytes()).await?;
            Ok(false)
        }
        WorkerCmd::ResizeTerminal { cols, rows, .. } => {
            // Best-effort resize, mirroring handle_worker_cmd. Without a
            // timeout a stuck SSH transport would freeze the worker loop and
            // make Ctrl+C unresponsive.
            match timeout(
                TERMINAL_RESIZE_TIMEOUT,
                shell_writer.window_change(cols, rows, 0, 0),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => {
                    // SFTP is already unavailable here, so the user already
                    // has a degraded session; do not escalate resize errors.
                }
            }
            Ok(false)
        }
        WorkerCmd::ListSshTunnels { respond_to } => {
            enqueue_tunnel_command(tunnel_commands, TunnelCommand::List { respond_to });
            Ok(false)
        }
        WorkerCmd::CreateSshTunnel { rule, respond_to } => {
            match serde_json::from_value::<SshTunnelRule>(rule) {
                Ok(rule) => enqueue_tunnel_command(
                    tunnel_commands,
                    TunnelCommand::Create { rule, respond_to },
                ),
                Err(error) => {
                    let _ = respond_to.send(Err(format!("Invalid tunnel rule: {error}")));
                }
            }
            Ok(false)
        }
        WorkerCmd::StartSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Start {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::StopSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Stop {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::DeleteSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Delete {
                    rule_id,
                    respond_to,
                },
            );
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
        | WorkerCmd::CommitRemoteStaging { respond_to, .. }
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

            if let Some(next_user) = new_sudo_user.filter(|user| !user.trim().is_empty()) {
                *sudo_user = Some(next_user);
            }
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd);
                }
            }

            if mode == "root" {
                // 与主路径一致：用 ROOT_ACCESS_VERIFY_TIMEOUT 包裹 sudo
                // 验证，避免 `exec_shell_file_command` 内部 10s 超时把
                // worker 主循环卡死，导致终端 select! 无法响应 Ctrl+C。
                let verify = match timeout(
                    ROOT_ACCESS_VERIFY_TIMEOUT,
                    exec_shell_file_command(handle, "true", sudo_user, sudo_password),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_) => Err("sudo 验证超时：服务器未在 1.5 秒内响应".to_string()),
                };
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
        WorkerCmd::Disconnect => Ok(true),
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
    shell_writer: &SshShellWriteHalf,
    sftp: &SharedSftpSession,
    transfer_sftp_slot: &TransferSftpSlot,
    file_access_mode: &mut String,
    sudo_user: &mut Option<String>,
    sudo_password: &mut Option<String>,
    tab_id: &str,
    app: &AppHandle,
    state: &tauri::State<'_, crate::services::workspace::WorkspaceState>,
    tunnel_commands: &mpsc::UnboundedSender<TunnelCommand>,
) -> Result<bool, String> {
    match cmd {
        WorkerCmd::WriteTerminal(data) => {
            write_shell_data(shell_writer, data.into_bytes()).await?;
            Ok(false)
        }
        WorkerCmd::ResizeTerminal { cols, rows, .. } => {
            // Resize is best-effort: a stuck SSH transport must not pin the
            // worker loop. The 16ms flush and terminal_input_rx polling
            // depend on this branch returning quickly.
            match timeout(
                TERMINAL_RESIZE_TIMEOUT,
                shell_writer.window_change(cols, rows, 0, 0),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        format!("terminal resize failed: {error}"),
                    );
                }
                Err(_) => {
                    crate::services::logging::session(
                        app,
                        "WARN",
                        "ssh",
                        tab_id,
                        "terminal resize timed out",
                    );
                }
            }
            Ok(false)
        }
        WorkerCmd::ListSshTunnels { respond_to } => {
            enqueue_tunnel_command(tunnel_commands, TunnelCommand::List { respond_to });
            Ok(false)
        }
        WorkerCmd::CreateSshTunnel { rule, respond_to } => {
            match serde_json::from_value::<SshTunnelRule>(rule) {
                Ok(rule) => enqueue_tunnel_command(
                    tunnel_commands,
                    TunnelCommand::Create { rule, respond_to },
                ),
                Err(error) => {
                    let _ = respond_to.send(Err(format!("Invalid tunnel rule: {error}")));
                }
            }
            Ok(false)
        }
        WorkerCmd::StartSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Start {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::StopSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Stop {
                    rule_id,
                    respond_to,
                },
            );
            Ok(false)
        }
        WorkerCmd::DeleteSshTunnel {
            rule_id,
            respond_to,
        } => {
            enqueue_tunnel_command(
                tunnel_commands,
                TunnelCommand::Delete {
                    rule_id,
                    respond_to,
                },
            );
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
                        Err(error) if is_sftp_not_found(&error) => Ok(None),
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
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            tokio::spawn(async move {
                let transfer_sftp =
                    acquire_transfer_sftp(&handle, &sftp, &transfer_sftp_slot, &app, &tab_id).await;
                let sftp_guard = transfer_sftp.write().await;
                let result = upload_local_file(
                    &sftp_guard,
                    &local_path,
                    &remote_path,
                    resume_offset,
                    &transfer_id,
                    cancel,
                    &app,
                )
                .await;
                drop(sftp_guard);
                if result.is_err() {
                    invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                }
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
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
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
                    )
                    .await
                } else {
                    let transfer_sftp =
                        acquire_transfer_sftp(&handle, &sftp, &transfer_sftp_slot, &app, &tab_id)
                            .await;
                    let sftp_guard = transfer_sftp.write().await;
                    let result = download_remote_file(
                        &sftp_guard,
                        &remote_path,
                        &local_path,
                        resume_offset,
                        &transfer_id,
                        cancel,
                        &app,
                    )
                    .await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
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
            let transfer_sftp_slot = Arc::clone(transfer_sftp_slot);
            let app = app.clone();
            let tab_id = tab_id.to_string();
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    replace_root_remote_file(&handle, &partial_path, &destination_path, &su, &sp)
                        .await
                } else {
                    let transfer_sftp =
                        acquire_transfer_sftp(&handle, &sftp, &transfer_sftp_slot, &app, &tab_id)
                            .await;
                    let sftp_guard = transfer_sftp.write().await;
                    let result =
                        replace_remote_file(&sftp_guard, &partial_path, &destination_path).await;
                    drop(sftp_guard);
                    if result.is_err() {
                        invalidate_transfer_sftp(&transfer_sftp, &sftp, &transfer_sftp_slot).await;
                    }
                    result
                };
                let _ = respond_to.send(result);
            });
            Ok(false)
        }
        WorkerCmd::CommitRemoteStaging {
            staging_path,
            partial_path,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let result = if fam == "root" {
                    commit_root_staging_file(&handle, &staging_path, &partial_path, &su, &sp).await
                } else {
                    Err("root staging 只能在 SSH root 文件模式下提交".to_string())
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
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(
                            &handle,
                            &format!("rm -f -- {}", shell_quote(&path)),
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()),
                        Err(_) => Err(format!("删除{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, async {
                        match sftp_guard.remove_file(&path).await {
                            Ok(()) => Ok(()),
                            Err(error) if is_sftp_not_found(&error) => Ok(()),
                            Err(error) => Err(error.to_string()),
                        }
                    })
                    .await
                    {
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
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_list_dir_via_shell(&handle, &path, &su, &sp),
                    )
                    .await
                    {
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
        WorkerCmd::ReadRemoteFile {
            path,
            encoding,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_read_file_via_shell(&handle, &path, &encoding, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        read_file(&sftp_guard, &path, &encoding),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("读取文件{}超时", path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::WriteRemoteFile {
            path,
            content,
            encoding,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_write_file_via_shell(&handle, &path, &content, &encoding, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        write_file(&sftp_guard, &path, &content, &encoding),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("写入文件{}超时", path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteDirectory {
            parent_path,
            name,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(
                            &handle,
                            &format!("mkdir -p {}", shell_quote(&full_path)),
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(FILE_OPERATION_TIMEOUT, create_dir(&sftp_guard, &full_path)).await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建目录{}超时", full_path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CreateRemoteFile {
            parent_path,
            name,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let full_path = format!("{}/{}", parent_path.trim_end_matches('/'), name);
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_write_file_via_shell(&handle, &full_path, "", "utf-8", &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        write_file(&sftp_guard, &full_path, "", "utf-8"),
                    )
                    .await
                    {
                        Ok(inner) => inner,
                        Err(_) => Err(format!("创建文件{}超时", full_path)),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::CopyRemotePath {
            target_path,
            destination_path,
            target_type,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let dest_dir = std::path::Path::new(&destination_path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "/".to_string());
                let cp_cmd = if target_type == "folder" {
                    "cp -R"
                } else {
                    "cp"
                };
                let cmd_str = format!(
                    "mkdir -p {} && {} {} {}",
                    shell_quote(&dest_dir),
                    cp_cmd,
                    shell_quote(&target_path),
                    shell_quote(&destination_path)
                );
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(&handle, &cmd_str, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                } else {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        super::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("复制超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::MoveRemotePath {
            target_path,
            destination_path,
            respond_to,
        } => {
            let handle = Arc::clone(handle);
            let sftp = Arc::clone(sftp);
            let fam = file_access_mode.clone();
            let su = sudo_user.clone();
            let sp = sudo_password.clone();
            tokio::spawn(async move {
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(
                            &handle,
                            &format!(
                                "mv {} {}",
                                shell_quote(&target_path),
                                shell_quote(&destination_path)
                            ),
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        sftp_guard.rename(&target_path, &destination_path),
                    )
                    .await
                    {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("移动超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::RenameRemotePath {
            target_path,
            new_name,
            respond_to,
        } => {
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
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(
                            &handle,
                            &format!("mv {} {}", shell_quote(&target_path), shell_quote(&dest)),
                            &su,
                            &sp,
                        ),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                } else {
                    let sftp_guard = sftp.write().await;
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        sftp_guard.rename(&target_path, &dest),
                    )
                    .await
                    {
                        Ok(inner) => inner.map_err(|e| e.to_string()),
                        Err(_) => Err("重命名超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::DeleteRemotePath {
            target_path,
            target_type,
            respond_to,
        } => {
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
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(&handle, &cmd_str, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                } else {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        super::system_metrics::exec_command(&handle, &cmd_str),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("删除超时".to_string()),
                    }
                };
                let _ = respond_to.send(res);
            });
            Ok(false)
        }
        WorkerCmd::ChangeRemotePermissions {
            target_path,
            permissions,
            recursive,
            apply_to,
            respond_to,
        } => {
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
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        "directories" => format!(
                            "chmod {} {} && find {} -type d -exec chmod {} {} +",
                            mode_str,
                            shell_quote(&target_path),
                            shell_quote(&target_path),
                            mode_str,
                            "{}"
                        ),
                        _ => format!("chmod -R {} {}", mode_str, shell_quote(&target_path)),
                    }
                };
                let res = if fam == "root" {
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        exec_shell_file_command(&handle, &cmd_str, &su, &sp),
                    )
                    .await
                    {
                        Ok(inner) => inner.map(|_| ()).map_err(|e| e.to_string()),
                        Err(_) => Err("修改权限超时".to_string()),
                    }
                } else {
                    let wrapped = format!("sh -lc {}", shell_quote(&cmd_str));
                    match timeout(
                        FILE_OPERATION_TIMEOUT,
                        super::system_metrics::exec_command(&handle, &wrapped),
                    )
                    .await
                    {
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

            if let Some(next_user) = new_sudo_user.filter(|user| !user.trim().is_empty()) {
                *sudo_user = Some(next_user);
            }
            if let Some(pwd) = new_sudo_password {
                if !pwd.is_empty() {
                    *sudo_password = Some(pwd);
                }
                // empty password ⇒ keep existing (cache reuse)
            }

            if mode == "root" {
                // 先验证 sudo 凭据，失败则回滚。`exec_shell_file_command`
                // 内部最长会等 SUDO_VERIFY_TIMEOUT（10s）才放弃，对 worker
                // 主循环来说太长——一旦 sudo 提示卡住或网络抖动，整个
                // 终端 select! 都停在这里，连 Ctrl+C 都进不去。这里用
                // ROOT_ACCESS_VERIFY_TIMEOUT（1.5s）做外层硬截断：超时
                // 同样视为验证失败并回滚，让用户拿到明确错误，而不是
                // 让 worker loop 沉默地阻塞数秒。
                let verify = match timeout(
                    ROOT_ACCESS_VERIFY_TIMEOUT,
                    exec_shell_file_command(handle, "true", sudo_user, sudo_password),
                )
                .await
                {
                    Ok(inner) => inner,
                    Err(_) => Err("sudo 验证超时：服务器未在 1.5 秒内响应".to_string()),
                };
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
        WorkerCmd::Disconnect => Ok(true),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SFTP helpers (russh-sftp 2.x)
// ─────────────────────────────────────────────────────────────────────────────

pub async fn list_dir(sftp: &SftpSession, dir_path: &str) -> Result<Vec<Value>, String> {
    let entries = sftp.read_dir(dir_path).await.map_err(|e| e.to_string())?;
    let mut items = Vec::new();
    // SFTP servers commonly omit `..` from read_dir. Keep the file pane
    // navigation consistent with Electron by creating the parent row ourselves.
    if let Some(parent_item) = parent_remote_item(dir_path) {
        items.push(parent_item);
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

fn parent_remote_item(dir_path: &str) -> Option<Value> {
    parent_remote_path(dir_path).map(|parent_path| {
        serde_json::json!({
            "name": "..",
            "path": parent_path,
            "type": "folder",
            "size": "-",
            "modified": "",
            "permission": "",
            "ownerGroup": "",
        })
    })
}

async fn read_file(sftp: &SftpSession, path: &str, encoding: &str) -> Result<String, String> {
    use tokio::io::AsyncReadExt;
    let mut f = sftp.open(path).await.map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await.map_err(|e| e.to_string())?;
    decode_bytes(&buf, encoding)
}

async fn write_file(
    sftp: &SftpSession,
    path: &str,
    content: &str,
    encoding: &str,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let bytes = encode_text(content, encoding);
    let mut f = sftp.create(path).await.map_err(|e| e.to_string())?;
    f.write_all(&bytes).await.map_err(|e| e.to_string())?;
    f.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn create_dir(sftp: &SftpSession, path: &str) -> Result<(), String> {
    match sftp.metadata(path).await {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => return Err(format!("远端路径不是目录: {path}")),
        Err(_) => {}
    }
    sftp.create_dir(path).await.map_err(|e| e.to_string())?;
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
    let metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| error.to_string())?;
    let total = metadata.len();
    if resume_offset > total {
        return Err("上传断点大于源文件".to_string());
    }
    ensure_transfer_parent_dir(sftp, remote_path).await?;
    let mut source = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| error.to_string())?;
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
    destination
        .flush()
        .await
        .map_err(|error| error.to_string())?;
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
    let metadata = sftp
        .metadata(remote_path)
        .await
        .map_err(|error| error.to_string())?;
    let total = metadata.size.unwrap_or(0);
    if resume_offset > total {
        return Err("下载断点大于源文件".to_string());
    }
    let mut source = sftp
        .open(remote_path)
        .await
        .map_err(|error| error.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    if let Some(parent) = std::path::Path::new(local_path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    if resume_offset == 0 {
        options.truncate(true);
    }
    let mut destination = options
        .open(local_path)
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
        let read = read_remote_transfer_chunk(&mut source, &mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        write_local_transfer_chunk(&mut destination, &buffer[..read], &cancel).await?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    destination
        .flush()
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn replace_remote_file(
    sftp: &SftpSession,
    partial_path: &str,
    destination_path: &str,
) -> Result<(), String> {
    let partial_metadata = sftp
        .symlink_metadata(partial_path)
        .await
        .map_err(|error| format!("无法读取远端断点文件属性: {error}"))?;
    let destination_metadata = match sftp.symlink_metadata(destination_path).await {
        Ok(metadata) => Some(metadata),
        Err(error) if is_sftp_not_found(&error) => None,
        Err(error) => return Err(format!("无法读取远端目标文件属性: {error}")),
    };

    if destination_metadata.as_ref().is_some_and(|destination| {
        destination.is_symlink()
            || matches!((destination.uid, partial_metadata.uid), (Some(left), Some(right)) if left != right)
    }) {
        let mut source = sftp
            .open(partial_path)
            .await
            .map_err(|error| format!("无法打开远端断点文件: {error}"))?;
        let mut destination = sftp
            .open_with_flags(
                destination_path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            )
            .await
            .map_err(|error| format!("无法写回远端目标文件: {error}"))?;
        tokio::io::copy(&mut source, &mut destination)
            .await
            .map_err(|error| format!("写回远端目标文件失败: {error}"))?;
        destination
            .flush()
            .await
            .map_err(|error| format!("刷新远端目标文件失败: {error}"))?;
        let committed_size = sftp
            .metadata(destination_path)
            .await
            .map_err(|error| format!("无法校验远端目标文件: {error}"))?
            .size
            .unwrap_or(0);
        if committed_size != partial_metadata.size.unwrap_or(0) {
            return Err(format!(
                "远端目标文件写回校验失败：{committed_size} bytes，期望 {}",
                partial_metadata.size.unwrap_or(0)
            ));
        }
        sftp.remove_file(partial_path)
            .await
            .map_err(|error| format!("无法清理远端断点文件: {error}"))?;
        return Ok(());
    }

    if let Some(permissions) = destination_metadata
        .as_ref()
        .and_then(|metadata| metadata.permissions)
    {
        let mut metadata = SftpMetadata::empty();
        metadata.permissions = Some(permissions);
        let _ = sftp.set_metadata(partial_path, metadata).await;
    }

    let backup_path = format!(
        "{destination_path}.fileterm-backup-{}",
        uuid::Uuid::new_v4()
    );
    let moved_destination = if destination_metadata.is_some() {
        sftp.rename(destination_path, &backup_path)
            .await
            .map_err(|error| format!("无法备份远端目标文件: {error}"))?;
        true
    } else {
        false
    };
    if let Err(error) = sftp.rename(partial_path, destination_path).await {
        if moved_destination {
            if let Err(rollback_error) = sftp.rename(&backup_path, destination_path).await {
                return Err(format!(
                    "远端文件替换失败，旧文件保留在 {backup_path}：{error}；回滚失败：{rollback_error}"
                ));
            }
        }
        return Err(format!("远端文件替换失败，断点已保留：{error}"));
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
    let Some((size, modified_at)) = output
        .trim()
        .lines()
        .next()
        .and_then(|line| line.split_once('|'))
    else {
        return Ok(None);
    };
    let size = size
        .trim()
        .parse::<u64>()
        .map_err(|_| "无法解析 root 文件大小".to_string())?;
    let modified_at = modified_at
        .trim()
        .parse::<u64>()
        .ok()
        .map(|value| value * 1000);
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

async fn commit_root_staging_file(
    handle: &Handle<ClientHandler>,
    staging_path: &str,
    partial_path: &str,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let parent = std::path::Path::new(partial_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let command = format!(
        "set -e\nmkdir -p {}\nrm -f -- {}\nmv -f -- {} {}",
        shell_quote(&parent),
        shell_quote(partial_path),
        shell_quote(staging_path),
        shell_quote(partial_path),
    );
    exec_shell_file_command(handle, &command, sudo_user, sudo_password)
        .await
        .map(|_| ())
}

#[allow(clippy::too_many_arguments)] // Root transfer context mirrors the resumable worker contract.
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
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(true);
    if resume_offset == 0 {
        options.truncate(true);
    }
    let mut local = options
        .open(local_path)
        .await
        .map_err(|error| error.to_string())?;
    local
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;

    let shell_command = if resume_offset == 0 {
        format!("cat -- {}", shell_quote(remote_path))
    } else {
        format!(
            "tail -c +{} -- {}",
            resume_offset + 1,
            shell_quote(remote_path)
        )
    };
    let user = sudo_user.as_deref().unwrap_or("root");
    let command = if sudo_password.is_some() {
        format!(
            "sudo -S -p '' -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(&shell_command)
        )
    } else {
        format!(
            "sudo -n -u {} sh -lc {}",
            shell_quote(user),
            shell_quote(&shell_command)
        )
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
                crate::services::transfers::report_progress(
                    app,
                    transfer_id,
                    transferred,
                    source.size,
                )
                .await;
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
        return Err(format!(
            "root 下载未完成（{transferred}/{} bytes）{suffix}",
            source.size
        ));
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
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            super::system_metrics::exec_command_with_stdin(handle, &full_cmd, &stdin),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(
                    "sudo 验证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string(),
                )
            }
        }
    } else {
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            super::system_metrics::exec_command(handle, &full_cmd),
        )
        .await
        {
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
        return Err("sudo 认证失败：密码错误或未授予 sudo 权限".to_string());
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
    if let Some(parent_item) = parent_remote_item(path) {
        items.push(parent_item);
    }
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
        let mtime: i64 = parts[2]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
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
    let digits = if value >= 10.0 || unit_index == 0 {
        0
    } else {
        1
    };
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
        build_http_connect_request, build_legacy_preferred, capture_sudo_password_input,
        coalesce_terminal_input, contains_interrupt_byte, default_ssh_key_paths,
        enqueue_tunnel_command, finish_shell_setup_suppression, format_sftp_unavailable_reason,
        is_password_prompt, looks_like_mfa_prompt, looks_like_root_prompt, looks_like_shell_prompt,
        missing_password_credential, parent_remote_item, parent_remote_path,
        remote_bind_host_matches, resolve_shell_file_access, resource_monitoring_enabled,
        shell_cwd_setup_for_platform, split_prompt_tail_for_setup_wait, suppress_shell_setup_echo,
        track_cwd_and_user, track_sudo_prompt_from_terminal, trim_string_front,
        try_keyboard_interactive_with_responder, tunnel_bind_address, validate_tunnel_rule,
        wait_for_ssh_stage, KeyboardInteractiveRequest, ShellSetupEchoSuppression, SshTunnelRule,
        TunnelCommand, SHELL_SETUP_SETTLE_DELAY,
    };
    #[cfg(unix)]
    use super::{forward_local_connection, forward_socks5_connection};
    use std::borrow::Cow;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use russh::keys::PrivateKey;
    use russh::{client, server};
    #[cfg(unix)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::{timeout, Duration};

    #[test]
    fn resource_monitoring_respects_explicit_profile_disable() {
        assert!(resource_monitoring_enabled(&serde_json::json!({})));
        assert!(resource_monitoring_enabled(&serde_json::json!({
            "enableResourceMonitoring": true
        })));
        assert!(!resource_monitoring_enabled(&serde_json::json!({
            "enableResourceMonitoring": false
        })));
    }

    #[test]
    fn shell_cwd_setup_reuses_linux_hook_for_darwin() {
        // Regression for M1: macOS remotes must keep CWD + sudo tracking.
        // `darwin` reuses the Linux hook; `windows` / unknown fail closed.
        assert!(shell_cwd_setup_for_platform("linux").is_some());
        assert!(shell_cwd_setup_for_platform("darwin").is_some());
        assert_eq!(
            shell_cwd_setup_for_platform("darwin"),
            shell_cwd_setup_for_platform("linux")
        );
        assert!(shell_cwd_setup_for_platform("busybox").is_some());
        assert_ne!(
            shell_cwd_setup_for_platform("busybox"),
            shell_cwd_setup_for_platform("linux"),
        );
        assert!(shell_cwd_setup_for_platform("windows").is_none());
        assert!(shell_cwd_setup_for_platform("unknown").is_none());
    }

    #[tokio::test]
    async fn dropped_tunnel_worker_rejects_queued_command() {
        let (tunnel_tx, tunnel_rx) = mpsc::unbounded_channel::<TunnelCommand>();
        drop(tunnel_rx);
        let (respond_to, response_rx) = oneshot::channel();

        enqueue_tunnel_command(&tunnel_tx, TunnelCommand::List { respond_to });

        assert_eq!(
            response_rx
                .await
                .expect("dropped tunnel worker must answer the caller")
                .expect_err("dropped tunnel worker must not report success"),
            "SSH tunnel worker stopped"
        );
    }

    #[test]
    fn trim_string_front_never_panics_on_multibyte_boundaries() {
        // 回归：热路径上的滚动 buffer 都含中文/U+FFFD（3 字节字符），
        // `s[len - keep..]` 直接切片落在字符内部会 panic 并无声杀死
        // worker/pump（终端冻结、Ctrl+C 失效）。裁剪后必须始终是合法 UTF-8。
        for fill in ["中文输出", "\u{FFFD}\u{FFFD}", "a中文b", "✓ 成功"] {
            for extra in 0..8 {
                let mut value = "x".repeat(extra) + &fill.repeat(1024);
                let original_len = value.len();
                trim_string_front(&mut value, 512);
                assert!(value.len() <= 512 || original_len <= 512);
                assert!(value.len() >= 512 - 3 || original_len <= 512);
            }
        }
        // keep 大于长度时不动；空字符串安全。
        let mut small = "abc中文".to_string();
        trim_string_front(&mut small, 1024);
        assert_eq!(small, "abc中文");
        let mut empty = String::new();
        trim_string_front(&mut empty, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn rolling_buffers_survive_cjk_flood_without_panic() {
        // 回归：模拟高吞吐中文脚本输出冲刷 track_cwd_and_user 与
        // track_sudo_prompt_from_terminal 的滚动窗口。修复前窗口裁剪
        // 落在多字节字符内部直接 panic，SSH worker 任务随之死亡。
        let flood = "[ ✓ success ] 检查点 重建分区表 running\r\n".repeat(400);
        let mut cwd_buffer = String::new();
        let mut prompt_buffer = String::new();
        let mut awaiting = false;
        let mut pending = String::new();
        let mut sudo_password = None;
        for chunk in flood.as_bytes().chunks(97) {
            let text = String::from_utf8_lossy(chunk);
            let _ = track_cwd_and_user(&text, &mut cwd_buffer);
            let _ = track_sudo_prompt_from_terminal(
                &text,
                &mut prompt_buffer,
                &mut awaiting,
                &mut pending,
                &mut sudo_password,
            );
        }
        assert!(cwd_buffer.len() < 16384);
        assert!(prompt_buffer.len() < 4096);
    }

    #[tokio::test]
    async fn ssh_stage_timeout_is_reported_without_waiting_for_the_client_default() {
        let error = wait_for_ssh_stage(
            "SSH password authentication",
            Duration::from_millis(1),
            std::future::pending::<Result<(), String>>(),
        )
        .await
        .unwrap_err();

        assert_eq!(error, "SSH password authentication timed out after 1 ms");
    }

    #[test]
    fn password_auth_requests_missing_credentials_without_falling_back_to_keys() {
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "username": "ops"
            })),
            Some("missing-password")
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "password": "secret"
            })),
            Some("missing-username")
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "username": "ops",
                "password": "secret"
            })),
            None
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "system",
                "username": "ops"
            })),
            None
        );
    }

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

        let root =
            std::env::temp_dir().join(format!("fileterm-tauri-sshd-{}", uuid::Uuid::new_v4()));
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
            assert_eq!(
                count, 1,
                "proxy client closed before completing CONNECT headers"
            );
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
        let visible = suppress_shell_setup_echo(&mut pending, "root@host:~# ");

        assert_eq!(visible, "Debian GNU/Linux\r\nuser@host:~$ root@host:~# ");
        assert!(pending.is_none());
    }

    #[test]
    fn detects_common_posix_prompts_after_terminal_colours_are_removed() {
        assert!(looks_like_shell_prompt(
            "\u{1b}[01;32mStoffel@fnOSNAS-CN\u{1b}[0m:\u{1b}[01;34m/\u{1b}[0m$ "
        ));
        assert!(looks_like_shell_prompt("root@host:~# "));
        assert!(looks_like_shell_prompt("host% "));
        assert!(!looks_like_shell_prompt("Last login: today\r\n"));
    }

    #[test]
    fn detects_fragmented_fn_os_prompt_and_cwd_marker() {
        let prompt = concat!(
            "Linux fnOSNAS-CN 6.18.18-trim\r\n",
            "Stoffel@fnOSNAS-CN:",
            "/$ "
        );
        assert!(looks_like_shell_prompt(prompt));

        let mut buffer = String::new();
        assert_eq!(
            track_cwd_and_user("\u{1b}]7;file:///e", &mut buffer),
            (None, None)
        );
        assert_eq!(
            track_cwd_and_user("tc\u{7}\u{1b}]1337;RemoteUser=Stoffel\u{7}", &mut buffer),
            (Some("/etc".to_string()), Some("Stoffel".to_string()))
        );
    }

    #[test]
    fn detects_root_prompt_after_terminal_colours_are_removed() {
        assert!(looks_like_root_prompt("\u{1b}[01;31mroot@host\u{1b}[0m:# "));
        assert!(!looks_like_root_prompt("user@host:$ "));
    }

    #[test]
    fn suppress_releases_new_prompt_after_marker_on_slow_device() {
        // 慢设备（群晖）：OSC marker 后新 prompt 在 settle delay 之后才到达。
        // 第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存（不 forward），
        // 所以 suppress 释放时只返回新 prompt（最后一个换行符之后的部分），
        // 吞掉 setup echo 和 OSC marker。用户最终看到一个完整 prompt。
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        // 喂入 setup echo + OSC marker，suppress 仍在等待新 prompt
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " __tdcwd(){ printf '\\033]7;file:///home/u\\007';};__tdcwd\r\n\u{1b}]7;file:///home/u\u{7}"
            ),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_some());
        // 新 prompt 到达（无论 settle delay 是否到期）：只释放新 prompt
        let visible = suppress_shell_setup_echo(&mut pending, "user@host:~$ ");
        assert_eq!(visible, "user@host:~$ ");
        assert!(pending.is_none());
    }

    #[test]
    fn finish_suppression_releases_newline_when_prompt_never_arrives() {
        // marker 已看到但新 prompt 迟迟未到（settle/timeout 到期）：
        // 补换行让晚到的新 prompt 从新行开始，避免粘在旧 prompt 后面。
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        // 喂入 setup echo + OSC marker，但新 prompt 一直没来
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " __tdcwd(){ printf '\\033]7;file:///home/u\\007';};__tdcwd\r\n\u{1b}]7;file:///home/u\u{7}"
            ),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_some());
        // 超时释放时 buffer 末尾不是 prompt，补换行
        let visible = finish_shell_setup_suppression(&mut pending);
        assert_eq!(visible, "\r\n");
        assert!(pending.is_none());
    }

    #[test]
    fn finish_suppression_no_newline_when_marker_never_seen() {
        // setup 执行失败（没检测到 OSC marker）时不补换行，避免多余的空行
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, " __tdcwd(){ broken syntax"),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_none());
        // 超时释放时不补换行
        let visible = finish_shell_setup_suppression(&mut pending);
        assert_eq!(visible, "");
        assert!(pending.is_none());
    }

    #[test]
    fn split_prompt_tail_separates_banner_from_prompt() {
        // banner + prompt 在同一 chunk：banner forward，prompt 暂存
        let (banner, tail) =
            split_prompt_tail_for_setup_wait("Welcome to Synology\r\nStoffel@SynologyNAS-MY:~$ ");
        assert_eq!(banner, "Welcome to Synology\r\n");
        assert_eq!(tail, "Stoffel@SynologyNAS-MY:~$ ");
    }

    #[test]
    fn split_prompt_tail_keeps_colored_prompt_escape_in_tail() {
        // 彩色 prompt 的 escape 序列划入 tail（不 forward），banner 部分保留原始 escape
        let (banner, tail) = split_prompt_tail_for_setup_wait(
            "\u{1b}[01;32mStoffel@SynologyNAS-MY\u{1b}[0m:\u{1b}[01;34m~\u{1b}[0m$ ",
        );
        assert_eq!(banner, "");
        assert_eq!(
            tail,
            "\u{1b}[01;32mStoffel@SynologyNAS-MY\u{1b}[0m:\u{1b}[01;34m~\u{1b}[0m$ "
        );
    }

    #[test]
    fn split_prompt_tail_returns_whole_chunk_when_no_prompt() {
        // 纯 banner（无 prompt 结尾符）：整个 chunk forward
        let (banner, tail) = split_prompt_tail_for_setup_wait(
            "Using terminal commands to modify system configs\r\n",
        );
        assert_eq!(
            banner,
            "Using terminal commands to modify system configs\r\n"
        );
        assert_eq!(tail, "");
    }

    #[test]
    fn split_prompt_tail_stops_at_newline_when_scanning_backwards() {
        // prompt 结尾符不在最后一行（最后一行是 banner 续行）：整个 chunk forward
        let (banner, tail) = split_prompt_tail_for_setup_wait("some $ var\r\nbanner continuation");
        assert_eq!(banner, "some $ var\r\nbanner continuation");
        assert_eq!(tail, "");
    }

    #[test]
    fn shell_identity_controls_file_access_independently_of_cached_sudo_auth() {
        assert_eq!(
            resolve_shell_file_access("stoffel", "root"),
            ("root", Some("root".to_string()))
        );
        assert_eq!(
            resolve_shell_file_access("stoffel", "postgres"),
            ("root", Some("postgres".to_string()))
        );
        assert_eq!(
            resolve_shell_file_access("stoffel", "stoffel"),
            ("user", None)
        );
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
    fn coalesces_high_frequency_terminal_input_without_losing_order() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        sender.send("clear\r".to_string()).unwrap();
        for _ in 0..2_000 {
            sender.send("\r".to_string()).unwrap();
        }

        let first = receiver.try_recv().unwrap();
        let merged = coalesce_terminal_input(first, &mut receiver);
        assert!(merged.starts_with("clear\r"));
        assert_eq!(merged.matches('\r').count(), 2_001);
        assert!(receiver.is_empty());
    }

    #[test]
    fn detects_ctrl_c_without_matching_other_control_bytes() {
        assert!(contains_interrupt_byte("build\r\u{3}"));
        assert!(!contains_interrupt_byte("build\r"));
        assert!(!contains_interrupt_byte("\u{1b}[2J"));
    }

    #[test]
    fn creates_parent_rows_only_below_remote_root() {
        assert_eq!(parent_remote_path("/"), None);
        assert_eq!(parent_remote_path("/home"), Some("/".to_string()));
        assert_eq!(
            parent_remote_path("/home/stoffel/下载/"),
            Some("/home/stoffel".to_string())
        );
        assert!(parent_remote_item("/").is_none());
        assert_eq!(parent_remote_item("/root").unwrap()["path"], "/");
        assert_eq!(parent_remote_item("/root").unwrap()["name"], "..");
    }

    #[test]
    fn default_ssh_key_candidates_match_electron_precedence() {
        let home = Path::new("/home/fileterm");
        assert_eq!(
            default_ssh_key_paths(home),
            vec![
                home.join(".ssh/id_ed25519"),
                home.join(".ssh/id_ecdsa"),
                home.join(".ssh/id_rsa"),
                home.join(".ssh/id_dsa"),
            ]
        );
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
        let mut server_config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        server_config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
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
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].prompts.len(), 1);
            assert_eq!(requests[0].prompts[0].prompt, "OTP code: ");
        }
        assert_eq!(
            responses.lock().unwrap().as_slice(),
            ["saved-password", "246810"]
        );

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

        let command =
            crate::sessions::system_metrics::exec_command(&handle, "printf 'tauri-openssh-exec'")
                .await
                .unwrap();
        assert_eq!(command, "tauri-openssh-exec");

        let platform = crate::sessions::system_metrics::probe_remote_platform(&handle).await;
        #[cfg(target_os = "linux")]
        assert_eq!(platform, "linux");
        #[cfg(target_os = "macos")]
        assert_eq!(
            platform, "darwin",
            "macOS remotes must be detected as `darwin` so CWD tracking stays active"
        );

        let channel = handle.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .unwrap();
        let remote_file = fixture.remote_dir.join("tauri-sftp.txt");
        let remote_file = remote_file.to_string_lossy().into_owned();
        sftp.create(&remote_file).await.unwrap();
        sftp.write(&remote_file, b"tauri-openssh-sftp")
            .await
            .unwrap();
        assert_eq!(
            sftp.read(&remote_file).await.unwrap(),
            b"tauri-openssh-sftp"
        );
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
        timeout(Duration::from_secs(2), target)
            .await
            .unwrap()
            .unwrap();
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
        let mut dynamic_client = tokio::net::TcpStream::connect(dynamic_address)
            .await
            .unwrap();
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
        dynamic_client
            .read_exact(&mut dynamic_response)
            .await
            .unwrap();
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
            assert!(request.starts_with(&format!("CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\n")));
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

        let invalid = SshTunnelRule {
            target_port: None,
            ..valid
        };
        assert!(validate_tunnel_rule(&invalid).is_err());
    }

    #[test]
    fn remote_forward_matches_exact_and_wildcard_bind_hosts() {
        assert!(remote_bind_host_matches("127.0.0.1", "127.0.0.1"));
        assert!(!remote_bind_host_matches("127.0.0.1", "10.0.0.4"));
        assert!(remote_bind_host_matches("0.0.0.0", "10.0.0.4"));
        assert!(remote_bind_host_matches("::", "2001:db8::4"));
    }

    #[test]
    fn legacy_preferred_appends_sha1_algorithms_after_sha2() {
        use russh::{kex, mac};

        let preferred = build_legacy_preferred();

        // SHA-2 类 MAC 应在 SHA-1 之前（保持 SHA-2 优先）
        let sha256_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA256)
            .expect("SHA-256 MAC must remain in legacy list");
        let sha1_etm_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA1_ETM)
            .expect("SHA-1 ETM MAC must be appended for legacy servers");
        let sha1_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA1)
            .expect("SHA-1 MAC must be appended for legacy servers");
        assert!(sha256_pos < sha1_etm_pos);
        assert!(sha1_etm_pos < sha1_pos);

        // SHA-2 类 KEX（DH_G14_SHA256）应在 SHA-1 类（DH_G14_SHA1）之前
        let sha256_kex_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G14_SHA256)
            .expect("SHA-256 KEX must remain in legacy list");
        let sha1_kex_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G14_SHA1)
            .expect("SHA-1 KEX must be appended for legacy servers");
        let g1_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G1_SHA1)
            .expect("DH-G1-SHA1 must be appended for very old servers");
        assert!(sha256_kex_pos < sha1_kex_pos);
        assert!(sha1_kex_pos < g1_pos);
    }
}
