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
