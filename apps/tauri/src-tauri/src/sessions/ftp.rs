use base64::Engine;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use sha2::{Digest, Sha256};
use suppaftp::list::{File as ListedFile, ListParser};
use suppaftp::tokio::{
    AsyncFtpStream, AsyncNativeTlsConnector, AsyncNativeTlsFtpStream, ImplAsyncFtpStream,
    TokioTlsStream,
};
use suppaftp::types::Mode;
use suppaftp::{FtpError, Status};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration, MissedTickBehavior};
use tokio_socks::tcp::Socks5Stream;
use tokio_util::sync::CancellationToken;

use super::terminal::{decode_terminal, encode_terminal};
use super::{
    reconnect::{port_from_profile, seconds_from_profile, KeepalivePolicy, ReconnectPolicy},
    TransferFileStat, WorkerCmd,
};
use crate::services::{workspace::RemoteFileCapabilities, WorkspaceTabStatus};

const TRANSFER_CANCELED: &str = "transfer canceled";
const DEFAULT_FTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_FTP_OPERATION_TIMEOUT: Duration = Duration::from_secs(60);
const FTP_PROXY_IO_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_FTP_DELETE_DEPTH: usize = 64;
const MAX_FTP_DELETE_ENTRIES: usize = 100_000;

enum FtpClient {
    Plain(AsyncFtpStream),
    Secure(AsyncNativeTlsFtpStream),
}

#[derive(Default)]
struct FtpListingState {
    mlsd_disabled: bool,
    mlst_disabled: bool,
    size_disabled: bool,
    resolved_types: HashMap<String, bool>,
    resolved_sizes: HashMap<String, usize>,
}

struct ParsedFtpListing {
    entry: ListedFile,
    type_is_trusted: bool,
}

fn default_ftp_capabilities() -> RemoteFileCapabilities {
    RemoteFileCapabilities {
        protocol: "ftp".to_string(),
        protocol_version: None,
        extensions: Vec::new(),
        checksum_algorithms: Vec::new(),
        disk_space: None,
        server_copy: false,
        symlink: false,
        hardlink: false,
    }
}

fn ftp_error_requires_reconnect(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "timed out",
        "connection reset",
        "connection closed",
        "broken pipe",
        "unexpected eof",
        "failed to fill whole buffer",
    ]
    .iter()
    .any(|needle| error.contains(needle))
}

macro_rules! respond_ftp_result {
    ($respond_to:expr, $result:expr) => {{
        let result = $result;
        let reconnect_error = result
            .as_ref()
            .err()
            .filter(|error| ftp_error_requires_reconnect(error))
            .cloned();
        let _ = $respond_to.send(result);
        if let Some(error) = reconnect_error {
            return Err(format!(
                "FTP control connection is no longer usable: {error}"
            ));
        }
    }};
}

pub fn start_ftp_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
    cancellation: CancellationToken,
) {
    crate::services::logging::session(&app, "INFO", "ftp", &tab_id, "worker starting");
    tauri::async_runtime::spawn(async move {
        let reconnect_mode = profile
            .get("reconnectMode")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let reconnect_policy = ReconnectPolicy::from_profile(&profile);
        let mut reconnect_attempt = 0;
        let mut command_rx = command_rx;
        loop {
            let result = {
                let run = run_ftp_worker(
                    &tab_id,
                    &profile,
                    &mut command_rx,
                    &app,
                    &mut reconnect_attempt,
                );
                tokio::select! {
                    result = run => result,
                    _ = cancellation.cancelled() => return,
                }
            };
            match result {
                Ok(()) => return,
                Err(error) if reconnect_mode == "auto" => {
                    let Some(attempt) = reconnect_policy.next_attempt(reconnect_attempt) else {
                        crate::services::logging::session(
                            &app,
                            "ERROR",
                            "ftp",
                            &tab_id,
                            format!("auto-reconnect limit reached: {error}"),
                        );
                        set_ftp_state(
                            &app,
                            &tab_id,
                            format!("FTP reconnect limit reached: {error}"),
                            WorkspaceTabStatus::Error,
                            None,
                            None,
                        )
                        .await;
                        return;
                    };
                    reconnect_attempt = attempt;
                    let delay = reconnect_policy.delay_for_attempt(attempt);
                    crate::services::logging::session(
                        &app,
                        "WARN",
                        "ftp",
                        &tab_id,
                        format!(
                            "auto-reconnect scheduled attempt={attempt} delay_ms={}",
                            delay.as_millis()
                        ),
                    );
                    set_ftp_state(
                        &app,
                        &tab_id,
                        format!("FTP reconnecting (attempt {attempt})"),
                        WorkspaceTabStatus::Connecting,
                        None,
                        None,
                    )
                    .await;
                    tokio::select! {
                        _ = sleep(delay) => {}
                        _ = cancellation.cancelled() => return,
                    }
                }
                Err(error) => {
                    crate::services::logging::session(&app, "ERROR", "ftp", &tab_id, &error);
                    set_ftp_state(
                        &app,
                        &tab_id,
                        format!("FTP error: {error}"),
                        WorkspaceTabStatus::Error,
                        None,
                        None,
                    )
                    .await;
                    return;
                }
            }
        }
    });
}

async fn run_ftp_worker(
    tab_id: &str,
    profile: &Value,
    command_rx: &mut mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
    reconnect_attempt: &mut u32,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "FTP host is required".to_string())?;
    let port = port_from_profile(profile, 21, "FTP")?;
    let remote_path = profile
        .get("remotePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("/")
        .to_string();
    let mut client = connect_ftp(profile, host, port).await?;
    *reconnect_attempt = 0;
    let mut listing_state = FtpListingState::default();
    let initial_files = ftp_with_timeout(
        profile,
        "list",
        client_list(&mut client, &remote_path, &mut listing_state),
    )
    .await?;
    crate::services::logging::session(
        app,
        "INFO",
        "ftp",
        tab_id,
        format!(
            "connected host={host} port={port} entries={}",
            initial_files.len()
        ),
    );
    set_ftp_state(
        app,
        tab_id,
        format!("FTP {}:{}", host, port),
        WorkspaceTabStatus::Connected,
        Some(remote_path.clone()),
        Some(initial_files),
    )
    .await;
    let capabilities =
        match ftp_with_timeout(profile, "features", client_features(&mut client)).await {
            Ok(features) => ftp_capabilities_from_features(features),
            Err(error) if ftp_error_requires_reconnect(&error) => return Err(error),
            Err(_) => default_ftp_capabilities(),
        };
    set_ftp_capabilities(app, tab_id, capabilities).await;
    let mut transfer_jobs = tokio::task::JoinSet::new();
    let cleanup_app = app.clone();
    let cleanup_tab_id = tab_id.to_string();
    tokio::spawn(async move {
        if let Err(error) =
            crate::services::transfers::retry_pending_cleanup_for_tab(&cleanup_app, &cleanup_tab_id)
                .await
        {
            crate::services::logging::warn(
                &cleanup_app,
                &format!("transfer:{cleanup_tab_id}"),
                format!("pending cleanup retry failed: {error}"),
            );
        }
    });
    let keepalive = KeepalivePolicy::from_profile(profile);
    let mut keepalive_tick =
        tokio::time::interval(keepalive.interval.unwrap_or(Duration::from_secs(86400)));
    keepalive_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    keepalive_tick.tick().await;
    let mut keepalive_misses = 0_usize;

    loop {
        while transfer_jobs.try_join_next().is_some() {}
        let command = tokio::select! {
            command = command_rx.recv() => command,
            _ = keepalive_tick.tick(), if keepalive.interval.is_some() => {
                if keepalive_misses >= keepalive.max_misses {
                    return Err(format!("FTP keepalive failed after {} attempts", keepalive.max_misses));
                }
                match ftp_with_timeout(profile, "keepalive", client_noop(&mut client)).await {
                    Ok(()) => keepalive_misses = 0,
                    Err(error) => {
                        if ftp_error_requires_reconnect(&error) {
                            return Err(error);
                        }
                        keepalive_misses += 1;
                        crate::services::logging::session(
                            app,
                            "WARN",
                            "ftp",
                            tab_id,
                            format!("keepalive failed misses={keepalive_misses}: {error}"),
                        );
                    }
                }
                continue;
            }
        };
        match command {
            Some(WorkerCmd::ListRemoteFiles {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "list",
                    cancellation,
                    client_list(&mut client, &path, &mut listing_state),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ReadRemoteFile {
                path,
                encoding,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "read",
                    cancellation,
                    client_read(&mut client, &path, &encoding),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::WriteRemoteFile {
                path,
                content,
                encoding,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "write",
                    cancellation,
                    client_write(&mut client, &path, &content, &encoding),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CreateRemoteDirectory {
                parent_path,
                name,
                cancellation,
                respond_to,
                ..
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let result = ftp_with_cancellation(
                    profile,
                    "mkdir",
                    cancellation,
                    client_ensure_dir(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CreateRemoteFile {
                parent_path,
                name,
                cancellation,
                respond_to,
                ..
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let result = ftp_with_cancellation(
                    profile,
                    "create file",
                    cancellation,
                    client_write(&mut client, &path, "", "utf-8"),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CopyRemotePath { respond_to, .. }) => {
                let _ =
                    respond_to.send(Err("FTP 不支持服务器内复制，请改用下载后上传".to_string()));
            }
            Some(WorkerCmd::MoveRemotePath {
                target_path,
                destination_path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "rename",
                    cancellation,
                    client_rename(&mut client, &target_path, &destination_path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::RenameRemotePath {
                target_path,
                new_name,
                cancellation,
                respond_to,
                ..
            }) => {
                let destination = join_remote_path(&parent_remote_path(&target_path), &new_name);
                let result = ftp_with_cancellation(
                    profile,
                    "rename",
                    cancellation,
                    client_rename(&mut client, &target_path, &destination),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::DeleteRemotePath {
                target_path,
                target_type,
                target_is_symlink,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "delete",
                    cancellation,
                    client_delete(&mut client, &target_path, &target_type, target_is_symlink),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ChangeRemotePermissions {
                target_path,
                permissions,
                recursive,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = if recursive {
                    Err("FTP 暂不支持递归修改权限".to_string())
                } else {
                    ftp_with_cancellation(
                        profile,
                        "chmod",
                        cancellation,
                        client_chmod(&mut client, &target_path, permissions),
                    )
                    .await
                };
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::SetRemoteFileAccessMode {
                mode, respond_to, ..
            }) => {
                let result = if mode == "root" {
                    Err("FTP 不支持 SSH root 文件模式".to_string())
                } else {
                    Ok(())
                };
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::StatRemoteFile {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "stat",
                    cancellation,
                    client_stat(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::UploadLocalFile {
                local_path,
                remote_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let profile = profile.clone();
                let host = host.to_string();
                let app = app.clone();
                let tab_id = tab_id.to_string();
                transfer_jobs.spawn(async move {
                    let result = async {
                        let mut transfer_client = connect_ftp(&profile, &host, port).await?;
                        crate::services::logging::session(
                            &app,
                            "INFO",
                            "ftp",
                            &tab_id,
                            format!("dedicated upload connection opened transfer={transfer_id}"),
                        );
                        let transfer_timeout = seconds_from_profile(
                            &profile,
                            "operationTimeoutSeconds",
                            DEFAULT_FTP_OPERATION_TIMEOUT,
                            Duration::from_secs(5),
                            Duration::from_secs(3600),
                        );
                        let transfer_cancel = cancel.clone();
                        let mut result = client_upload(
                            &mut transfer_client,
                            &local_path,
                            &remote_path,
                            resume_offset,
                            &transfer_id,
                            cancel,
                            &app,
                            transfer_timeout,
                        )
                        .await;
                        if result.is_ok() && !transfer_cancel.is_cancelled() {
                            result = verify_ftp_transfer_checksum(
                                &mut transfer_client,
                                &local_path,
                                &remote_path,
                                transfer_timeout,
                            )
                            .await;
                        }
                        let _ = client_quit(&mut transfer_client).await;
                        result
                    }
                    .await;
                    let _ = respond_to.send(result);
                });
            }
            Some(WorkerCmd::DownloadRemoteFile {
                remote_path,
                local_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let profile = profile.clone();
                let host = host.to_string();
                let app = app.clone();
                let tab_id = tab_id.to_string();
                transfer_jobs.spawn(async move {
                    let result = async {
                        let mut transfer_client = connect_ftp(&profile, &host, port).await?;
                        crate::services::logging::session(
                            &app,
                            "INFO",
                            "ftp",
                            &tab_id,
                            format!("dedicated download connection opened transfer={transfer_id}"),
                        );
                        let transfer_timeout = seconds_from_profile(
                            &profile,
                            "operationTimeoutSeconds",
                            DEFAULT_FTP_OPERATION_TIMEOUT,
                            Duration::from_secs(5),
                            Duration::from_secs(3600),
                        );
                        let transfer_cancel = cancel.clone();
                        let mut result = client_download(
                            &mut transfer_client,
                            &remote_path,
                            &local_path,
                            resume_offset,
                            &transfer_id,
                            cancel,
                            &app,
                            transfer_timeout,
                        )
                        .await;
                        if result.is_ok() && !transfer_cancel.is_cancelled() {
                            result = verify_ftp_transfer_checksum(
                                &mut transfer_client,
                                &local_path,
                                &remote_path,
                                transfer_timeout,
                            )
                            .await;
                        }
                        let _ = client_quit(&mut transfer_client).await;
                        result
                    }
                    .await;
                    let _ = respond_to.send(result);
                });
            }
            Some(WorkerCmd::ReplaceRemoteFile {
                partial_path,
                destination_path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "replace",
                    cancellation,
                    client_replace(&mut client, &partial_path, &destination_path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::CommitRemoteStaging { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不使用 SSH root staging 提交链路".to_string()));
            }
            Some(WorkerCmd::RemoveRemoteFile {
                path,
                cancellation,
                respond_to,
                ..
            }) => {
                let result = ftp_with_cancellation(
                    profile,
                    "remove",
                    cancellation,
                    client_remove(&mut client, &path),
                )
                .await;
                respond_ftp_result!(respond_to, result);
            }
            Some(WorkerCmd::ExecuteRemoteCommand { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持远程命令执行".to_string()));
            }
            Some(WorkerCmd::ListSshTunnels { respond_to })
            | Some(WorkerCmd::CreateSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StartSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StopSshTunnel { respond_to, .. })
            | Some(WorkerCmd::DeleteSshTunnel { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持 SSH 隧道".to_string()));
            }
            Some(WorkerCmd::SerialControl { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持串口控制".to_string()));
            }
            Some(WorkerCmd::SerialTransfer { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持串口文件传输".to_string()));
            }
            Some(WorkerCmd::WriteTerminal(_)) | Some(WorkerCmd::ResizeTerminal { .. }) => {}
            Some(WorkerCmd::Disconnect) | None => {
                crate::services::logging::session(app, "INFO", "ftp", tab_id, "disconnecting");
                transfer_jobs.abort_all();
                while transfer_jobs.join_next().await.is_some() {
                    // Drain aborted and already-completed jobs before the
                    // session worker releases its runtime state.
                }
                let _ = ftp_with_timeout(profile, "quit", client_quit(&mut client)).await;
                set_ftp_state(
                    app,
                    tab_id,
                    "FTP disconnected".to_string(),
                    WorkspaceTabStatus::Closed,
                    None,
                    None,
                )
                .await;
                return Ok(());
            }
        }
    }
}

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

async fn set_ftp_state(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    status: WorkspaceTabStatus,
    remote_path: Option<String>,
    remote_files: Option<Vec<Value>>,
) {
    let connected = status.is_connected();
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(tab) = state
        .tabs
        .write()
        .await
        .iter_mut()
        .find(|tab| tab.id == tab_id)
    {
        tab.status = status;
    }
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.summary = summary;
        session.connected = connected;
        if connected {
            session.remote_capabilities = Some(default_ftp_capabilities());
        } else {
            // A reconnecting/error tab must not keep advertising the previous
            // server's extensions or showing its stale directory snapshot.
            // The next successful connection repopulates both atomically
            // before the capability panel is rendered again.
            session.remote_capabilities = None;
            session.remote_files.clear();
            session.remote_files_loading = false;
        }
        if let Some(path) = remote_path {
            session.remote_path = path;
        }
        if let Some(files) = remote_files {
            session.remote_files = files;
        }
    }
    let operation_state = if connected {
        crate::services::connection_operations::ConnectionOperationState::Connected
    } else {
        crate::services::connection_operations::ConnectionOperationState::Failed {
            code: crate::services::connection_operations::FILETERM_CONNECTION_FAILED.to_string(),
        }
    };
    state
        .connection_operations
        .publish_for_tab(tab_id, operation_state)
        .await;
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

async fn set_ftp_capabilities(app: &AppHandle, tab_id: &str, capabilities: RemoteFileCapabilities) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.remote_capabilities = Some(capabilities);
    }
    if let Ok(snapshot) = crate::commands::get_workspace_snapshot(app.clone()).await {
        let _ = app.emit("workspace:snapshot", snapshot);
    }
}

macro_rules! ftp_match {
    ($client:expr, $ftp:ident => $operation:expr) => {
        match $client {
            FtpClient::Plain($ftp) => $operation.await,
            FtpClient::Secure($ftp) => $operation.await,
        }
    };
}

async fn client_noop(client: &mut FtpClient) -> Result<(), String> {
    ftp_match!(client, ftp => ftp.noop()).map_err(|error| error.to_string())
}

async fn client_features(
    client: &mut FtpClient,
) -> Result<HashMap<String, Option<String>>, String> {
    ftp_match!(client, ftp => ftp.feat()).map_err(|error| error.to_string())
}

async fn client_custom_command(client: &mut FtpClient, command: &str) -> Result<String, String> {
    ftp_match!(client, ftp => ftp.custom_command(
        command,
        &[Status::File, Status::CommandOk, Status::RequestedFileActionOk],
    ))
    .map(|response| String::from_utf8_lossy(&response.body).into_owned())
    .map_err(|error| error.to_string())
}

fn ftp_capabilities_from_features(
    features: HashMap<String, Option<String>>,
) -> RemoteFileCapabilities {
    let mut capabilities = default_ftp_capabilities();
    let mut checksum_algorithms = Vec::new();
    for (name, value) in features {
        let name = name.trim().to_ascii_uppercase();
        if name.is_empty() {
            continue;
        }
        capabilities.extensions.push(name.clone());
        let value = value.unwrap_or_default().to_ascii_uppercase();
        let feature_text = format!("{name} {value}");
        for (needle, label) in [
            ("SHA-256", "SHA-256"),
            ("SHA256", "SHA-256"),
            ("SHA-1", "SHA-1"),
            ("SHA1", "SHA-1"),
            ("MD5", "MD5"),
            ("CRC", "CRC"),
        ] {
            if feature_text.contains(needle)
                && !checksum_algorithms.iter().any(|item| item == label)
            {
                checksum_algorithms.push(label.to_string());
            }
        }
    }
    capabilities.extensions.sort();
    capabilities.extensions.dedup();
    checksum_algorithms.sort();
    capabilities.checksum_algorithms = checksum_algorithms;
    capabilities
}

fn ftp_sha256_command(features: &HashMap<String, Option<String>>) -> Option<String> {
    for (name, value) in features {
        let name = name.trim().to_ascii_uppercase();
        let value = value.as_deref().unwrap_or("").to_ascii_uppercase();
        if name == "HASH" && (value.contains("SHA-256") || value.contains("SHA256")) {
            return Some("HASH".to_string());
        }
        if matches!(name.as_str(), "XSHA256" | "XSHA-256" | "SHA256") {
            return Some(name);
        }
    }
    None
}

fn parse_ftp_sha256_response(response: &str) -> Option<String> {
    response
        .split_whitespace()
        .rev()
        .find(|token| token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

fn ftp_hash_requires_algorithm_selection(command: &str) -> bool {
    command.eq_ignore_ascii_case("HASH")
}

async fn client_select_hash_sha256(client: &mut FtpClient) -> Result<(), String> {
    // The standardized HASH command uses the server's currently selected
    // algorithm. Select SHA-256 explicitly before every checksum request so
    // a server whose default is SHA-1/MD5 cannot be mistaken for SHA-256.
    ftp_match!(client, ftp => ftp.opts("HASH", Some("SHA-256"))).map_err(|error| error.to_string())
}

async fn client_sha256(
    client: &mut FtpClient,
    command: &str,
    remote_path: &str,
) -> Result<String, String> {
    if remote_path.is_empty()
        || remote_path.len() > 4096
        || remote_path.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
    {
        return Err("FTP remote path contains invalid command characters".to_string());
    }
    if ftp_hash_requires_algorithm_selection(command) {
        client_select_hash_sha256(client).await?;
    }
    let response = client_custom_command(client, &format!("{command} {remote_path}")).await?;
    parse_ftp_sha256_response(&response)
        .ok_or_else(|| "FTP server returned no recognizable SHA-256 checksum".to_string())
}

async fn verify_ftp_transfer_checksum(
    client: &mut FtpClient,
    local_path: &str,
    remote_path: &str,
    io_timeout: Duration,
) -> Result<(), String> {
    let features = ftp_io_with_timeout(
        io_timeout,
        "read FTP checksum features",
        client_features(client),
    )
    .await?;
    let Some(command) = ftp_sha256_command(&features) else {
        return Ok(());
    };
    let local_hash = crate::sessions::file_integrity::sha256_file(local_path).await?;
    let remote_hash = ftp_io_with_timeout(
        io_timeout,
        "read FTP remote checksum",
        client_sha256(client, &command, remote_path),
    )
    .await?;
    if local_hash != remote_hash {
        return Err(format!(
            "FTP transfer checksum mismatch: local {local_hash}, remote {remote_hash}"
        ));
    }
    Ok(())
}

async fn client_list(
    client: &mut FtpClient,
    path: &str,
    state: &mut FtpListingState,
) -> Result<Vec<Value>, String> {
    ftp_match!(client, ftp => list_files_with_state(ftp, path, state))
}

async fn client_read(client: &mut FtpClient, path: &str, encoding: &str) -> Result<String, String> {
    ftp_match!(client, ftp => read_file(ftp, path, encoding))
}

async fn client_write(
    client: &mut FtpClient,
    path: &str,
    content: &str,
    encoding: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => write_file(ftp, path, content, encoding))
}

async fn client_ensure_dir(client: &mut FtpClient, path: &str) -> Result<(), String> {
    ftp_match!(client, ftp => ensure_dir(ftp, path))
}

async fn client_rename(
    client: &mut FtpClient,
    source: &str,
    destination: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => rename_file(ftp, source, destination))
}

async fn client_delete(
    client: &mut FtpClient,
    path: &str,
    target_type: &str,
    target_is_symlink: bool,
) -> Result<(), String> {
    let mut visited = HashSet::new();
    let mut entries = 0;
    match client {
        FtpClient::Plain(ftp) => {
            delete_path(
                ftp,
                path,
                target_type,
                target_is_symlink,
                0,
                &mut visited,
                &mut entries,
            )
            .await
        }
        FtpClient::Secure(ftp) => {
            delete_path(
                ftp,
                path,
                target_type,
                target_is_symlink,
                0,
                &mut visited,
                &mut entries,
            )
            .await
        }
    }
}

async fn client_chmod(client: &mut FtpClient, path: &str, permissions: u32) -> Result<(), String> {
    let mode = format!("{:o}", permissions & 0o7777);
    ftp_match!(client, ftp => chmod_file(ftp, path, &mode))
}

async fn client_stat(
    client: &mut FtpClient,
    path: &str,
) -> Result<Option<TransferFileStat>, String> {
    ftp_match!(client, ftp => stat_file(ftp, path))
}

#[allow(clippy::too_many_arguments)] // Transfer state and its response channel are kept explicit at the worker boundary.
async fn client_upload(
    client: &mut FtpClient,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    ftp_match!(client, ftp => upload_file(ftp, local_path, remote_path, resume_offset, transfer_id, cancel, Some(app), io_timeout))
}

#[allow(clippy::too_many_arguments)] // Transfer state and its response channel are kept explicit at the worker boundary.
async fn client_download(
    client: &mut FtpClient,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    ftp_match!(client, ftp => download_file(ftp, remote_path, local_path, resume_offset, transfer_id, cancel, app, io_timeout))
}

async fn client_replace(
    client: &mut FtpClient,
    partial: &str,
    destination: &str,
) -> Result<(), String> {
    ftp_match!(client, ftp => replace_file(ftp, partial, destination))
}

async fn client_remove(client: &mut FtpClient, path: &str) -> Result<(), String> {
    ftp_match!(client, ftp => remove_file(ftp, path))
}

async fn client_quit(client: &mut FtpClient) -> Result<(), String> {
    ftp_match!(client, ftp => quit(ftp))
}

async fn rename_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    source: &str,
    destination: &str,
) -> Result<(), String> {
    ftp.rename(source, destination)
        .await
        .map_err(|error| error.to_string())
}

async fn chmod_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    mode: &str,
) -> Result<(), String> {
    ftp.site(format!("CHMOD {mode} {path}"))
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn remove_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<(), String> {
    match ftp.rm(path).await {
        Ok(()) => Ok(()),
        Err(error) if is_ftp_file_not_found(&error) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn is_ftp_file_not_found(error: &FtpError) -> bool {
    let FtpError::UnexpectedResponse(response) = error else {
        return false;
    };
    if response.status != Status::FileUnavailable {
        return false;
    }
    let message = String::from_utf8_lossy(&response.body).to_lowercase();
    [
        "not found",
        "no such",
        "does not exist",
        "cannot find",
        "can't find",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn is_ftp_existing_path(error: &FtpError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "file exists",
        "already exists",
        "directory exists",
        "path exists",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

async fn quit<T: TokioTlsStream + Send>(ftp: &mut ImplAsyncFtpStream<T>) -> Result<(), String> {
    ftp.quit().await.map_err(|error| error.to_string())
}

async fn list_files<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<Vec<Value>, String> {
    let mut state = FtpListingState::default();
    list_files_with_state(ftp, path, &mut state).await
}

async fn list_files_with_state<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    state: &mut FtpListingState,
) -> Result<Vec<Value>, String> {
    let lines = if state.mlsd_disabled {
        ftp.list(Some(path))
            .await
            .map_err(|error| error.to_string())?
    } else {
        match ftp.mlsd(Some(path)).await {
            Ok(lines) if lines.iter().all(|line| looks_like_mlsd_line(line)) => lines,
            Ok(lines) => {
                // A few embedded servers accept MLSD but return classic LIST
                // rows. Keep those rows, but do not pay the failed capability
                // probe again on every directory navigation.
                state.mlsd_disabled = true;
                lines
            }
            Err(_) => {
                state.mlsd_disabled = true;
                ftp.list(Some(path))
                    .await
                    .map_err(|error| error.to_string())?
            }
        }
    };
    let mut files = Vec::new();
    if path != "/" {
        files.push(serde_json::json!({
            "name": "..", "path": parent_remote_path(path), "type": "folder", "size": "-",
            "modified": "", "permission": "", "ownerGroup": ""
        }));
    }
    for line in lines {
        // `File::from_str` deliberately tries POSIX and DOS LIST formats
        // before MLSD. Some embedded FTP servers accept MLSD but still send
        // classic Unix LIST rows; parsing those as MLSD first succeeds with
        // the entire row as the name and zeroed metadata.
        let Some(parsed) = parse_ftp_listing_line(&line) else {
            continue;
        };
        let entry = parsed.entry;
        let name = entry.name();
        if matches!(name, "." | "..") {
            continue;
        }
        let full_path = join_remote_path(path, name);
        let is_symlink = entry.is_symlink();
        let mut is_directory = entry.is_directory();
        let mut size = entry.size();
        if !parsed.type_is_trusted || is_symlink {
            let resolved = resolve_untrusted_ftp_entry(ftp, &full_path, state).await;
            is_directory = resolved.0;
            if let Some(resolved_size) = resolved.1 {
                size = resolved_size;
            }
        } else {
            state.resolved_types.insert(full_path.clone(), is_directory);
        }
        let modified = entry
            .modified()
            .duration_since(UNIX_EPOCH)
            .map(|value| super::ssh::format_unix_ts(value.as_secs() as i64))
            .unwrap_or_default();
        let permission = ftp_listing_permission(&line);
        files.push(serde_json::json!({
            "name": name,
            "path": full_path,
            "type": super::ssh::effective_remote_file_type(
                is_directory,
                is_symlink,
                is_directory,
            ),
            "isSymlink": is_symlink,
            "size": if is_directory { "-".to_string() } else { format_bytes(size as u64) },
            "modified": modified,
            "permission": permission,
            "ownerGroup": match (entry.uid(), entry.gid()) { (Some(uid), Some(gid)) => format!("{uid}/{gid}"), _ => String::new() },
        }));
    }
    files.sort_by(|left, right| {
        let left_folder = left.get("type").and_then(Value::as_str) == Some("folder");
        let right_folder = right.get("type").and_then(Value::as_str) == Some("folder");
        right_folder
            .cmp(&left_folder)
            .then_with(|| left["name"].as_str().cmp(&right["name"].as_str()))
    });
    Ok(files)
}

async fn resolve_untrusted_ftp_entry<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    state: &mut FtpListingState,
) -> (bool, Option<usize>) {
    if let Some(is_directory) = state.resolved_types.get(path).copied() {
        return (is_directory, state.resolved_sizes.get(path).copied());
    }

    if !state.mlst_disabled {
        match ftp.mlst(Some(path)).await {
            Ok(line) if looks_like_mlsd_line(&line) => {
                if let Ok(entry) = ListParser::parse_mlst(&line) {
                    let is_directory = entry.is_directory();
                    state.resolved_types.insert(path.to_string(), is_directory);
                    if !is_directory {
                        state.resolved_sizes.insert(path.to_string(), entry.size());
                    }
                    return (is_directory, Some(entry.size()));
                }
                state.mlst_disabled = true;
            }
            Ok(_) => state.mlst_disabled = true,
            Err(_) => state.mlst_disabled = true,
        }
    }

    if !state.size_disabled {
        match ftp.size(path).await {
            Ok(size) => {
                state.resolved_types.insert(path.to_string(), false);
                state.resolved_sizes.insert(path.to_string(), size);
                return (false, Some(size));
            }
            Err(error) => {
                if is_unsupported_ftp_command(&error.to_string()) {
                    state.size_disabled = true;
                }
            }
        }
    }

    let previous_path = ftp.pwd().await.ok();
    let is_directory = ftp.cwd(path).await.is_ok();
    if is_directory {
        if let Some(previous_path) = previous_path {
            let _ = ftp.cwd(previous_path).await;
        }
    }
    state.resolved_types.insert(path.to_string(), is_directory);
    (is_directory, None)
}

fn parse_ftp_listing_line(line: &str) -> Option<ParsedFtpListing> {
    if let Ok(entry) = ListParser::parse_posix(line) {
        return Some(ParsedFtpListing {
            entry,
            type_is_trusted: true,
        });
    }
    if let Ok(entry) = ListParser::parse_dos(line) {
        return Some(ParsedFtpListing {
            entry,
            type_is_trusted: true,
        });
    }
    if looks_like_mlsd_line(line) {
        if let Ok(entry) = ListParser::parse_mlsd(line) {
            return Some(ParsedFtpListing {
                entry,
                type_is_trusted: true,
            });
        }
    }
    line.parse::<ListedFile>()
        .ok()
        .map(|entry| ParsedFtpListing {
            entry,
            type_is_trusted: false,
        })
}

fn looks_like_mlsd_line(line: &str) -> bool {
    let facts = line.trim_start().split_once(' ').map(|value| value.0);
    facts.is_some_and(|facts| {
        facts.contains(';')
            && facts.split(';').any(|fact| {
                fact.split_once('=')
                    .is_some_and(|(key, value)| !key.is_empty() && !value.is_empty())
            })
    })
}

fn is_unsupported_ftp_command(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "500",
        "501",
        "502",
        "504",
        "unknown command",
        "not implemented",
        "unsupported",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn ftp_listing_permission(line: &str) -> String {
    let token = line.split_whitespace().next().unwrap_or_default();
    if token.len() == 10 && matches!(token.as_bytes().first(), Some(b'-' | b'd' | b'l')) {
        return token.to_string();
    }

    let lower = line.to_ascii_lowercase();
    let Some(mode_start) = lower.find("unix.mode=") else {
        return String::new();
    };
    let mode = line[mode_start + "unix.mode=".len()..]
        .split(';')
        .next()
        .unwrap_or_default();
    let mode = mode.strip_prefix('0').unwrap_or(mode);
    if mode.len() != 3 || !mode.bytes().all(|value| matches!(value, b'0'..=b'7')) {
        return String::new();
    }
    let kind = if lower.contains("type=dir;") {
        'd'
    } else {
        '-'
    };
    let mut permission = String::with_capacity(10);
    permission.push(kind);
    for value in mode.bytes().map(|value| value - b'0') {
        permission.push(if value & 4 != 0 { 'r' } else { '-' });
        permission.push(if value & 2 != 0 { 'w' } else { '-' });
        permission.push(if value & 1 != 0 { 'x' } else { '-' });
    }
    permission
}

async fn read_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    encoding: &str,
) -> Result<String, String> {
    let mut stream = ftp
        .retr_as_stream(path)
        .await
        .map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    ftp.finalize_retr_stream(stream)
        .await
        .map_err(|error| error.to_string())?;
    Ok(decode_terminal(&bytes, encoding))
}

async fn write_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    content: &str,
    encoding: &str,
) -> Result<(), String> {
    ensure_dir(ftp, &parent_remote_path(path)).await?;
    let bytes = encode_terminal(content, encoding);
    let mut stream = ftp
        .put_with_stream(path)
        .await
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    ftp.finalize_put_stream(stream)
        .await
        .map_err(|error| error.to_string())
}

async fn ensure_dir<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<(), String> {
    let mut current = String::new();
    for part in path.split('/').filter(|part| !part.is_empty()) {
        current.push('/');
        current.push_str(part);
        match ftp.mkdir(&current).await {
            Ok(()) => {}
            Err(error) if is_ftp_existing_path(&error) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

async fn delete_path<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    target_type: &str,
    target_is_symlink: bool,
    depth: usize,
    visited: &mut HashSet<String>,
    entries: &mut usize,
) -> Result<(), String> {
    *entries = entries.saturating_add(1);
    if *entries > MAX_FTP_DELETE_ENTRIES {
        return Err(format!(
            "FTP 目录删除超过 {} 个条目，已停止以保护远端文件",
            MAX_FTP_DELETE_ENTRIES
        ));
    }
    if target_is_symlink || target_type != "folder" {
        return ftp.rm(path).await.map_err(|error| error.to_string());
    }
    if depth >= MAX_FTP_DELETE_DEPTH {
        return Err(format!(
            "FTP 目录删除超过 {} 层，已停止以保护远端文件",
            MAX_FTP_DELETE_DEPTH
        ));
    }
    if !visited.insert(path.to_string()) {
        return Err(format!("FTP 目录删除检测到循环路径：{path}"));
    }
    let children = list_files(ftp, path).await?;
    for child in children
        .into_iter()
        .filter(|child| child.get("name").and_then(Value::as_str) != Some(".."))
    {
        let child_path = child
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let child_type = child.get("type").and_then(Value::as_str).unwrap_or("file");
        let child_is_symlink = child
            .get("isSymlink")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Box::pin(delete_path(
            ftp,
            child_path,
            child_type,
            child_is_symlink,
            depth + 1,
            visited,
            entries,
        ))
        .await?;
    }
    ftp.rmdir(path).await.map_err(|error| error.to_string())
}

async fn stat_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<Option<TransferFileStat>, String> {
    match ftp.size(path).await {
        Ok(size) => Ok(Some(TransferFileStat {
            size: size as u64,
            modified_at: None,
        })),
        Err(error) if is_ftp_file_not_found(&error) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[allow(clippy::too_many_arguments)] // Resume, cancellation, and progress controls are protocol-level inputs.
async fn upload_file<T: TokioTlsStream + Send + 'static>(
    ftp: &mut ImplAsyncFtpStream<T>,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: Option<&AppHandle>,
    io_timeout: Duration,
) -> Result<(), String> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| error.to_string())?
        .len();
    if resume_offset > total {
        return Err("FTP 上传断点大于源文件".to_string());
    }
    ftp_io_with_timeout(
        io_timeout,
        "upload parent directory",
        ensure_dir(ftp, &parent_remote_path(remote_path)),
    )
    .await?;
    let mut local = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut attempt_offset = resume_offset;
    let mut rebuilt_from_zero = false;

    loop {
        local
            .seek(std::io::SeekFrom::Start(attempt_offset))
            .await
            .map_err(|error| error.to_string())?;
        let mut stream = if attempt_offset > 0 {
            match ftp_io_with_timeout(
                io_timeout,
                "open append stream",
                ftp.append_with_stream(remote_path),
            )
            .await
            {
                Ok(stream) => stream,
                Err(append_error) => {
                    ftp_io_with_timeout(
                        io_timeout,
                        "prepare resumed upload",
                        ftp.resume_transfer(attempt_offset as usize),
                    )
                    .await
                    .map_err(|rest_error| {
                        format!("FTP 续传失败：APPE={append_error}；REST={rest_error}")
                    })?;
                    ftp_io_with_timeout(
                        io_timeout,
                        "open resumed upload",
                        ftp.put_with_stream(remote_path),
                    )
                    .await
                    .map_err(|stor_error| {
                        format!("FTP 续传失败：APPE={append_error}；REST+STOR={stor_error}")
                    })?
                }
            }
        } else {
            ftp_io_with_timeout(
                io_timeout,
                "open upload stream",
                ftp.put_with_stream(remote_path),
            )
            .await
            .map_err(|error| error.to_string())?
        };
        let mut transferred = attempt_offset;
        if let Some(app) = app {
            crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
        }
        loop {
            let count = tokio::select! {
                _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
                result = ftp_io_with_timeout(io_timeout, "read local upload", local.read(&mut buffer)) => result?,
            };
            if count == 0 {
                break;
            }
            tokio::select! {
                _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
                result = ftp_io_with_timeout(io_timeout, "write FTP upload", stream.write_all(&buffer[..count])) => result?,
            }
            transferred += count as u64;
            if let Some(app) = app {
                crate::services::transfers::report_progress(app, transfer_id, transferred, total)
                    .await;
            }
        }
        ftp_io_with_timeout(
            io_timeout,
            "finalize upload",
            ftp.finalize_put_stream(stream),
        )
        .await?;

        let uploaded_size =
            ftp_io_with_timeout(io_timeout, "verify uploaded size", ftp.size(remote_path))
                .await
                .map_err(|error| format!("FTP 上传后无法校验断点大小: {error}"))?
                as u64;
        if uploaded_size == total {
            return Ok(());
        }
        if attempt_offset == 0 || rebuilt_from_zero {
            return Err(format!(
                "FTP 上传校验失败：远端 {uploaded_size} bytes，期望 {total}"
            ));
        }

        ftp_io_with_timeout(
            io_timeout,
            "remove invalid resumed upload",
            ftp.rm(remote_path),
        )
        .await
        .map_err(|error| format!("FTP 续传结果不可信，且无法删除断点: {error}"))?;
        attempt_offset = 0;
        rebuilt_from_zero = true;
    }
}

#[allow(clippy::too_many_arguments)] // Resume, cancellation, and progress controls are protocol-level inputs.
async fn download_file<T: TokioTlsStream + Send + 'static>(
    ftp: &mut ImplAsyncFtpStream<T>,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    let total = ftp_io_with_timeout(io_timeout, "read download size", ftp.size(remote_path))
        .await
        .map_err(|error| error.to_string())? as u64;
    if resume_offset > total {
        return Err("FTP 下载断点大于源文件".to_string());
    }
    if let Some(parent) = Path::new(local_path).parent() {
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
    if resume_offset > 0 {
        ftp_io_with_timeout(
            io_timeout,
            "prepare resumed download",
            ftp.resume_transfer(resume_offset as usize),
        )
        .await
        .map_err(|error| error.to_string())?;
    }
    let mut stream = ftp_io_with_timeout(
        io_timeout,
        "open download stream",
        ftp.retr_as_stream(remote_path),
    )
    .await
    .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut transferred = resume_offset;
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let count = tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = ftp_io_with_timeout(io_timeout, "read FTP download", stream.read(&mut buffer)) => result?,
        };
        if count == 0 {
            break;
        }
        tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = ftp_io_with_timeout(io_timeout, "write local download", local.write_all(&buffer[..count])) => result?,
        }
        transferred += count as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    ftp_io_with_timeout(
        io_timeout,
        "finalize download",
        ftp.finalize_retr_stream(stream),
    )
    .await
}

async fn replace_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    partial: &str,
    destination: &str,
) -> Result<(), String> {
    let backup = format!("{destination}.fileterm-backup-{}", uuid::Uuid::new_v4());
    let moved_destination = match ftp.rename(destination, backup.as_str()).await {
        Ok(()) => true,
        Err(rename_error) => match ftp.size(destination).await {
            Ok(_) => {
                return Err(format!(
                    "FTP 无法备份现有目标文件，已保留断点：{rename_error}"
                ));
            }
            Err(size_error) if is_ftp_file_not_found(&size_error) => false,
            Err(size_error) => {
                return Err(format!(
                    "FTP 无法确认目标文件是否存在，为避免覆盖现有文件已保留断点：{rename_error}；检查失败：{size_error}"
                ));
            }
        },
    };
    if let Err(error) = ftp.rename(partial, destination).await {
        if moved_destination {
            if let Err(rollback_error) = ftp.rename(backup.as_str(), destination).await {
                return Err(format!(
                    "FTP 文件替换失败，旧文件保留在 {backup}：{error}；回滚失败：{rollback_error}"
                ));
            }
        }
        return Err(format!("FTP 文件替换失败，断点已保留：{error}"));
    }
    if moved_destination {
        let _ = ftp.rm(backup).await;
    }
    Ok(())
}

fn parent_remote_path(path: &str) -> String {
    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => path[..index].to_string(),
    }
}

fn join_remote_path(directory: &str, name: &str) -> String {
    if directory == "/" || directory.is_empty() {
        format!("/{name}")
    } else {
        format!("{}/{name}", directory.trim_end_matches('/'))
    }
}

fn format_bytes(bytes: u64) -> String {
    // 统一使用 SI 单位（1000 进制），与 ssh.rs::format_bytes 和
    // local_files.rs::format_size 保持一致；同一文件在 SFTP / FTP / 本地
    // 三个视图下显示的大小必须一致，否则用户会认为是不同文件。
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", bytes)
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[cfg(unix)]
    use super::connect_ftp_with_tls_connector;
    use super::{
        client_list, client_quit, client_read, client_write, connect_ftp,
        ftp_capabilities_from_features, ftp_listing_permission, ftp_sha256_command,
        is_ftp_existing_path, is_ftp_file_not_found, join_remote_path,
        normalize_ftp_certificate_fingerprint, parent_remote_path, parse_ftp_listing_line,
        parse_ftp_sha256_response, upload_file, FtpClient, FtpListingState,
        DEFAULT_FTP_OPERATION_TIMEOUT,
    };
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    #[test]
    fn cleanup_treats_only_missing_ftp_files_as_idempotent() {
        let missing = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"No such file or directory".to_vec(),
        });
        let denied = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"Permission denied".to_vec(),
        });

        assert!(is_ftp_file_not_found(&missing));
        assert!(!is_ftp_file_not_found(&denied));

        let existing = suppaftp::FtpError::UnexpectedResponse(suppaftp::types::Response {
            status: suppaftp::Status::FileUnavailable,
            body: b"Can't create directory: File exists".to_vec(),
        });
        assert!(is_ftp_existing_path(&existing));
        assert!(!is_ftp_existing_path(&denied));
    }

    #[test]
    fn normalizes_ftps_certificate_fingerprint_formats() {
        let digest = [0xab; 32];
        let hex = "ab".repeat(32);
        let colon_hex = hex
            .as_bytes()
            .chunks(2)
            .map(|chunk| std::str::from_utf8(chunk).unwrap())
            .collect::<Vec<_>>()
            .join(":");
        let base64 = base64::engine::general_purpose::STANDARD.encode(digest);
        assert_eq!(
            normalize_ftp_certificate_fingerprint(&format!("SHA256:{colon_hex}")),
            Ok(format!("sha256:{hex}"))
        );
        assert_eq!(
            normalize_ftp_certificate_fingerprint(&base64),
            Ok(format!("sha256:{hex}"))
        );
        assert!(normalize_ftp_certificate_fingerprint("not-a-fingerprint").is_err());
    }

    #[test]
    fn discovers_ftp_checksum_extensions_and_commands() {
        let features = HashMap::from([
            ("HASH".to_string(), Some("SHA-256 SHA-1".to_string())),
            ("UTF8".to_string(), None),
        ]);
        let capabilities = ftp_capabilities_from_features(features.clone());
        assert_eq!(capabilities.extensions, vec!["HASH", "UTF8"]);
        assert_eq!(capabilities.checksum_algorithms, vec!["SHA-1", "SHA-256"]);
        assert_eq!(ftp_sha256_command(&features), Some("HASH".to_string()));

        let xsha = HashMap::from([("XSHA256".to_string(), None)]);
        assert_eq!(ftp_sha256_command(&xsha), Some("XSHA256".to_string()));
        assert!(super::ftp_hash_requires_algorithm_selection("HASH"));
        assert!(!super::ftp_hash_requires_algorithm_selection("XSHA256"));
        assert_eq!(
            parse_ftp_sha256_response(
                "213 /tmp/file 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string())
        );
    }

    #[test]
    fn parses_classic_unix_listing_before_mlsd_fallback() {
        let line = "drwxr-xr-x 5 0 0 4096 Jun 18 23:00 anydesk";
        let parsed = parse_ftp_listing_line(line).expect("classic LIST row should parse");
        let entry = parsed.entry;

        assert!(parsed.type_is_trusted);
        assert_eq!(entry.name(), "anydesk");
        assert!(entry.is_directory());
        assert_eq!(entry.size(), 4096);
        assert_eq!(ftp_listing_permission(line), "drwxr-xr-x");
    }

    #[test]
    fn keeps_standard_mlsd_listing_support() {
        let line = "type=file;size=8192;modify=20260715163248;UNIX.mode=0644;UNIX.uid=0;UNIX.gid=0; readme.txt";
        let parsed = parse_ftp_listing_line(line).expect("MLSD row should parse");
        let entry = parsed.entry;

        assert!(parsed.type_is_trusted);
        assert_eq!(entry.name(), "readme.txt");
        assert!(!entry.is_directory());
        assert_eq!(entry.size(), 8192);
        assert_eq!(ftp_listing_permission(line), "-rw-r--r--");
    }

    #[test]
    fn marks_unstructured_serv_u_rows_for_capability_probe() {
        let parsed =
            parse_ftp_listing_line("reports").expect("name-only row should remain visible");

        assert_eq!(parsed.entry.name(), "reports");
        assert!(!parsed.type_is_trusted);
    }

    #[tokio::test]
    async fn remembers_mlsd_failure_and_uses_fast_classic_list_afterward() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_classic_listing_server(listener, commands.clone()));
        let profile = serde_json::json!({
            "type": "ftp", "username": "test", "password": "test", "securityMode": "none"
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        let mut state = FtpListingState::default();

        let first = client_list(&mut client, "/", &mut state).await.unwrap();
        let second = client_list(&mut client, "/", &mut state).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(first[0]["name"], "folder");
        assert_eq!(first[0]["type"], "folder");
        assert_eq!(first[1]["name"], "payload.bin");
        assert_eq!(first[1]["size"], "2.0 KB");

        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        let commands = commands.lock().await;
        assert_eq!(
            commands.iter().filter(|command| *command == "MLSD").count(),
            1
        );
        assert_eq!(
            commands.iter().filter(|command| *command == "LIST").count(),
            2
        );
    }

    async fn run_classic_listing_server(listener: TcpListener, commands: Arc<Mutex<Vec<String>>>) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        writer
            .write_all(b"220 Serv-U compatible fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, _) = command.split_once(' ').unwrap_or((command, ""));
            let verb = verb.to_ascii_uppercase();
            commands.lock().await.push(verb.clone());
            match verb.as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "EPSV" | "PASV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let data_port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb == "EPSV" {
                        format!("229 Entering Extended Passive Mode (|||{data_port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            data_port / 256,
                            data_port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "MLSD" => writer.write_all(b"500 Unknown command\r\n").await.unwrap(),
                "LIST" => {
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    data.write_all(
                        b"drwxr-xr-x 2 0 0 4096 Jun 18 23:00 folder\r\n-rw-r--r-- 1 0 0 2048 Jun 18 23:00 payload.bin\r\n",
                    )
                    .await
                    .unwrap();
                    data.shutdown().await.unwrap();
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    async fn run_resumable_upload_server(
        listener: TcpListener,
        supports_appe: bool,
        stored: Arc<Mutex<Vec<u8>>>,
        commands: Arc<Mutex<Vec<String>>>,
    ) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        let mut rest_offset = 0_usize;
        writer
            .write_all(b"220 FileTerm resumable upload fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            let verb = verb.to_ascii_uppercase();
            commands.lock().await.push(verb.clone());
            match verb.as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "EPSV" | "PASV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let data_port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb == "EPSV" {
                        format!("229 Entering Extended Passive Mode (|||{data_port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            data_port / 256,
                            data_port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "APPE" if supports_appe => {
                    assert_eq!(argument, "/resume.bin");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut suffix = Vec::new();
                    data.read_to_end(&mut suffix).await.unwrap();
                    stored.lock().await.extend_from_slice(&suffix);
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "APPE" => {
                    let _ = data_listener.take().unwrap().accept().await.unwrap();
                    writer.write_all(b"502 APPE unsupported\r\n").await.unwrap();
                }
                "REST" => {
                    rest_offset = argument.parse().unwrap();
                    writer
                        .write_all(b"350 Restarting at offset\r\n")
                        .await
                        .unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/resume.bin");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut suffix = Vec::new();
                    data.read_to_end(&mut suffix).await.unwrap();
                    let mut bytes = stored.lock().await;
                    bytes.truncate(rest_offset);
                    bytes.extend_from_slice(&suffix);
                    rest_offset = 0;
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "SIZE" => {
                    let size = stored.lock().await.len();
                    writer
                        .write_all(format!("213 {size}\r\n").as_bytes())
                        .await
                        .unwrap();
                }
                "DELE" => {
                    stored.lock().await.clear();
                    writer.write_all(b"250 Deleted\r\n").await.unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    async fn assert_resumable_upload_strategy(supports_appe: bool) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stored = Arc::new(Mutex::new(b"abc".to_vec()));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_resumable_upload_server(
            listener,
            supports_appe,
            stored.clone(),
            commands.clone(),
        ));
        let root =
            std::env::temp_dir().join(format!("fileterm-ftp-resume-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root).await.unwrap();
        let source = root.join("resume.bin");
        tokio::fs::write(&source, b"abcdef").await.unwrap();
        let profile = serde_json::json!({
            "type": "ftp", "username": "test", "password": "test", "securityMode": "none"
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        match &mut client {
            FtpClient::Plain(ftp) => upload_file(
                ftp,
                source.to_str().unwrap(),
                "/resume.bin",
                3,
                "transfer-test",
                tokio_util::sync::CancellationToken::new(),
                None,
                DEFAULT_FTP_OPERATION_TIMEOUT,
            )
            .await
            .unwrap(),
            FtpClient::Secure(_) => panic!("plain fixture returned a secure client"),
        }
        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        assert_eq!(*stored.lock().await, b"abcdef");

        let commands = commands.lock().await;
        let appe = commands
            .iter()
            .position(|command| command == "APPE")
            .unwrap();
        if supports_appe {
            assert!(!commands.iter().any(|command| command == "REST"));
            assert!(!commands.iter().any(|command| command == "STOR"));
        } else {
            let rest = commands
                .iter()
                .position(|command| command == "REST")
                .unwrap();
            let stor = commands
                .iter()
                .position(|command| command == "STOR")
                .unwrap();
            assert!(appe < rest && rest < stor);
        }
        assert!(commands.iter().any(|command| command == "SIZE"));
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn resumable_upload_prefers_appe_and_verifies_size() {
        assert_resumable_upload_strategy(true).await;
    }

    #[tokio::test]
    async fn resumable_upload_falls_back_to_rest_and_stor() {
        assert_resumable_upload_strategy(false).await;
    }

    #[cfg(unix)]
    async fn run_secured_ftps_session<S>(
        stream: S,
        acceptor: &suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
        send_greeting: bool,
    ) where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let mut control = BufReader::new(stream);
        let mut data_listener = None;
        if send_greeting {
            control
                .get_mut()
                .write_all(b"220 FileTerm real FTPS fixture\r\n")
                .await
                .unwrap();
        }
        let mut line = String::new();
        loop {
            line.clear();
            if control.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            match verb.to_ascii_uppercase().as_str() {
                "USER" => control
                    .get_mut()
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => control
                    .get_mut()
                    .write_all(b"230 Logged in\r\n")
                    .await
                    .unwrap(),
                "PBSZ" | "PROT" | "TYPE" | "OPTS" => {
                    control.get_mut().write_all(b"200 OK\r\n").await.unwrap()
                }
                "PASV" | "EPSV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb.eq_ignore_ascii_case("EPSV") {
                        format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        )
                    };
                    control
                        .get_mut()
                        .write_all(response.as_bytes())
                        .await
                        .unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    control
                        .get_mut()
                        .write_all(b"150 Opening protected data connection\r\n")
                        .await
                        .unwrap();
                    let (data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut data = acceptor.accept(data).await.unwrap();
                    let mut bytes = Vec::new();
                    data.read_to_end(&mut bytes).await.unwrap();
                    *stored.lock().await = bytes;
                    control
                        .get_mut()
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "RETR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    control
                        .get_mut()
                        .write_all(b"150 Opening protected data connection\r\n")
                        .await
                        .unwrap();
                    let (data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut data = acceptor.accept(data).await.unwrap();
                    let bytes = stored.lock().await.clone();
                    data.write_all(&bytes).await.unwrap();
                    data.shutdown().await.unwrap();
                    control
                        .get_mut()
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    control
                        .get_mut()
                        .write_all(b"221 Goodbye\r\n")
                        .await
                        .unwrap();
                    return;
                }
                _ => control.get_mut().write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }

    #[cfg(unix)]
    async fn run_explicit_ftps_server(
        listener: TcpListener,
        acceptor: suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = BufReader::new(stream);
        control
            .get_mut()
            .write_all(b"220 FileTerm explicit FTPS fixture\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            assert!(control.read_line(&mut line).await.unwrap() > 0);
            let command = line.trim_end_matches(['\r', '\n']);
            if command.eq_ignore_ascii_case("AUTH TLS") {
                control
                    .get_mut()
                    .write_all(b"234 Begin TLS negotiation\r\n")
                    .await
                    .unwrap();
                let secured = acceptor.accept(control.into_inner()).await.unwrap();
                run_secured_ftps_session(secured, &acceptor, stored, false).await;
                return;
            }
            control
                .get_mut()
                .write_all(b"500 Send AUTH TLS first\r\n")
                .await
                .unwrap();
        }
    }

    #[cfg(unix)]
    async fn run_implicit_ftps_server(
        listener: TcpListener,
        acceptor: suppaftp::async_native_tls::TlsAcceptor,
        stored: Arc<Mutex<Vec<u8>>>,
    ) {
        let (stream, _) = listener.accept().await.unwrap();
        let secured = acceptor.accept(stream).await.unwrap();
        run_secured_ftps_session(secured, &acceptor, stored, true).await;
    }

    #[cfg(unix)]
    fn create_ftps_identity() -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("fileterm-ftps-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let key = root.join("key.pem");
        let cert = root.join("cert.pem");
        let identity = root.join("identity.p12");
        let openssl = "/usr/bin/openssl";
        assert!(
            std::path::Path::new(openssl).exists(),
            "real FTPS fixture requires {openssl}"
        );
        let certificate = std::process::Command::new(openssl)
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout"])
            .arg(&key)
            .args(["-out"])
            .arg(&cert)
            .args(["-subj", "/CN=localhost", "-days", "1"])
            .output()
            .unwrap();
        assert!(
            certificate.status.success(),
            "openssl certificate generation failed: {}",
            String::from_utf8_lossy(&certificate.stderr)
        );
        let package = std::process::Command::new(openssl)
            .args(["pkcs12", "-export", "-out"])
            .arg(&identity)
            .args(["-inkey"])
            .arg(&key)
            .args(["-in"])
            .arg(&cert)
            .args(["-passout", "pass:fileterm-test"])
            .output()
            .unwrap();
        assert!(
            package.status.success(),
            "openssl PKCS#12 generation failed: {}",
            String::from_utf8_lossy(&package.stderr)
        );
        (root, identity)
    }

    #[test]
    fn keeps_ftp_paths_posix_normalized() {
        assert_eq!(parent_remote_path("/one/file"), "/one");
        assert_eq!(join_remote_path("/", "file"), "/file");
    }

    #[tokio::test]
    async fn plain_ftp_client_round_trips_against_a_real_tcp_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let stored = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn(run_minimal_ftp_server(listener, stored.clone()));
        let profile = serde_json::json!({
            "securityMode": "none", "username": "fileterm", "password": "test",
        });
        let mut client = connect_ftp(&profile, "127.0.0.1", port).await.unwrap();
        client_write(&mut client, "/roundtrip.txt", "Tauri FTP", "utf-8")
            .await
            .unwrap();
        assert_eq!(
            client_read(&mut client, "/roundtrip.txt", "utf-8")
                .await
                .unwrap(),
            "Tauri FTP"
        );
        client_quit(&mut client).await.unwrap();
        server.await.unwrap();
        assert_eq!(&*stored.lock().await, b"Tauri FTP");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_and_implicit_ftps_round_trip_over_real_tls_control_and_data_channels() {
        let (root, identity) = create_ftps_identity();
        for security_mode in ["explicit", "implicit"] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let acceptor = suppaftp::async_native_tls::TlsAcceptor::new(
                tokio::fs::File::open(&identity).await.unwrap(),
                "fileterm-test",
            )
            .await
            .unwrap();
            let stored = Arc::new(Mutex::new(Vec::new()));
            let server = if security_mode == "explicit" {
                tokio::spawn(run_explicit_ftps_server(listener, acceptor, stored.clone()))
            } else {
                tokio::spawn(run_implicit_ftps_server(listener, acceptor, stored.clone()))
            };
            let insecure_connector = suppaftp::async_native_tls::TlsConnector::new()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true);
            let profile = serde_json::json!({
                "securityMode": security_mode,
                "username": "fileterm",
                "password": "test",
            });
            let mut client = connect_ftp_with_tls_connector(
                &profile,
                "localhost",
                port,
                suppaftp::tokio::AsyncNativeTlsConnector::from(insecure_connector),
            )
            .await
            .unwrap();
            client_write(&mut client, "/roundtrip.txt", "Tauri FTPS", "utf-8")
                .await
                .unwrap();
            assert_eq!(
                client_read(&mut client, "/roundtrip.txt", "utf-8")
                    .await
                    .unwrap(),
                "Tauri FTPS"
            );
            client_quit(&mut client).await.unwrap();
            server.await.unwrap();
            assert_eq!(&*stored.lock().await, b"Tauri FTPS");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    async fn run_minimal_ftp_server(listener: TcpListener, stored: Arc<Mutex<Vec<u8>>>) {
        let (control, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = control.into_split();
        let mut reader = BufReader::new(reader);
        let mut data_listener = None;
        writer
            .write_all(b"220 FileTerm Tauri test FTP\r\n")
            .await
            .unwrap();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap() == 0 {
                return;
            }
            let command = line.trim_end_matches(['\r', '\n']);
            let (verb, argument) = command.split_once(' ').unwrap_or((command, ""));
            match verb.to_ascii_uppercase().as_str() {
                "USER" => writer
                    .write_all(b"331 Password required\r\n")
                    .await
                    .unwrap(),
                "PASS" => writer.write_all(b"230 Logged in\r\n").await.unwrap(),
                "TYPE" | "OPTS" => writer.write_all(b"200 OK\r\n").await.unwrap(),
                "PASV" | "EPSV" => {
                    let data = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = data.local_addr().unwrap().port();
                    data_listener = Some(data);
                    let response = if verb.eq_ignore_ascii_case("EPSV") {
                        format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                    } else {
                        format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        )
                    };
                    writer.write_all(response.as_bytes()).await.unwrap();
                }
                "STOR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let mut bytes = Vec::new();
                    data.read_to_end(&mut bytes).await.unwrap();
                    *stored.lock().await = bytes;
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "RETR" => {
                    assert_eq!(argument, "/roundtrip.txt");
                    writer
                        .write_all(b"150 Opening data connection\r\n")
                        .await
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().await.unwrap();
                    let bytes = stored.lock().await.clone();
                    data.write_all(&bytes).await.unwrap();
                    data.shutdown().await.unwrap();
                    writer
                        .write_all(b"226 Transfer complete\r\n")
                        .await
                        .unwrap();
                }
                "QUIT" => {
                    writer.write_all(b"221 Goodbye\r\n").await.unwrap();
                    return;
                }
                _ => writer.write_all(b"200 OK\r\n").await.unwrap(),
            }
        }
    }
}
