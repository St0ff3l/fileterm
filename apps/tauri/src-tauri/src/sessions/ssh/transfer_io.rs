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

// Standard SFTP upload/download and atomic replacement.

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
