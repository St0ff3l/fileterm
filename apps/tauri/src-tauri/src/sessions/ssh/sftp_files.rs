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
