use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;
use suppaftp::list::{File as ListedFile, ListParser};
use suppaftp::tokio::{
    AsyncFtpStream, AsyncNativeTlsConnector, AsyncNativeTlsFtpStream, ImplAsyncFtpStream,
    TokioTlsStream,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

use super::terminal::{decode_terminal, encode_terminal};
use super::{TransferFileStat, WorkerCmd};

const TRANSFER_CANCELED: &str = "transfer canceled";

enum FtpClient {
    Plain(AsyncFtpStream),
    Secure(AsyncNativeTlsFtpStream),
}

pub fn start_ftp_worker(
    tab_id: String,
    profile: Value,
    command_rx: mpsc::Receiver<WorkerCmd>,
    app: AppHandle,
) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_ftp_worker(&tab_id, &profile, command_rx, &app).await {
            crate::services::logging::write(&app, "ERROR", "ftp", format!("tab={tab_id} {error}"));
            set_ftp_state(
                &app,
                &tab_id,
                format!("FTP error: {error}"),
                false,
                None,
                None,
            )
            .await;
        }
    });
}

async fn run_ftp_worker(
    tab_id: &str,
    profile: &Value,
    mut command_rx: mpsc::Receiver<WorkerCmd>,
    app: &AppHandle,
) -> Result<(), String> {
    let host = profile
        .get("host")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "FTP host is required".to_string())?;
    let port = profile.get("port").and_then(Value::as_u64).unwrap_or(21) as u16;
    let remote_path = profile
        .get("remotePath")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("/")
        .to_string();
    let mut client = connect_ftp(profile, host, port).await?;
    let initial_files = client_list(&mut client, &remote_path).await?;
    set_ftp_state(
        app,
        tab_id,
        format!("FTP {}:{}", host, port),
        true,
        Some(remote_path.clone()),
        Some(initial_files),
    )
    .await;

    loop {
        match command_rx.recv().await {
            Some(WorkerCmd::ListRemoteFiles { path, respond_to }) => {
                let _ = respond_to.send(client_list(&mut client, &path).await);
            }
            Some(WorkerCmd::ReadRemoteFile {
                path,
                encoding,
                respond_to,
            }) => {
                let result = client_read(&mut client, &path, &encoding).await;
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::WriteRemoteFile {
                path,
                content,
                encoding,
                respond_to,
            }) => {
                let result = client_write(&mut client, &path, &content, &encoding).await;
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::CreateRemoteDirectory {
                parent_path,
                name,
                respond_to,
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let _ = respond_to.send(client_ensure_dir(&mut client, &path).await);
            }
            Some(WorkerCmd::CreateRemoteFile {
                parent_path,
                name,
                respond_to,
            }) => {
                let path = join_remote_path(&parent_path, &name);
                let result = client_write(&mut client, &path, "", "utf-8").await;
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::CopyRemotePath { respond_to, .. }) => {
                let _ =
                    respond_to.send(Err("FTP 不支持服务器内复制，请改用下载后上传".to_string()));
            }
            Some(WorkerCmd::MoveRemotePath {
                target_path,
                destination_path,
                respond_to,
            }) => {
                let _ = respond_to
                    .send(client_rename(&mut client, &target_path, &destination_path).await);
            }
            Some(WorkerCmd::RenameRemotePath {
                target_path,
                new_name,
                respond_to,
            }) => {
                let destination = join_remote_path(&parent_remote_path(&target_path), &new_name);
                let _ =
                    respond_to.send(client_rename(&mut client, &target_path, &destination).await);
            }
            Some(WorkerCmd::DeleteRemotePath {
                target_path,
                target_type,
                respond_to,
            }) => {
                let _ =
                    respond_to.send(client_delete(&mut client, &target_path, &target_type).await);
            }
            Some(WorkerCmd::ChangeRemotePermissions {
                target_path,
                permissions,
                recursive,
                respond_to,
                ..
            }) => {
                let result = if recursive {
                    Err("FTP 暂不支持递归修改权限".to_string())
                } else {
                    client_chmod(&mut client, &target_path, permissions).await
                };
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::SetRemoteFileAccessMode {
                mode, respond_to, ..
            }) => {
                let result = if mode == "root" {
                    Err("FTP 不支持 SSH root 文件模式".to_string())
                } else {
                    Ok(())
                };
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::StatRemoteFile { path, respond_to }) => {
                let _ = respond_to.send(client_stat(&mut client, &path).await);
            }
            Some(WorkerCmd::UploadLocalFile {
                local_path,
                remote_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let result = client_upload(
                    &mut client,
                    &local_path,
                    &remote_path,
                    resume_offset,
                    &transfer_id,
                    cancel,
                    app,
                )
                .await;
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::DownloadRemoteFile {
                remote_path,
                local_path,
                resume_offset,
                transfer_id,
                cancel,
                respond_to,
            }) => {
                let result = client_download(
                    &mut client,
                    &remote_path,
                    &local_path,
                    resume_offset,
                    &transfer_id,
                    cancel,
                    app,
                )
                .await;
                let _ = respond_to.send(result);
            }
            Some(WorkerCmd::ReplaceRemoteFile {
                partial_path,
                destination_path,
                respond_to,
            }) => {
                let _ = respond_to
                    .send(client_replace(&mut client, &partial_path, &destination_path).await);
            }
            Some(WorkerCmd::RemoveRemoteFile { path, respond_to }) => {
                let _ = respond_to.send(client_remove(&mut client, &path).await);
            }
            Some(WorkerCmd::ListSshTunnels { respond_to })
            | Some(WorkerCmd::CreateSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StartSshTunnel { respond_to, .. })
            | Some(WorkerCmd::StopSshTunnel { respond_to, .. })
            | Some(WorkerCmd::DeleteSshTunnel { respond_to, .. }) => {
                let _ = respond_to.send(Err("FTP 不支持 SSH 隧道".to_string()));
            }
            Some(WorkerCmd::WriteTerminal(_)) | Some(WorkerCmd::ResizeTerminal { .. }) => {}
            Some(WorkerCmd::Disconnect) | None => {
                let _ = client_quit(&mut client).await;
                set_ftp_state(
                    app,
                    tab_id,
                    "FTP disconnected".to_string(),
                    false,
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
    connect_ftp_with_tls_connector(
        profile,
        host,
        port,
        AsyncNativeTlsConnector::from(suppaftp::async_native_tls::TlsConnector::new()),
    )
    .await
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
    let address = (host, port);
    match mode {
        "none" => {
            let mut client = AsyncFtpStream::connect(address)
                .await
                .map_err(|error| error.to_string())?;
            client
                .login(username, password)
                .await
                .map_err(|error| error.to_string())?;
            Ok(FtpClient::Plain(client))
        }
        "explicit" => {
            // `into_secure` needs a stream typed for the TLS backend up front; using the
            // no-TLS alias here makes the generic stream types incompatible.
            let client = AsyncNativeTlsFtpStream::connect(address)
                .await
                .map_err(|error| error.to_string())?;
            let mut client = client
                .into_secure(tls_connector, host)
                .await
                .map_err(|error| error.to_string())?;
            client
                .login(username, password)
                .await
                .map_err(|error| error.to_string())?;
            Ok(FtpClient::Secure(client))
        }
        "implicit" => {
            let mut client = AsyncNativeTlsFtpStream::connect_secure_implicit(
                address,
                tls_connector,
                host,
            )
            .await
            .map_err(|error| error.to_string())?;
            client
                .login(username, password)
                .await
                .map_err(|error| error.to_string())?;
            Ok(FtpClient::Secure(client))
        }
        other => Err(format!("Unsupported FTP security mode: {other}")),
    }
}

async fn set_ftp_state(
    app: &AppHandle,
    tab_id: &str,
    summary: String,
    connected: bool,
    remote_path: Option<String>,
    remote_files: Option<Vec<Value>>,
) {
    let state = app.state::<crate::services::workspace::WorkspaceState>();
    if let Some(tab) = state
        .tabs
        .write()
        .await
        .iter_mut()
        .find(|tab| tab.id == tab_id)
    {
        tab.status = if connected {
            "connected"
        } else {
            "disconnected"
        }
        .to_string();
    }
    if let Some(session) = state.sessions.write().await.get_mut(tab_id) {
        session.summary = summary;
        session.connected = connected;
        if let Some(path) = remote_path {
            session.remote_path = path;
        }
        if let Some(files) = remote_files {
            session.remote_files = files;
        }
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

async fn client_list(client: &mut FtpClient, path: &str) -> Result<Vec<Value>, String> {
    ftp_match!(client, ftp => list_files(ftp, path))
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
) -> Result<(), String> {
    ftp_match!(client, ftp => delete_path(ftp, path, target_type))
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

async fn client_upload(
    client: &mut FtpClient,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    ftp_match!(client, ftp => upload_file(ftp, local_path, remote_path, resume_offset, transfer_id, cancel, app))
}

async fn client_download(
    client: &mut FtpClient,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    ftp_match!(client, ftp => download_file(ftp, remote_path, local_path, resume_offset, transfer_id, cancel, app))
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
    ftp.rm(path).await.map_err(|error| error.to_string())
}

async fn quit<T: TokioTlsStream + Send>(ftp: &mut ImplAsyncFtpStream<T>) -> Result<(), String> {
    ftp.quit().await.map_err(|error| error.to_string())
}

async fn list_files<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
) -> Result<Vec<Value>, String> {
    let lines = match ftp.mlsd(Some(path)).await {
        Ok(lines) => lines,
        Err(_) => ftp
            .list(Some(path))
            .await
            .map_err(|error| error.to_string())?,
    };
    let mut files = Vec::new();
    if path != "/" {
        files.push(serde_json::json!({
            "name": "..", "path": parent_remote_path(path), "type": "folder", "size": "-",
            "modified": "", "permission": "", "ownerGroup": ""
        }));
    }
    for line in lines {
        let entry = ListParser::parse_mlsd(&line).or_else(|_| line.parse::<ListedFile>());
        let Ok(entry) = entry else { continue };
        let name = entry.name();
        if matches!(name, "." | "..") {
            continue;
        }
        let is_directory = entry.is_directory();
        let modified = entry
            .modified()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs().to_string())
            .unwrap_or_default();
        files.push(serde_json::json!({
            "name": name,
            "path": join_remote_path(path, name),
            "type": if is_directory { "folder" } else if entry.is_symlink() { "symlink" } else { "file" },
            "size": if is_directory { "-".to_string() } else { format_bytes(entry.size() as u64) },
            "modified": modified,
            "permission": "",
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
        let _ = ftp.mkdir(&current).await;
    }
    Ok(())
}

async fn delete_path<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    path: &str,
    target_type: &str,
) -> Result<(), String> {
    if target_type != "folder" {
        return ftp.rm(path).await.map_err(|error| error.to_string());
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
        Box::pin(delete_path(ftp, child_path, child_type)).await?;
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
        Err(error) => {
            let message = error.to_string().to_lowercase();
            if message.contains("not found")
                || message.contains("no such")
                || message.contains("550")
            {
                Ok(None)
            } else {
                Err(error.to_string())
            }
        }
    }
}

async fn upload_file<T: TokioTlsStream + Send + 'static>(
    ftp: &mut ImplAsyncFtpStream<T>,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    let total = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| error.to_string())?
        .len();
    if resume_offset > total {
        return Err("FTP 上传断点大于源文件".to_string());
    }
    ensure_dir(ftp, &parent_remote_path(remote_path)).await?;
    let mut local = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| error.to_string())?;
    local
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    if resume_offset > 0 {
        ftp.resume_transfer(resume_offset as usize)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut stream = ftp
        .put_with_stream(remote_path)
        .await
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut transferred = resume_offset;
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let count = tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = local.read(&mut buffer) => result.map_err(|error| error.to_string())?,
        };
        if count == 0 {
            break;
        }
        tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = stream.write_all(&buffer[..count]) => result.map_err(|error| error.to_string())?,
        }
        transferred += count as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    ftp.finalize_put_stream(stream)
        .await
        .map_err(|error| error.to_string())
}

async fn download_file<T: TokioTlsStream + Send + 'static>(
    ftp: &mut ImplAsyncFtpStream<T>,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    let total = ftp
        .size(remote_path)
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
        ftp.resume_transfer(resume_offset as usize)
            .await
            .map_err(|error| error.to_string())?;
    }
    let mut stream = ftp
        .retr_as_stream(remote_path)
        .await
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut transferred = resume_offset;
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let count = tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = stream.read(&mut buffer) => result.map_err(|error| error.to_string())?,
        };
        if count == 0 {
            break;
        }
        tokio::select! {
            _ = cancel.cancelled() => { let _ = ftp.abort(stream).await; return Err(TRANSFER_CANCELED.to_string()); }
            result = local.write_all(&buffer[..count]) => result.map_err(|error| error.to_string())?,
        }
        transferred += count as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    ftp.finalize_retr_stream(stream)
        .await
        .map_err(|error| error.to_string())
}

async fn replace_file<T: TokioTlsStream + Send>(
    ftp: &mut ImplAsyncFtpStream<T>,
    partial: &str,
    destination: &str,
) -> Result<(), String> {
    let backup = format!("{destination}.fileterm-backup-{}", uuid::Uuid::new_v4());
    let moved_destination = match ftp.size(destination).await {
        Ok(_) => {
            ftp.rename(destination, backup.as_str())
                .await
                .map_err(|error| error.to_string())?;
            true
        }
        Err(_) => false,
    };
    if let Err(error) = ftp.rename(partial, destination).await {
        if moved_destination {
            let _ = ftp.rename(backup.as_str(), destination).await;
        }
        return Err(error.to_string());
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
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
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
    use std::sync::Arc;

    use super::{
        client_quit, client_read, client_write, connect_ftp, connect_ftp_with_tls_connector,
        join_remote_path, parent_remote_path,
    };
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

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
                "PBSZ" | "PROT" | "TYPE" | "OPTS" => control
                    .get_mut()
                    .write_all(b"200 OK\r\n")
                    .await
                    .unwrap(),
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
                    control.get_mut().write_all(response.as_bytes()).await.unwrap();
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
                    control.get_mut().write_all(b"221 Goodbye\r\n").await.unwrap();
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
            control.get_mut().write_all(b"500 Send AUTH TLS first\r\n").await.unwrap();
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
        assert!(std::path::Path::new(openssl).exists(), "real FTPS fixture requires {openssl}");
        let certificate = std::process::Command::new(openssl)
            .args([
                "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout",
            ])
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
