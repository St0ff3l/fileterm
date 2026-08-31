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
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, Mutex as StdMutex,
};
use std::time::{Duration, Instant};

use base64::Engine;
use russh::client::{Handle, Handler};
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{
    AuthResult, Channel, ChannelMsg, ChannelWriteHalf, Disconnect, MethodKind, MethodSet, Sig,
};
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::Metadata as SftpMetadata;
use russh_sftp::client::{Config as SftpConfig, SftpSession};
use russh_sftp::protocol::{OpenFlags, StatusCode};
use serde_json::Value;
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use tokio::io::{
    copy_bidirectional, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tokio::time::{sleep, timeout};
use tokio_socks::tcp::Socks5Stream;
use tokio_util::sync::CancellationToken;

use crate::sessions::{
    reconnect::{port_from_profile, seconds_from_profile, KeepalivePolicy, ReconnectPolicy},
    TransferFileStat, WorkerCmd,
};
use crate::services::{
    transfers::is_root_upload_staging_path,
    workspace::{ConnectionCapabilities, RemoteDiskSpace, RemoteFileCapabilities},
    WorkspaceTabStatus,
};

const DEFAULT_SSH_KEY_FILES: [&str; 4] = ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"];
const SSH_INTERACTION_TIMEOUT: Duration = Duration::from_secs(300);
// Connection tests are transient and have no workspace session to keep alive.
// If a first-time host-key prompt cannot be observed by the form, release the
// SSH handshake promptly instead of occupying a server-side unauthenticated
// slot for the normal five-minute interaction window.
const SSH_CONNECTION_TEST_INTERACTION_TIMEOUT: Duration = Duration::from_secs(30);
// A TCP connection, SSH protocol handshake, or password-auth reply can remain
// pending indefinitely on a broken server or middlebox. Keep each startup
// stage bounded so the workspace moves out of `connecting` and the user can
// retry instead of seeing a permanently reconnecting terminal.
const SSH_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(30);
const SSH_PASSWORD_AUTH_TIMEOUT: Duration = Duration::from_secs(30);
/// A server may send several keyboard-interactive challenges (for example a
/// password prompt followed by an OTP prompt). Bound the number of visible
/// rounds so a broken PAM module cannot keep a connection attempt alive
/// forever.
const MAX_KEYBOARD_INTERACTIVE_ROUNDS: usize = 16;
/// RFC 4252 allows a method to complete with partial success and ask the
/// client to begin keyboard-interactive again for the next factor.
const MAX_KEYBOARD_INTERACTIVE_RESTARTS: usize = 8;
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
/// Capability metadata is optional and must not occupy the single SFTP
/// request stream long enough to delay a user's first file action.
const INITIAL_CAPABILITY_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

type SshShellWriteHalf = ChannelWriteHalf<russh::client::Msg>;
type SharedRemoteSshId = Arc<StdMutex<Option<Vec<u8>>>>;

struct OpenSshSession {
    handle: Handle<ClientHandler>,
    remote_sshid: Vec<u8>,
}

/// Which endpoint in an SSH connection flow owns an authentication prompt.
/// A jump host and the final target are independent SSH sessions, even though
/// the latter transport travels through the former's direct-tcpip channel.
#[derive(Clone, Copy)]
enum SshAuthenticationTarget {
    Direct,
    JumpHost,
    Target,
}

impl SshAuthenticationTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::JumpHost => "jump-host",
            Self::Target => "target",
        }
    }
}

fn read_shared_remote_sshid(remote_sshid: &SharedRemoteSshId) -> Vec<u8> {
    remote_sshid
        .lock()
        .ok()
        .and_then(|value| value.clone())
        .unwrap_or_default()
}

fn default_sftp_capabilities() -> RemoteFileCapabilities {
    RemoteFileCapabilities {
        protocol: "sftp".to_string(),
        protocol_version: Some("3".to_string()),
        extensions: Vec::new(),
        // SFTP v3 has no portable server-side checksum request. Keep this
        // empty rather than claiming a shell-specific hash command is a
        // protocol capability.
        checksum_algorithms: Vec::new(),
        disk_space: None,
        server_copy: false,
        symlink: true,
        hardlink: false,
    }
}

async fn inspect_sftp_capabilities(sftp: &SftpSession, path: &str) -> RemoteFileCapabilities {
    let mut capabilities = default_sftp_capabilities();
    capabilities.protocol_version = Some("3".to_string());
    match sftp.fs_info(path.to_string()).await {
        Ok(Some(info)) => {
            let block_size = info.fragment_size.max(1);
            capabilities.disk_space = Some(RemoteDiskSpace {
                available_bytes: info.blocks_avail.saturating_mul(block_size),
                total_bytes: info.blocks.saturating_mul(block_size),
            });
            capabilities
                .extensions
                .push("statvfs@openssh.com".to_string());
        }
        Ok(None) => {}
        Err(_) => {}
    }
    let probe_id = uuid::Uuid::new_v4();
    let base = path.trim_end_matches('/');
    let base = if base.is_empty() { "/" } else { base };
    let symlink_probe = if base == "/" {
        format!("/.fileterm-symlink-capability-{probe_id}")
    } else {
        format!("{base}/.fileterm-symlink-capability-{probe_id}")
    };
    // READLINK on a deliberately absent path is non-mutating. A normal
    // no-such-file response proves that the request is understood, while
    // an explicit unsupported/unknown-request response does not.
    capabilities.symlink = match sftp.read_link(&symlink_probe).await {
        Ok(_) => true,
        Err(error) => !sftp_operation_rejected(&error),
    };

    let hardlink_source = if base == "/" {
        format!("/.fileterm-hardlink-source-{probe_id}")
    } else {
        format!("{base}/.fileterm-hardlink-source-{probe_id}")
    };
    let hardlink_destination = if base == "/" {
        format!("/.fileterm-hardlink-destination-{probe_id}")
    } else {
        format!("{base}/.fileterm-hardlink-destination-{probe_id}")
    };
    // russh-sftp returns Ok(false) when hardlink@openssh.com was not
    // advertised. If the extension is present, an absent source normally
    // returns a server error; that still proves the operation is supported.
    capabilities.hardlink = match sftp.hardlink(&hardlink_source, &hardlink_destination).await {
        Ok(created) => {
            if created {
                let _ = sftp.remove_file(&hardlink_destination).await;
            }
            created
        }
        Err(error) => !sftp_operation_rejected(&error),
    };
    if capabilities.hardlink {
        capabilities
            .extensions
            .push("hardlink@openssh.com".to_string());
    }
    capabilities
}

fn sftp_operation_rejected(error: &SftpError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "unsupported",
        "not supported",
        "not implemented",
        "unknown request",
        "operation unavailable",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

async fn inspect_ssh_exec_capabilities(
    handle: &Handle<ClientHandler>,
    operation_timeout: Duration,
) -> (bool, Vec<String>) {
    let probe = "if command -v cp >/dev/null 2>&1; then printf '__FILETERM_CP__\\n'; fi; if command -v sha256sum >/dev/null 2>&1; then printf '__FILETERM_SHA256SUM__\\n'; fi; if command -v shasum >/dev/null 2>&1; then printf '__FILETERM_SHASUM__\\n'; fi";
    let Ok(Ok((output, _))) = timeout(
        operation_timeout,
        crate::sessions::system_metrics::exec_command_with_status(handle, probe),
    )
    .await
    else {
        return (false, Vec::new());
    };
    let mut checksum_algorithms = Vec::new();
    if output.contains("__FILETERM_SHA256SUM__") || output.contains("__FILETERM_SHASUM__") {
        checksum_algorithms.push("SHA-256 (SSH exec)".to_string());
    }
    (output.contains("__FILETERM_CP__"), checksum_algorithms)
}

#[allow(clippy::too_many_arguments)] // Root-mode verification needs the same explicit credential context as the transfer.
async fn verify_sftp_transfer_sha256(
    handle: &Handle<ClientHandler>,
    local_path: &str,
    remote_path: &str,
    file_access_mode: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
    operation_timeout: Duration,
) -> Result<(), String> {
    let local_hash = crate::sessions::file_integrity::sha256_file(local_path).await?;
    let command = format!(
        "if command -v sha256sum >/dev/null 2>&1; then sha256sum -- {}; elif command -v shasum >/dev/null 2>&1; then shasum -a 256 -- {}; else printf '__FILETERM_SHA256_UNAVAILABLE__\\n'; fi",
        shell_quote(remote_path),
        shell_quote(remote_path),
    );
    let output = if file_access_mode == "root" {
        match timeout(
            operation_timeout,
            exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err("远端 SHA-256 校验超时".to_string()),
        }
    } else {
        match timeout(
            operation_timeout,
            crate::sessions::system_metrics::exec_command_with_status(handle, &command),
        )
        .await
        {
            Ok(Ok((output, _))) => output,
            Ok(Err(error)) => return Err(format!("远端 SHA-256 校验失败: {error}")),
            Err(_) => return Err("远端 SHA-256 校验超时".to_string()),
        }
    };
    if output.contains("__FILETERM_SHA256_UNAVAILABLE__") {
        return Ok(());
    }
    let Some(remote_hash) = crate::sessions::file_integrity::parse_sha256_output(&output) else {
        return Err("远端未返回可识别的 SHA-256 校验和".to_string());
    };
    if remote_hash != local_hash {
        return Err(format!(
            "SFTP 传输校验失败：本地 {local_hash}，远端 {remote_hash}"
        ));
    }
    Ok(())
}

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

/// Keep user keystrokes away from the interactive line editor while the
/// internal shell hook is being installed. The shell echoes input through the
/// same PTY, so prompt detection cannot safely distinguish a literal `#` typed
/// by the user from a root prompt. Ctrl+C remains an emergency escape hatch:
/// it must reach the remote shell immediately so a stuck setup can be
/// cancelled.
fn should_buffer_terminal_input_during_shell_setup(
    waiting_for_initial_prompt: bool,
    setup_echo_pending: bool,
    data: &str,
) -> bool {
    (waiting_for_initial_prompt || setup_echo_pending) && !contains_interrupt_byte(data)
}

fn flush_deferred_terminal_input(
    pending: &mut Vec<Vec<u8>>,
    terminal_write_tx: &mpsc::UnboundedSender<Vec<u8>>,
) -> Result<(), String> {
    for data in pending.drain(..) {
        terminal_write_tx
            .send(data)
            .map_err(|_| "Terminal writer stopped".to_string())?;
    }
    Ok(())
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
    let timeout_label = timeout_label(deadline);
    timeout(deadline, operation)
        .await
        .map_err(|_| format!("{stage} timed out after {timeout_label}"))?
}

fn timeout_label(deadline: Duration) -> String {
    if deadline.as_secs() > 0 {
        format!("{} seconds", deadline.as_secs())
    } else {
        format!("{} ms", deadline.as_millis())
    }
}

/// A host-key confirmation is part of the SSH handshake, but it is an
/// intentional user decision rather than stalled network I/O. Pause the
/// normal handshake budget while that explicit prompt is visible, while still
/// bounding transport work and user interaction independently.
async fn wait_for_ssh_handshake_with_network_timeout<T>(
    stage: &str,
    host_verification_waiting: Arc<AtomicBool>,
    network_timeout: Duration,
    interaction_timeout: Duration,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    wait_for_ssh_handshake_with_timeouts(
        stage,
        host_verification_waiting,
        network_timeout,
        interaction_timeout,
        operation,
    )
    .await
}

async fn wait_for_ssh_handshake_with_timeouts<T>(
    stage: &str,
    host_verification_waiting: Arc<AtomicBool>,
    network_timeout: Duration,
    interaction_timeout: Duration,
    operation: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let mut network_elapsed = Duration::ZERO;
    let mut verification_elapsed = Duration::ZERO;
    let mut last_tick = Instant::now();
    tokio::pin!(operation);

    loop {
        tokio::select! {
            result = &mut operation => return result,
            _ = sleep(Duration::from_millis(100)) => {
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last_tick);
                last_tick = now;

                if host_verification_waiting.load(Ordering::Acquire) {
                    verification_elapsed += elapsed;
                    if verification_elapsed >= interaction_timeout {
                        return Err(format!(
                            "SSH host-key verification timed out after {}",
                            timeout_label(interaction_timeout)
                        ));
                    }
                } else {
                    network_elapsed += elapsed;
                    if network_elapsed >= network_timeout {
                        return Err(format!("{stage} timed out after {}", timeout_label(network_timeout)));
                    }
                }
            }
        }
    }
}

fn resource_monitoring_enabled(profile: &Value) -> bool {
    profile
        .get("enableResourceMonitoring")
        .and_then(Value::as_bool)
        != Some(false)
}

fn exec_channel_enabled(profile: &Value) -> bool {
    profile.get("enableExecChannel").and_then(Value::as_bool) != Some(false)
}
