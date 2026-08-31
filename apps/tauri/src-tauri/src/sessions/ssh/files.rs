fn editor_staging_path(path: &str) -> String {
    format!("{path}.fileterm-edit-{}", uuid::Uuid::new_v4())
}

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
        // `DirEntry::metadata()` preserves the link itself. Resolve the
        // target only for navigation so a link to a directory remains
        // enterable while a link to a regular file opens in the editor.
        let link_target_is_dir = if is_link {
            match timeout(SFTP_SYMLINK_TARGET_TIMEOUT, sftp.metadata(&full_path)).await {
                Ok(Ok(target)) => target.is_dir(),
                _ => false,
            }
        } else {
            false
        };
        let file_type = effective_remote_file_type(is_dir, is_link, link_target_is_dir);
        let size_str = if is_dir || link_target_is_dir {
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
            "isSymlink": is_link,
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
    let bytes = encode_text(content, encoding)?;
    let destination_metadata = match sftp.symlink_metadata(path).await {
        Ok(metadata) => Some(metadata),
        Err(error) if is_sftp_not_found(&error) => None,
        Err(error) => return Err(format!("无法读取远端文件属性: {error}")),
    };
    let commit_path = if destination_metadata
        .as_ref()
        .is_some_and(SftpMetadata::is_symlink)
    {
        sftp.canonicalize(path)
            .await
            .map_err(|error| format!("无法解析远端软链接目标，已阻止保存: {error}"))?
    } else {
        path.to_string()
    };
    let staging_path = editor_staging_path(&commit_path);

    // Check write permission against the destination before using rename for
    // a regular file. Otherwise a writable parent directory could let an
    // atomic replacement bypass a read-only destination's file mode.
    if destination_metadata.is_some() {
        let _ = sftp
            .open_with_flags(path, OpenFlags::WRITE)
            .await
            .map_err(|error| format!("远端文件不可写: {error}"))?;
    }

    let write_result = async {
        {
            let mut file = sftp
                .create(&staging_path)
                .await
                .map_err(|error| format!("无法创建远端编辑临时文件: {error}"))?;
            file.write_all(&bytes)
                .await
                .map_err(|error| format!("写入远端编辑临时文件失败: {error}"))?;
            file.flush()
                .await
                .map_err(|error| format!("刷新远端编辑临时文件失败: {error}"))?;
        }

        let written_size = sftp
            .symlink_metadata(&staging_path)
            .await
            .map_err(|error| format!("无法校验远端编辑临时文件: {error}"))?
            .size
            .unwrap_or(0);
        if written_size != bytes.len() as u64 {
            return Err(format!(
                "远端编辑临时文件校验失败：{written_size} bytes，期望 {}",
                bytes.len()
            ));
        }
        Ok::<(), String>(())
    }
    .await;

    if let Err(error) = write_result {
        let _ = sftp.remove_file(&staging_path).await;
        return Err(error);
    }

    // Commit the effective target path so a symlink remains intact while its
    // regular-file target still receives the same atomic rename/rollback.
    replace_remote_file(sftp, &staging_path, &commit_path).await
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
// root-mode helpers (exec channel + `sudo` / `su`)
// ─────────────────────────────────────────────────────────────────────────────

fn root_stat_shell_command(path: &str) -> String {
    format!(
        "if [ -e {} ] && [ ! -d {} ]; then stat -c '%s|%Y' -- {}; fi",
        shell_quote(path),
        shell_quote(path),
        shell_quote(path),
    )
}

fn root_editor_write_shell_command(staging_path: &str, expected_size: u64) -> String {
    format!(
        "set -e\nbase64 -d > {}\ntest \"$(wc -c < {})\" -eq {}",
        shell_quote(staging_path),
        shell_quote(staging_path),
        expected_size,
    )
}

fn root_editor_verify_shell_command(path: &str, expected_size: u64) -> String {
    format!(
        "set -e\ntest \"$(wc -c < {})\" -eq {}",
        shell_quote(path),
        expected_size,
    )
}

async fn stat_root_remote_file(
    handle: &Handle<ClientHandler>,
    path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Option<TransferFileStat>, String> {
    // A missing .fileterm-part means a fresh upload, not a failed stat.
    // Keep the shell command successful in that case so exec status handling
    // can distinguish it from an actual root/su failure.
    let command = root_stat_shell_command(path);
    let output =
        exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password).await?;
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
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let parent = std::path::Path::new(destination_path)
        .parent()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".to_string());
    let command = root_replace_remote_file_command(&parent, partial_path, destination_path);
    exec_shell_file_command(handle, &command, access_method, sudo_user, sudo_password)
        .await
        .map(|_| ())
}

fn root_replace_remote_file_command(
    parent: &str,
    partial_path: &str,
    destination_path: &str,
) -> String {
    format!(
        "set -e\nmkdir -p {parent}\nif [ -L {destination} ]; then\n  target=\"$(readlink -f -- {destination} 2>/dev/null || true)\"\n  if [ -n \"$target\" ] && [ -f \"$target\" ]; then\n    chown --reference=\"$target\" -- {partial} 2>/dev/null || true\n    chmod --reference=\"$target\" -- {partial} 2>/dev/null || true\n    mv -f -- {partial} \"$target\"\n  else\n    cat -- {partial} > {destination}\n    rm -f -- {partial}\n  fi\nelse\n  if [ -e {destination} ]; then\n    chown --reference={destination} -- {partial} 2>/dev/null || true\n    chmod --reference={destination} -- {partial} 2>/dev/null || true\n  fi\n  mv -f -- {partial} {destination}\nfi",
        parent = shell_quote(parent),
        partial = shell_quote(partial_path),
        destination = shell_quote(destination_path),
    )
}

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

/// POSIX shell quoting: wrap in single quotes, escape embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn root_file_command(
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
    command: &str,
) -> (String, Option<String>) {
    let user = sudo_user.as_deref().unwrap_or("root");
    match access_method {
        RootFileAccessMethod::Sudo => {
            let full_command = if sudo_password.is_some() {
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
            (full_command, sudo_password.clone())
        }
        RootFileAccessMethod::Su => (
            format!(
                "su -s /bin/sh -c {} {}",
                shell_quote(command),
                shell_quote(user)
            ),
            sudo_password.clone(),
        ),
    }
}

/// Add a post-authentication frame to commands executed through `su`.
///
/// A PTY combines the password prompt and command output into one stream. The
/// marker is printed only after `su` has accepted the password, so consumers
/// can discard the prompt without guessing at localized text or corrupting a
/// `stat`/`base64` payload.
fn su_exec_command(command: &str) -> String {
    format!(
        "printf '%s\\n' {}; {}",
        shell_quote(SU_EXEC_OUTPUT_MARKER),
        command
    )
}

fn strip_su_exec_output(output: &str) -> Result<String, String> {
    let Some(marker_start) = output.find(SU_EXEC_OUTPUT_MARKER) else {
        return Err("su root 文件命令未返回认证后的输出标记".to_string());
    };
    let body = &output[marker_start + SU_EXEC_OUTPUT_MARKER.len()..];
    // PTY line discipline may translate LF to CRLF. Normalize it before
    // parsing `find -printf` rows or other line-oriented root command output.
    Ok(body
        .trim_start_matches(['\r', '\n'])
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

async fn request_root_exec_pty(channel: &Channel<russh::client::Msg>) -> Result<(), String> {
    timeout(
        SUDO_VERIFY_TIMEOUT,
        channel.request_pty(
            true,
            "xterm-256color",
            80,
            24,
            0,
            0,
            &[
                (russh::Pty::ECHO, 0),
                (russh::Pty::ECHOE, 0),
                (russh::Pty::ECHOK, 0),
                (russh::Pty::ECHONL, 0),
                (russh::Pty::TTY_OP_ISPEED, 115200),
                (russh::Pty::TTY_OP_OSPEED, 115200),
            ],
        ),
    )
    .await
    .map_err(|_| "su 认证超时：服务器未响应 PTY 请求".to_string())?
    .map_err(|error| format!("su 文件通道无法申请 PTY: {error}"))
}

/// Authenticate a `su` exec channel and return any bytes that followed the
/// post-authentication marker in the same SSH packet.  Streaming upload and
/// download use this handshake before sending/decoding their payload.
async fn wait_for_su_output_marker(
    channel: &mut Channel<russh::client::Msg>,
    password: Option<&str>,
) -> Result<Vec<u8>, String> {
    let marker = SU_EXEC_OUTPUT_MARKER.as_bytes();
    let mut output = Vec::new();
    let mut password_sent = password.is_none();
    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "su 认证超时：服务器未在 10 秒内响应".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if output.len() < 64 * 1024 {
                    output.extend_from_slice(data.as_ref());
                }
                let visible = visible_shell_text(&String::from_utf8_lossy(&output));
                let lower = visible.to_ascii_lowercase();
                let marker_seen = output.windows(marker.len()).any(|window| window == marker);
                if !password_sent
                    && !marker_seen
                    && (lower.contains("password") || visible.contains("密码"))
                {
                    if let Some(password) = password {
                        channel
                            .data_bytes(format!("{password}\n").into_bytes())
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    password_sent = true;
                }
                if let Some(start) = output
                    .windows(marker.len())
                    .position(|window| window == marker)
                {
                    return Ok(output[start + marker.len()..].to_vec());
                }
            }
            ChannelMsg::ExitStatus {
                exit_status: status,
            } => {
                let detail = String::from_utf8_lossy(&output).trim().to_string();
                if root_access_auth_failed(&detail.to_lowercase())
                    || detail.to_lowercase().contains("password")
                    || detail.contains("密码")
                {
                    return Err("su 认证失败：密码错误或未授予 su 权限".to_string());
                }
                let detail = if detail.is_empty() {
                    String::new()
                } else {
                    format!("：{}", detail.chars().take(512).collect::<String>())
                };
                return Err(format!("su 文件命令失败（exit={status}）{detail}"));
            }
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }
    let detail = String::from_utf8_lossy(&output).trim().to_string();
    if root_access_auth_failed(&detail.to_lowercase())
        || detail.to_lowercase().contains("password")
        || detail.contains("密码")
    {
        Err("su 认证失败：密码错误或未授予 su 权限".to_string())
    } else {
        Err("su root 文件命令未返回认证后的输出标记".to_string())
    }
}

async fn wait_for_root_stream_exit(
    channel: &mut Channel<russh::client::Msg>,
) -> Result<u32, String> {
    let mut exit_status = None;
    let mut detail = String::new();
    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "root 文件传输完成后远端命令未退出，已超时".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                if detail.len() < 4096 {
                    detail.push_str(&String::from_utf8_lossy(data.as_ref()));
                }
            }
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
    let status = exit_status.ok_or_else(|| "root 文件传输未返回退出状态".to_string())?;
    if status != 0 {
        let detail = detail.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!("：{}", detail.chars().take(512).collect::<String>())
        };
        return Err(format!("root 文件传输命令失败（exit={status}）{detail}"));
    }
    Ok(status)
}

/// Execute a `su -c` command through a PTY and complete the password
/// handshake before sending any command input. Some PAM/su combinations drop
/// bytes that arrive before the password prompt, even though a normal shell
/// accepts them from the PTY input queue. The marker printed by
/// `su_exec_command` also gives passwordless/root callers a safe point at
/// which to send payload data.
#[allow(clippy::too_many_arguments)]
async fn exec_su_command_with_pty_input(
    handle: &Handle<ClientHandler>,
    command: &str,
    password: Option<&str>,
    input: Option<&[u8]>,
    send_eof: bool,
) -> Result<(String, Option<u32>), String> {
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| error.to_string())?;
    request_root_exec_pty(&channel).await?;
    channel
        .exec(true, command)
        .await
        .map_err(|error| error.to_string())?;

    let marker = SU_EXEC_OUTPUT_MARKER.as_bytes();
    let mut output = Vec::new();
    let mut password_sent = password.is_none();
    let mut input_sent = input.is_none();
    let mut exit_status = None;
    let mut marker_seen = false;
    let mut password_prompt_seen = false;

    loop {
        let message = timeout(SUDO_VERIFY_TIMEOUT, channel.wait())
            .await
            .map_err(|_| "su 认证超时：服务器未在 10 秒内响应".to_string())?;
        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                let bytes = data.as_ref();
                if output.len() < 64 * 1024 {
                    let remaining = 64 * 1024 - output.len();
                    output.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
                }
                if output.windows(marker.len()).any(|window| window == marker) {
                    marker_seen = true;
                }
                let visible = visible_shell_text(&String::from_utf8_lossy(&output));
                let lower = visible.to_ascii_lowercase();
                if !marker_seen && (lower.contains("password") || visible.contains("密码")) {
                    password_prompt_seen = true;
                }

                if !password_sent && (password_prompt_seen || marker_seen) {
                    if let Some(password) = password {
                        if password_prompt_seen {
                            channel
                                .data_bytes(format!("{password}\n").into_bytes())
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    password_sent = true;
                }
                if marker_seen && !input_sent {
                    if let Some(input) = input {
                        channel
                            .data_bytes(input.to_vec())
                            .await
                            .map_err(|error| error.to_string())?;
                        if send_eof {
                            channel
                                .data_bytes(vec![0x04])
                                .await
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    input_sent = true;
                }
            }
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

    Ok((String::from_utf8_lossy(&output).into_owned(), exit_status))
}

/// Run a shell command through the independent exec channel with the same
/// root strategy observed in the interactive terminal.
async fn exec_shell_file_command(
    handle: &Handle<ClientHandler>,
    command: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let command = if access_method == RootFileAccessMethod::Su {
        su_exec_command(command)
    } else {
        command.to_string()
    };
    let (full_cmd, password) = root_file_command(access_method, sudo_user, sudo_password, &command);

    // 整个 exec 包超时：PTY 模式下 root 错误密码可能 retry 多次，channel
    // 不会自然退出。超时后返回错误，前端 loading 能在 10 秒内解除。
    let (output, exit_status) = if access_method == RootFileAccessMethod::Su {
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            exec_su_command_with_pty_input(handle, &full_cmd, password.as_deref(), None, false),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(
                    "root 认证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string(),
                )
            }
        }
    } else if let Some(pwd) = password {
        let stdin = format!("{pwd}\n");
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            crate::sessions::system_metrics::exec_command_with_stdin_status(
                handle, &full_cmd, &stdin,
            ),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => {
                return Err(
                    "root 认证超时：服务器未在 10 秒内响应，可能密码错误或网络中断".to_string(),
                )
            }
        }
    } else {
        match timeout(
            SUDO_VERIFY_TIMEOUT,
            crate::sessions::system_metrics::exec_command_with_status(handle, &full_cmd),
        )
        .await
        {
            Ok(inner) => inner?,
            Err(_) => return Err("root 认证超时：服务器未在 10 秒内响应".to_string()),
        }
    };

    let lower = output.to_lowercase();
    if root_access_auth_failed(&lower)
        || lower.contains("a password is required")
        || lower.contains("no password was provided")
        || lower.contains("sudo: permission denied")
        || (access_method == RootFileAccessMethod::Su
            && (lower.contains("password") || output.contains("密码"))
            && !output.contains(SU_EXEC_OUTPUT_MARKER))
    {
        return Err(match access_method {
            RootFileAccessMethod::Sudo => "sudo 认证失败：密码错误或未授予 sudo 权限".to_string(),
            RootFileAccessMethod::Su => "su 认证失败：密码错误或未授予 su 权限".to_string(),
        });
    }

    let command_output = if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output).unwrap_or_else(|_| output.clone())
    } else {
        output.clone()
    };
    let status = exit_status.ok_or_else(|| "root 文件命令未返回退出状态".to_string())?;
    if status != 0 {
        let detail = command_output.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!("：{}", detail.chars().take(512).collect::<String>())
        };
        return Err(format!("root 文件命令失败（exit={status}）{detail}"));
    }
    if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output)
    } else {
        Ok(output)
    }
}

/// List a directory via `find -printf` under the active root strategy.
async fn exec_list_dir_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<Vec<Value>, String> {
    let cmd = root_list_shell_command(path);
    let output =
        exec_shell_file_command(handle, &cmd, access_method, sudo_user, sudo_password).await?;
    Ok(parse_root_file_list(&output, path))
}

fn root_list_shell_command(path: &str) -> String {
    // `%y` is the entry type and `%Y` is the type after following a
    // symbolic link. Keep both so the renderer can retain link information
    // without mistaking a link to a regular file for a directory.
    format!(
        "find {} -maxdepth 1 -mindepth 1 -printf '%y|%Y|%s|%T@|%u:%g|%m|%f\\n' 2>/dev/null",
        shell_quote(path)
    )
}

fn parse_root_file_list(output: &str, path: &str) -> Vec<Value> {
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
        let parts: Vec<&str> = line.splitn(7, '|').collect();
        if parts.len() < 7 {
            continue;
        }
        let type_char = parts[0].chars().next().unwrap_or('f');
        let is_dir = type_char == 'd';
        let is_link = type_char == 'l';
        let link_target_is_dir = is_link && parts[1].starts_with('d');
        let effective_is_dir = is_dir || link_target_is_dir;
        let size_value = parts[2].parse::<u64>().unwrap_or(0);
        let size_str = if effective_is_dir {
            "-".to_string()
        } else {
            format_bytes(size_value)
        };
        let mtime: i64 = parts[3]
            .split('.')
            .next()
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let owner_group = parts[4].to_string();
        let perm_octal = u32::from_str_radix(parts[5], 8).unwrap_or(0o644);
        let name = parts[6].to_string();
        if name == "." || name == ".." {
            continue;
        }

        let file_type = effective_remote_file_type(is_dir, is_link, link_target_is_dir);
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
            "isSymlink": is_link,
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
    items
}

pub(crate) fn effective_remote_file_type(
    is_dir: bool,
    is_link: bool,
    link_target_is_dir: bool,
) -> &'static str {
    if is_dir || (is_link && link_target_is_dir) {
        "folder"
    } else {
        "file"
    }
}

/// Read a file via the active root strategy + base64 (binary-safe over exec).
/// Decodes the result using the given encoding (mirrors Electron's
/// `readRemoteFileViaShell` + `decodeBuffer`).
async fn exec_read_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    encoding: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<String, String> {
    let cmd = format!("base64 {}", shell_quote(path));
    let output =
        exec_shell_file_command(handle, &cmd, access_method, sudo_user, sudo_password).await?;
    let trimmed: String = output.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&trimmed)
        .map_err(|e| format!("base64 decode failed: {}", e))?;
    decode_bytes(&bytes, encoding)
}

/// Write a file via the active root strategy + base64 (binary-safe). Encodes the content
/// using the given encoding before base64-wrapping (mirrors Electron's
/// `writeRemoteFileViaShell` + `encodeText`).
async fn exec_write_file_via_shell(
    handle: &Handle<ClientHandler>,
    path: &str,
    content: &str,
    encoding: &str,
    access_method: RootFileAccessMethod,
    sudo_user: &Option<String>,
    sudo_password: &Option<String>,
) -> Result<(), String> {
    let bytes = encode_text(content, encoding)?;
    let staging_path = editor_staging_path(path);
    // Never stream editor content directly into the destination. The old
    // `base64 -d | tee destination` pipeline truncated the original first,
    // and a failed/truncated base64 stage could still be reported successful
    // because the shell returned tee's status.
    let cmd = root_editor_write_shell_command(&staging_path, bytes.len() as u64);
    let command = if access_method == RootFileAccessMethod::Su {
        su_exec_command(&cmd)
    } else {
        cmd
    };
    let (full_cmd, password) = root_file_command(access_method, sudo_user, sudo_password, &command);
    let encoded_input = if access_method == RootFileAccessMethod::Su {
        // A PTY normally runs in canonical mode, whose input line limit is
        // commonly 4096 bytes. Keep every base64 line below that limit while
        // preserving 3-byte block boundaries so concatenated lines decode to
        // the original bytes exactly.
        let mut lines = String::new();
        for chunk in bytes.chunks(3000) {
            lines.push_str(&base64::engine::general_purpose::STANDARD.encode(chunk));
            lines.push('\n');
        }
        if lines.is_empty() {
            lines.push('\n');
        }
        lines
    } else {
        format!(
            "{}\n",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        )
    };
    let (output, exit_status) = if access_method == RootFileAccessMethod::Su {
        exec_su_command_with_pty_input(
            handle,
            &full_cmd,
            password.as_deref(),
            Some(encoded_input.as_bytes()),
            true,
        )
        .await?
    } else {
        let stdin = if let Some(pwd) = password {
            format!("{}\n{}", pwd, encoded_input)
        } else {
            encoded_input
        };
        crate::sessions::system_metrics::exec_command_with_stdin_status(
            handle, &full_cmd, &stdin,
        )
        .await?
    };
    let lower = output.to_lowercase();
    if root_access_auth_failed(&lower)
        || (access_method == RootFileAccessMethod::Su
            && (lower.contains("password") || output.contains("密码"))
            && !output.contains(SU_EXEC_OUTPUT_MARKER))
    {
        return Err(match access_method {
            RootFileAccessMethod::Sudo => "sudo authentication failed".to_string(),
            RootFileAccessMethod::Su => "su authentication failed".to_string(),
        });
    }
    let status = exit_status.ok_or_else(|| "root 写入命令未返回退出状态".to_string())?;
    if status != 0 {
        return Err(format!("root 写入命令失败（exit={status}）"));
    }
    if access_method == RootFileAccessMethod::Su {
        strip_su_exec_output(&output)?;
    }

    replace_root_remote_file(
        handle,
        &staging_path,
        path,
        access_method,
        sudo_user,
        sudo_password,
    )
    .await
    .map_err(|error| format!("远端文件提交失败（临时文件保留：{staging_path}）：{error}"))?;

    exec_shell_file_command(
        handle,
        &root_editor_verify_shell_command(path, bytes.len() as u64),
        access_method,
        sudo_user,
        sudo_password,
    )
    .await
    .map(|_| ())
    .map_err(|error| format!("远端文件提交后校验失败：{error}"))?;
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
