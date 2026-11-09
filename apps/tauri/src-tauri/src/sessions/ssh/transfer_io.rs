fn transfer_checksum_available(
    exec_enabled: bool,
    mode: &str,
    algorithms: Option<&[String]>,
) -> bool {
    // Root resume verification is mandatory; never turn a failed probe into a bypass.
    mode == "root"
        || (exec_enabled
            && algorithms
                .is_some_and(|items| items.iter().any(|item| item == "SHA-256 (SSH exec)")))
}

const TRANSFER_CANCELED: &str = "transfer canceled";

fn sftp_transfer_error(operation: &str, path: &str, error: impl std::fmt::Display) -> String {
    format!("SFTP {operation} path={path:?}: {error}")
}

fn is_sftp_permission_denied(error: &SftpError) -> bool {
    matches!(error, SftpError::Status(status) if status.status_code == StatusCode::PermissionDenied)
}

async fn transfer_remote_metadata(
    sftp: &SftpSession,
    path: &str,
) -> Result<SftpMetadata, SftpError> {
    match sftp.metadata(path).await {
        Err(error) if is_sftp_permission_denied(&error) => {
            // Older Xlight requires READ for STAT even when LIST is permitted.
            // https://www.xlightftpd.com/whatsnew.htm (3.8.5.1)
            let parent = parent_remote_path(path).unwrap_or_else(|| ".".into());
            let name = path.rsplit('/').next().unwrap_or(path);
            let entries = match sftp.read_dir(&parent).await {
                Ok(entries) => entries,
                Err(listing_error) => {
                    tracing::warn!(category = "transfer", scope = "transfer", stage = "READDIR", remote_path = %parent, error = %listing_error, "SFTP STAT fallback failed");
                    return Err(error);
                }
            };
            if let Some(entry) = entries.into_iter().find(|entry| entry.file_name() == name) {
                let metadata = entry.metadata();
                // READDIR does not follow links like STAT does.
                if metadata.file_type().is_file() && metadata.size.is_some() {
                    tracing::warn!(
                        category = "transfer",
                        scope = "transfer",
                        stage = "STAT",
                        remote_path = path,
                        "SFTP STAT denied; using listed file attributes"
                    );
                    return Ok(metadata);
                }
            }
            // An omitted/filtered entry does not prove the checkpoint is absent.
            Err(error)
        }
        result => result,
    }
}

async fn transfer_directory_exists(sftp: &SftpSession, path: &str) -> Result<bool, String> {
    match sftp.metadata(path).await {
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(format!("传输目标父路径不是目录: {path}")),
        Err(error) if is_sftp_permission_denied(&error) => {
            // A successful directory listing confirms a virtual directory without STAT.
            sftp.read_dir(path).await.map_err(|listing_error| {
                sftp_transfer_error(
                    "STAT",
                    path,
                    format!(
                        "{error}; {}",
                        sftp_transfer_error("READDIR directory fallback", path, listing_error)
                    ),
                )
            })?;
            tracing::warn!(
                category = "transfer",
                scope = "transfer",
                stage = "STAT",
                remote_path = path,
                "SFTP STAT denied; directory confirmed by listing"
            );
            Ok(true)
        }
        Err(error) if is_sftp_not_found(&error) => Ok(false),
        Err(error) => Err(sftp_transfer_error("STAT", path, error)),
    }
}

async fn ensure_transfer_parent_dir(sftp: &SftpSession, path: &str) -> Result<(), String> {
    let parent = parent_remote_path(path).unwrap_or_else(|| "/".to_string());
    if parent == "/" || transfer_directory_exists(sftp, &parent).await? {
        return Ok(());
    }
    let mut current = String::new();
    for segment in parent.split('/').filter(|segment| !segment.is_empty()) {
        current.push('/');
        current.push_str(segment);
        if !transfer_directory_exists(sftp, &current).await? {
            sftp.create_dir(&current)
                .await
                .map_err(|error| sftp_transfer_error("MKDIR", &current, error))?;
        }
    }
    Ok(())
}

async fn read_remote_transfer_chunk<R: AsyncRead + Unpin>(
    file: &mut R,
    buffer: &mut [u8],
    remaining: u64,
    cancel: &CancellationToken,
    io_timeout: Duration,
) -> Result<usize, String> {
    // Do not issue an extra READ at EOF: some older SFTP servers never answer it.
    if remaining == 0 {
        return Ok(0);
    }
    let limit = remaining.min(buffer.len() as u64) as usize;
    let count = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
        result = timeout(io_timeout, file.read(&mut buffer[..limit])) => {
            result.map_err(|_| "读取远端下载数据超时".to_string())?
                .map_err(|error| format!("读取远端下载数据失败: {error}"))?
        }
    };
    if count == 0 {
        return Err(format!(
            "下载源提前结束，还缺 {remaining} bytes，断点已保留"
        ));
    }
    Ok(count)
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
    io_timeout: Duration,
) -> Result<(), String> {
    tokio::select! {
        _ = cancel.cancelled() => Err(TRANSFER_CANCELED.to_string()),
        result = timeout(io_timeout, file.write_all(bytes)) => result
            .map_err(|_| "写入本地下载文件超时".to_string())?
            .map_err(|error| format!("写入本地下载文件失败: {error}")),
    }
}

// Standard SFTP upload/download and atomic replacement.

#[allow(clippy::too_many_arguments)] // Bounded size and per-read timeout belong to the upload contract.
async fn upload_local_file(
    sftp: &SftpSession,
    local_path: &str,
    total: u64,
    remote_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: Option<&AppHandle>,
    io_timeout: Duration,
) -> Result<String, String> {
    let mut source = crate::sessions::upload_source::UploadSource::open(
        local_path,
        total,
        resume_offset,
        &cancel,
    )
    .await?;
    ensure_transfer_parent_dir(sftp, remote_path).await?;
    if resume_offset > 0 {
        let mut checkpoint = sftp
            .open(remote_path)
            .await
            .map_err(|error| sftp_transfer_error("OPEN resume-read", remote_path, error))?;
        crate::sessions::upload_source::verify_resume_prefix(
            &mut checkpoint,
            resume_offset,
            &source.hash(),
            &cancel,
            io_timeout,
        )
        .await
        .map_err(|error| {
            if error == TRANSFER_CANCELED {
                error
            } else {
                sftp_transfer_error("READ resume-verify", remote_path, error)
            }
        })?;
        if resume_offset == total {
            return Ok(source.hash());
        }
    }
    let flags = if resume_offset == 0 {
        OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
    } else {
        OpenFlags::WRITE | OpenFlags::CREATE
    };
    let mut destination = sftp
        .open_with_flags(remote_path, flags)
        .await
        .map_err(|error| {
            sftp_transfer_error(
                "OPEN write",
                remote_path,
                format!("flags={flags:?} offset={resume_offset}: {error}"),
            )
        })?;
    destination
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| sftp_transfer_error("SEEK", remote_path, error))?;
    let mut transferred = resume_offset;
    let mut buffer = vec![0_u8; crate::sessions::TRANSFER_IO_BUFFER_BYTES];
    if let Some(app) = app {
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    loop {
        let read = source.read(&mut buffer, &cancel).await?;
        if read == 0 {
            break;
        }
        write_remote_transfer_chunk(&mut destination, &buffer[..read], &cancel)
            .await
            .map_err(|error| {
                if error == TRANSFER_CANCELED {
                    error
                } else {
                    sftp_transfer_error(
                        "WRITE",
                        remote_path,
                        format!("offset={transferred} bytes={read}: {error}"),
                    )
                }
            })?;
        transferred += read as u64;
        if let Some(app) = app {
            crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
        }
    }
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
        result = timeout(io_timeout, async {
            destination.flush().await.map_err(|error| sftp_transfer_error("WRITE/FLUSH", remote_path, error))?;
            destination.shutdown().await.map_err(|error| sftp_transfer_error("CLOSE", remote_path, error))
        }) => {
            result.map_err(|_| sftp_transfer_error("WRITE/FLUSH/CLOSE", remote_path, "timeout"))??;
        }
    }
    Ok(source.hash())
}

#[allow(clippy::too_many_arguments)] // Download carries its per-operation timeout.
async fn download_remote_file(
    sftp: &SftpSession,
    remote_path: &str,
    local_path: &str,
    resume_offset: u64,
    transfer_id: &str,
    cancel: tokio_util::sync::CancellationToken,
    app: &AppHandle,
    io_timeout: Duration,
) -> Result<(), String> {
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "download-metadata", "SFTP download stage started");
    let metadata = sftp
        .metadata(remote_path)
        .await
        .map_err(|error| format!("读取远端下载文件属性失败: {error}"))?;
    let total = metadata.size.ok_or("远端未返回下载文件大小")?;
    if resume_offset > total {
        return Err("下载断点大于源文件".to_string());
    }
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "remote-open", "SFTP download stage started");
    let mut source = sftp
        .open(remote_path)
        .await
        .map_err(|error| format!("打开远端下载文件失败: {error}"))?;
    source
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "local-open", "SFTP download stage started");
    let mut destination = crate::sessions::transfer_file_safety::open_local_download_checkpoint(
        local_path,
        resume_offset,
    )
    .await
    .map_err(|error| format!("打开本地下载断点失败: {error}"))?;
    destination
        .seek(std::io::SeekFrom::Start(resume_offset))
        .await
        .map_err(|error| error.to_string())?;
    let mut transferred = resume_offset;
    let mut buffer = vec![0_u8; crate::sessions::TRANSFER_IO_BUFFER_BYTES];
    crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "download-read", total_bytes = total, resume_offset, "SFTP download stage started");
    loop {
        let read = read_remote_transfer_chunk(
            &mut source,
            &mut buffer,
            total - transferred,
            &cancel,
            io_timeout,
        )
        .await
        .map_err(|error| {
            format!("{error} (transferred_bytes={transferred} total_bytes={total})")
        })?;
        if read == 0 {
            break;
        }
        write_local_transfer_chunk(&mut destination, &buffer[..read], &cancel, io_timeout).await?;
        transferred += read as u64;
        crate::services::transfers::report_progress(app, transfer_id, transferred, total).await;
    }
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "download-read", transferred_bytes = transferred, "SFTP download stage completed");
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "local-flush", "SFTP download stage started");
    tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(TRANSFER_CANCELED.to_string()),
        result = timeout(io_timeout, destination.flush()) => result
            .map_err(|_| "刷新本地下载文件超时".to_string())?
            .map_err(|error| format!("刷新本地下载文件失败: {error}"))?,
    }
    tracing::info!(category = "transfer", scope = "transfer", %transfer_id, stage = "local-flush", "SFTP download stage completed");
    Ok(())
}

async fn replace_remote_file(
    sftp: &SftpSession,
    partial_path: &str,
    destination_path: &str,
) -> Result<(), String> {
    replace_remote_file_with_policy(sftp, partial_path, destination_path, None)
        .await
        .map(|_| ())
}

async fn replace_remote_file_with_policy(
    sftp: &SftpSession,
    partial_path: &str,
    destination_path: &str,
    replace_existing: Option<bool>,
) -> Result<bool, String> {
    let partial_metadata = sftp
        .symlink_metadata(partial_path)
        .await
        .map_err(|error| sftp_transfer_error("LSTAT checkpoint", partial_path, error))?;
    let destination_metadata = match sftp.symlink_metadata(destination_path).await {
        Ok(metadata) => Some(metadata),
        Err(error) if is_sftp_not_found(&error) => None,
        Err(error) => {
            return Err(sftp_transfer_error(
                "LSTAT destination",
                destination_path,
                error,
            ))
        }
    };

    if let Some(replace) = replace_existing {
        if !partial_metadata.file_type().is_file() {
            return Err("提交断点不是普通文件，已保留断点".into());
        }
        if destination_metadata.is_some() && !replace {
            return Ok(false);
        }
        if destination_metadata
            .as_ref()
            .is_some_and(|metadata| !metadata.file_type().is_file())
        {
            return Err("目标不是普通文件，拒绝覆盖".into());
        }
    }

    if replace_existing.is_none() && destination_metadata.as_ref().is_some_and(|destination| {
        destination.is_symlink()
            || matches!((destination.uid, partial_metadata.uid), (Some(left), Some(right)) if left != right)
    }) {
        let mut source = sftp
            .open(partial_path)
            .await
            .map_err(|error| sftp_transfer_error("OPEN checkpoint-read", partial_path, error))?;
        let mut destination = sftp
            .open_with_flags(
                destination_path,
                OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE,
            )
            .await
            .map_err(|error| sftp_transfer_error("OPEN destination-write", destination_path, error))?;
        tokio::io::copy(&mut source, &mut destination)
            .await
            .map_err(|error| sftp_transfer_error("WRITE destination-copy", destination_path, error))?;
        destination
            .flush()
            .await
            .map_err(|error| sftp_transfer_error("FLUSH destination", destination_path, error))?;
        let committed_size = sftp
            .metadata(destination_path)
            .await
            .map_err(|error| sftp_transfer_error("STAT destination-verify", destination_path, error))?
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
            .map_err(|error| sftp_transfer_error("REMOVE checkpoint", partial_path, error))?;
        return Ok(true);
    }

    if let Some(permissions) = destination_metadata
        .as_ref()
        .and_then(|metadata| metadata.permissions)
    {
        let mut metadata = SftpMetadata::empty();
        metadata.permissions = Some(permissions);
        sftp.set_metadata(partial_path, metadata)
            .await
            .map_err(|error| format!("无法保留远端文件权限，已停止替换: {error}"))?;
    }

    let backup_path = format!(
        "{destination_path}.fileterm-backup-{}",
        uuid::Uuid::new_v4()
    );
    let moved_destination = if destination_metadata.is_some() {
        sftp.rename(destination_path, &backup_path)
            .await
            .map_err(|error| {
                sftp_transfer_error(
                    "RENAME backup",
                    destination_path,
                    format!("to={backup_path:?}: {error}"),
                )
            })?;
        true
    } else {
        false
    };
    if let Err(error) = sftp.rename(partial_path, destination_path).await {
        if moved_destination {
            if let Err(rollback_error) = sftp.rename(&backup_path, destination_path).await {
                return Err(format!(
                    "SFTP RENAME path={partial_path:?} to={destination_path:?} 失败：{error}；ROLLBACK path={backup_path:?} to={destination_path:?} 失败：{rollback_error}"
                ));
            }
        }
        return Err(format!(
            "SFTP RENAME path={partial_path:?} to={destination_path:?} 失败，断点已保留：{error}"
        ));
    }
    if moved_destination {
        let _ = sftp.remove_file(&backup_path).await;
    }
    Ok(true)
}

// ─────────────────────────────────────────────────────────────────────────────
// root-mode helpers (exec channel + `sudo` / `su`)
// ─────────────────────────────────────────────────────────────────────────────

async fn relay_sftp_file(
    sftp: &SftpSession,
    request: crate::sessions::relay_io::RelayRequest,
    stream: &mut tokio::io::DuplexStream,
    cancel: &tokio_util::sync::CancellationToken,
    io_timeout: Duration,
) -> Result<(), String> {
    use crate::sessions::relay_io::copy_exact;
    if request.upload {
        ensure_transfer_parent_dir(sftp, &request.path).await?;
    }
    let metadata = sftp.symlink_metadata(&request.path).await;
    match metadata {
        Ok(metadata) if metadata.is_symlink() || metadata.is_dir() => {
            return Err("中转路径必须是普通文件".into())
        }
        Err(error) if !(request.upload && request.offset == 0 && is_sftp_not_found(&error)) => {
            return Err(error.to_string())
        }
        _ => {}
    }
    let flags = if request.upload {
        if request.offset == 0 {
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE
        } else {
            OpenFlags::WRITE
        }
    } else {
        OpenFlags::READ
    };
    let mut file = sftp
        .open_with_flags(&request.path, flags)
        .await
        .map_err(|e| e.to_string())?;
    file.seek(std::io::SeekFrom::Start(request.offset))
        .await
        .map_err(|e| e.to_string())?;
    if request.upload {
        copy_exact(stream, &mut file, request.length, cancel, io_timeout).await?;
        file.flush().await.map_err(|e| e.to_string())?;
        file.shutdown().await.map_err(|e| e.to_string())
    } else {
        copy_exact(&mut file, stream, request.length, cancel, io_timeout).await?;
        stream.shutdown().await.map_err(|e| e.to_string())
    }
}
