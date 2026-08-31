fn format_size(bytes: u64) -> String {
    // 统一使用 SI 单位（1000 进制），与 ssh.rs::format_bytes / ftp.rs::format_bytes
    // 保持一致。units 数组必须包含 "B" 前缀，否则循环升级单位时索引会偏移：
    // 旧实现 units=["KB",...] 下，bytes=1000 进入循环后 value=1.0、unit_idx=1，
    // 错误地落到 "MB" 段输出 "1.0 MB"。
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0usize;
    while value >= 1000.0 && unit_idx < units.len() - 1 {
        value /= 1000.0;
        unit_idx += 1;
    }
    let decimals = if value >= 10.0 || unit_idx == 0 { 0 } else { 1 };
    format!("{:.*} {}", decimals, value, units[unit_idx])
}

fn format_modified(secs: u64) -> String {
    if secs == 0 {
        return "1970/01/01 00:00".to_string();
    }
    let mut remaining = (secs / 86400) as i64;
    let time_secs = (secs % 86400) as i64;
    let (h, m) = (time_secs / 3600, (time_secs % 3600) / 60);
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
    let mut month = 1i64;
    for &days in &md {
        if remaining < days {
            break;
        }
        remaining -= days;
        month += 1;
    }
    format!(
        "{:04}/{:02}/{:02} {:02}:{:02}",
        year,
        month,
        remaining + 1,
        h,
        m
    )
}

fn leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[cfg(unix)]
fn format_permission_bits(mode: u32, is_dir: bool) -> String {
    let segments = [
        [0o400, 0o200, 0o100],
        [0o040, 0o020, 0o010],
        [0o004, 0o002, 0o001],
    ];
    let mut s = String::with_capacity(10);
    s.push(if is_dir { 'd' } else { '-' });
    for seg in &segments {
        s.push(if mode & seg[0] != 0 { 'r' } else { '-' });
        s.push(if mode & seg[1] != 0 { 'w' } else { '-' });
        s.push(if mode & seg[2] != 0 { 'x' } else { '-' });
    }
    s
}

#[cfg(not(unix))]
fn format_permission_bits(_mode: u32, _is_dir: bool) -> String {
    String::new()
}

#[cfg(unix)]
fn file_mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode()
}

#[cfg(not(unix))]
fn file_mode(_meta: &fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn owner_group(meta: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!("{}/{}", meta.uid(), meta.gid())
}

#[cfg(not(unix))]
fn owner_group(_meta: &fs::Metadata) -> String {
    String::new()
}

fn modified_secs(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// List the drive-letter roots visible to the current user on Windows.
///
/// Uses `GetLogicalDrives` instead of probing each `X:\` with `fs::metadata`.
/// The probe form blocks for seconds on unready removable drives (empty
/// optical media, disconnected floppy/USB), which freezes the local file
/// manager whenever the user navigates to "此电脑". `GetLogicalDrives` is a
/// pure kernel32 bitmask query with no I/O, so it returns immediately and
/// never hangs. Drives that report as present but are not actually ready
/// surface an error when the user tries to open them, matching Windows
/// Explorer behavior.
#[cfg(target_os = "windows")]
fn list_windows_drive_roots() -> Vec<LocalFileItem> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    // SAFETY: `GetLogicalDrives` takes no parameters, performs no I/O, and has
    // no side effects. It returns a 32-bit bitmask where bit N (from 0)
    // corresponds to drive letter `char::from(b'A' + N as u8)`.
    let mask = unsafe { GetLogicalDrives() };
    let mut items = Vec::new();
    for index in 0u32..26 {
        if (mask & (1u32 << index)) == 0 {
            continue;
        }
        let letter = (b'A' + index as u8) as char;
        items.push(LocalFileItem {
            path: format!("{}:\\", letter),
            name: format!("{}:", letter),
            r#type: "folder".to_string(),
            modified: String::new(),
            size: "-".to_string(),
            permission: String::new(),
            owner_group: String::new(),
        });
    }
    items
}
#[tauri::command]
pub fn app_list_local_directory(dir_path: Option<String>) -> Result<DirectorySnapshot, AppError> {
    #[cfg(target_os = "windows")]
    if dir_path.as_deref() == Some(WINDOWS_DRIVES_PATH) {
        return Ok(DirectorySnapshot {
            path: WINDOWS_DRIVES_PATH.to_string(),
            items: list_windows_drive_roots(),
        });
    }

    let requested_path = match dir_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => initial_path(),
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let requested_path_text = requested_path.to_string_lossy().into_owned();

    #[cfg(target_os = "macos")]
    let root = if is_network_path(&requested_path_text) {
        resolve_mac_smb_path(&requested_path_text)
            .ok_or_else(|| smb_credentials_required(&requested_path_text, "SMB 路径尚未连接"))?
    } else {
        requested_path
    };
    #[cfg(target_os = "windows")]
    let root = network_path_as_unc(&requested_path_text)
        .map(PathBuf::from)
        .unwrap_or(requested_path);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let root = requested_path;

    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(error) => {
            crate::services::logging::error_global("local", format!("list failed error={error}"));
            #[cfg(target_os = "windows")]
            if is_network_path(&requested_path_text)
                && (error.raw_os_error() == Some(1326)
                    || (is_network_host_path(&requested_path_text)
                        && error.raw_os_error() == Some(67)))
            {
                return Err(smb_credentials_required(&requested_path_text, error));
            }
            return Err(AppError::Storage(format!(
                "Failed to read directory {}: {}",
                root.display(),
                error
            )));
        }
    };

    let mut items: Vec<LocalFileItem> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let full_path = entry.path().to_string_lossy().to_string();
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let is_dir = meta.is_dir();
        let mode = file_mode(&meta);
        items.push(LocalFileItem {
            path: full_path,
            name,
            r#type: if is_dir {
                "folder".to_string()
            } else {
                "file".to_string()
            },
            modified: format_modified(modified_secs(&meta)),
            size: if is_dir {
                "-".to_string()
            } else {
                format_size(meta.len())
            },
            permission: format_permission_bits(mode, is_dir),
            owner_group: owner_group(&meta),
        });
    }

    items.sort_by(|a, b| {
        let af = a.r#type == "folder";
        let bf = b.r#type == "folder";
        bf.cmp(&af).then_with(|| a.name.cmp(&b.name))
    });

    crate::services::logging::debug_global(
        "local",
        format!("listed directory entries={}", items.len()),
    );

    Ok(DirectorySnapshot {
        path: root.to_string_lossy().to_string(),
        items,
    })
}
