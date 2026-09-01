#[allow(clippy::too_many_arguments)] // Root transfer context mirrors the resumable worker contract.
async fn upload_root_local_file(
    handle: &Handle<ClientHandler>,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    if access_method == RootFileAccessMethod::Su {
        return upload_root_local_file_via_su_pty(
            handle,
            local_path,
            remote_path,
            resume_offset,
            transfer_id,
            cancel,
            app,
            sudo_user,
            sudo_password,
        )
        .await;
    }
    let metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| error.to_string())?;
    let total = metadata.len();
    if resume_offset > total {
        return Err("上传断点大于源文件".to_string());
    }
    let mut source = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| error.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;

    let shell_command = root_upload_shell_command(remote_path, resume_offset);
    let (command, password) =
        root_file_command(access_method, sudo_user, sudo_password, &shell_command);
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(password) = password.as_deref() {
        channel
            .data_bytes(format!("{password}\n").into_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }

    let mut transferred = resume_offset;
    let mut buffer = vec![0_u8; 64 * 1024];
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let read = read_local_transfer_chunk(&mut source, &mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        if cancel.is_cancelled() {
            return Err(TRANSFER_CANCELED.to_string());
        }
        channel
            .data_bytes(buffer[..read].to_vec())
            .await
            .map_err(|error| error.to_string())?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    channel.eof().await.map_err(|error| error.to_string())?;

    let mut stderr = String::new();
    let mut exit_status = None;
    loop {
        let message = match timeout(SUDO_VERIFY_TIMEOUT, channel.wait()).await {
            Ok(message) => message,
            Err(_) => return Err("root 上传完成后远端命令未退出，已超时".to_string()),
        };
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::ExtendedData { data, .. } => {
                if stderr.len() < 4096 {
                    stderr.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                exit_status = Some(status);
            }
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    if exit_status.is_some_and(|status| status != 0) {
        let detail = stderr.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!("：{detail}")
        };
        return Err(format!(
            "root 上传命令失败（exit={}）{detail}",
            exit_status.unwrap_or(1)
        ));
    }
    let lower = stderr.to_lowercase();
    if root_access_auth_failed(&lower) {
        return Err(match access_method {
            RootFileAccessMethod::Sudo => "sudo authentication failed".to_string(),
            RootFileAccessMethod::Su => "su authentication failed".to_string(),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn upload_root_local_file_via_su_pty(
    handle: &Handle<ClientHandler>,
    local_path: &str,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| error.to_string())?;
    let total = metadata.len();
    if resume_offset > total {
        return Err("上传断点大于源文件".to_string());
    }
    let mut source = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| error.to_string())?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;

    let shell_command = root_upload_base64_shell_command(remote_path, resume_offset);
    let command = su_exec_command(&shell_command);
    let (command, password) =
        root_file_command(RootFileAccessMethod::Su, sudo_user, sudo_password, &command);
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    request_root_exec_pty(&channel).await?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| error.to_string())?;
    let _ = wait_for_su_output_marker(&mut channel, password.as_deref()).await?;

    let mut transferred = resume_offset;
    // 3,000 raw bytes become 4,000 base64 characters, below the usual
    // canonical-PTY line limit while retaining a 3-byte boundary.
    let mut buffer = vec![0_u8; 3000];
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    loop {
        let read = read_local_transfer_chunk(&mut source, &mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(&buffer[..read]);
        channel
            .data_bytes(format!("{encoded}\n").into_bytes())
            .await
            .map_err(|error| error.to_string())?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    // The PTY-backed base64 decoder needs a terminal VEOF (Ctrl+D), not only
    // an SSH channel EOF, to finish decoding and let the su command exit.
    channel
        .data_bytes(vec![0x04])
        .await
        .map_err(|error| error.to_string())?;
    wait_for_root_stream_exit(&mut channel).await?;
    Ok(())
}

fn root_upload_shell_command(remote_path: &str, resume_offset: u64) -> String {
    let parent = std::path::Path::new(remote_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let write_operator = if resume_offset == 0 { ">" } else { ">>" };
    format!(
        "set -e\nmkdir -p {}\ncat {} {}",
        shell_quote(&parent),
        write_operator,
        shell_quote(remote_path),
    )
}

fn root_upload_base64_shell_command(remote_path: &str, resume_offset: u64) -> String {
    let parent = std::path::Path::new(remote_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let write_operator = if resume_offset == 0 { ">" } else { ">>" };
    format!(
        "set -e\nmkdir -p {}\nbase64 -d {} {}",
        shell_quote(&parent),
        write_operator,
        shell_quote(remote_path),
    )
}

// Privileged staging/stream transfer operations.

async fn commit_root_staging_file(
    handle: &Handle<ClientHandler>,
    staging_path: &str,
    partial_path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let parent = std::path::Path::new(partial_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let command = format!(
        "set -e\nmkdir -p {}\nrm -f -- {}\ncat -- {} > {}\nrm -f -- {}",
        shell_quote(&parent),
        shell_quote(partial_path),
        shell_quote(staging_path),
        shell_quote(partial_path),
        shell_quote(staging_path),
    );
    exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password)
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
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let source =
        stat_root_remote_file(handle, remote_path, access_method, sudo_user, sudo_password)
            .await?
            .ok_or_else(|| "root 下载源文件不存在或无法读取".to_string())?;
    if resume_offset > source.size {
        return Err("root 下载断点大于源文件".to_string());
    }
    if access_method == RootFileAccessMethod::Su {
        return download_root_remote_file_via_su_pty(
            handle,
            remote_path,
            local_path,
            resume_offset,
            transfer_id,
            cancel,
            app,
            &source,
            sudo_user,
            sudo_password,
        )
        .await;
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
    let (command, password) =
        root_file_command(access_method, sudo_user, sudo_password, &shell_command);
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| error.to_string())?;
    if let Some(password) = password.as_deref() {
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

fn append_base64_stream_bytes(encoded: &mut Vec<u8>, bytes: &[u8]) {
    encoded.extend(
        bytes
            .iter()
            .copied()
            .filter(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'+' | b'/' | b'=')),
    );
}

#[allow(clippy::too_many_arguments)]
async fn write_base64_stream_blocks(
    encoded: &mut Vec<u8>,
    local: &mut tokio::fs::File,
    transferred: &mut u64,
    transfer_id: &str,
    total: u64,
    cancel: &tokio_util::sync::CancellationToken,
    app: &AppHandle,
) -> Result<(), String> {
    let complete_length = encoded.len() / 4 * 4;
    if complete_length == 0 {
        return Ok(());
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded[..complete_length])
        .map_err(|error| format!("root 下载 base64 解码失败: {error}"))?;
    tokio::select! {
        _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
        result = local.write_all(&decoded) => result.map_err(|error| error.to_string())?,
    }
    encoded.drain(..complete_length);
    *transferred += decoded.len() as u64;
    crate::services::transfers::report_progress(app, transfer_id, *transferred, total).await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_root_remote_file_via_su_pty(
    handle: &Handle<ClientHandler>,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    source: &TransferFileStat,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
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
        format!("base64 {}", shell_quote(remote_path))
    } else {
        format!(
            "tail -c +{} -- {} | base64",
            resume_offset + 1,
            shell_quote(remote_path)
        )
    };
    let command = su_exec_command(&shell_command);
    let (command, password) =
        root_file_command(RootFileAccessMethod::Su, sudo_user, sudo_password, &command);
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    request_root_exec_pty(&channel).await?;
    channel
        .exec(true, command.as_str())
        .await
        .map_err(|error| error.to_string())?;

    let mut encoded = Vec::new();
    let initial_data = wait_for_su_output_marker(&mut channel, password.as_deref()).await?;
    append_base64_stream_bytes(&mut encoded, &initial_data);

    let mut transferred = resume_offset;
    crate::services::transfers::report_progress(app, transfer_id, transferred, source.size).await;
    write_base64_stream_blocks(
        &mut encoded,
        &mut local,
        &mut transferred,
        transfer_id,
        source.size,
        &cancel,
        app,
    )
    .await?;

    let mut exit_status = None;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
            message = channel.wait() => message,
        };
        match next {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                append_base64_stream_bytes(&mut encoded, data.as_ref());
                write_base64_stream_blocks(
                    &mut encoded,
                    &mut local,
                    &mut transferred,
                    transfer_id,
                    source.size,
                    &cancel,
                    app,
                )
                .await?;
            }
            Some(ChannelMsg::ExitStatus {
                exit_status: status,
            }) => {
                exit_status = Some(status);
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) if exit_status.is_some() => break,
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                exit_status = wait_for_root_download_exit_status(&mut channel, &cancel).await?;
                break;
            }
            None => break,
            _ => {}
        }
    }

    if !encoded.is_empty() {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .map_err(|error| format!("root 下载 base64 解码失败: {error}"))?;
        tokio::select! {
            _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
            result = local.write_all(&decoded) => result.map_err(|error| error.to_string())?,
        }
        transferred += decoded.len() as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, source.size)
            .await;
    }
    local.flush().await.map_err(|error| error.to_string())?;

    if exit_status.is_none() {
        crate::services::logging::warn(
            app,
            &format!("transfer:{transfer_id}"),
            "root 下载完成但 SSH 通道未返回退出状态，按完整字节数确认成功",
        );
    }
    validate_root_download_completion(exit_status, transferred, source.size)
}

async fn wait_for_root_download_exit_status(
    channel: &mut Channel<russh::client::Msg>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Option<u32>, String> {
    let mut exit_status = None;
    loop {
        let message = tokio::select! {
            _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
            result = timeout(SUDO_VERIFY_TIMEOUT, channel.wait()) => {
                match result {
                    Ok(message) => message,
                    Err(_) => return Ok(exit_status),
                }
            }
        };
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                exit_status = Some(status);
            }
            ChannelMsg::Eof | ChannelMsg::Close if exit_status.is_some() => break,
            ChannelMsg::Eof | ChannelMsg::Close => {}
            _ => {}
        }
    }
    Ok(exit_status)
}

/// PTY-backed `su` channels on some SSH servers close without forwarding an
/// exit status. Once the exact source byte count has been received, the binary
/// stream itself is sufficient to establish transfer completeness. An explicit
/// non-zero status and a short stream must still fail.
fn validate_root_download_completion(
    exit_status: Option<u32>,
    transferred: u64,
    expected: u64,
) -> Result<(), String> {
    if transferred != expected {
        return Err(format!("root 下载未完成（{transferred}/{expected} bytes）"));
    }
    if let Some(status) = exit_status {
        if status != 0 {
            return Err(format!("root 下载命令失败（exit={status}）"));
        }
    }
    Ok(())
}
