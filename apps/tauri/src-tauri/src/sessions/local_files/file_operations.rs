#[tauri::command]
pub fn app_read_local_file(
    file_path: String,
    encoding: Option<String>,
) -> Result<String, AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    let bytes = fs::read(&file_path).map_err(|error| {
        crate::services::logging::error_global("local", format!("read failed error={error}"));
        AppError::Storage(error.to_string())
    })?;
    crate::services::logging::debug_global(
        "local",
        format!("read file bytes={} encoding={enc}", bytes.len()),
    );
    Ok(decode_bytes(&bytes, &enc))
}
#[tauri::command]
pub fn app_write_local_file(
    file_path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let enc = encoding.unwrap_or_else(|| "utf-8".to_string());
    let bytes = encode_text(&content, &enc);
    if let Some(parent) = Path::new(&file_path).parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    let byte_count = bytes.len();
    let result = fs::write(&file_path, bytes).map_err(|e| AppError::Storage(e.to_string()));
    log_local_result("write file", &result, Some(byte_count));
    result
}

#[tauri::command]
pub fn app_create_local_directory(dir_path: String, name: String) -> Result<(), AppError> {
    let target = Path::new(&dir_path).join(&name);
    let result = fs::create_dir_all(&target).map_err(|e| AppError::Storage(e.to_string()));
    log_local_result("create directory", &result, None);
    result
}

#[tauri::command]
pub fn app_create_local_file(dir_path: String, name: String) -> Result<(), AppError> {
    let target = Path::new(&dir_path).join(&name);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    let result = fs::write(&target, b"").map_err(|e| AppError::Storage(e.to_string()));
    log_local_result("create file", &result, Some(0));
    result
}

#[tauri::command]
pub fn app_copy_local_path(source_path: String, destination_path: String) -> Result<(), AppError> {
    if source_path == destination_path {
        return Ok(());
    }
    if let Some(parent) = Path::new(&destination_path).parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    let result = copy_recursive(Path::new(&source_path), Path::new(&destination_path));
    log_local_result("copy path", &result, None);
    result
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    let meta = fs::metadata(src).map_err(|e| AppError::Storage(e.to_string()))?;
    if meta.is_dir() {
        copy_dir_recursive(src, dst)
    } else {
        fs::copy(src, dst).map_err(|e| AppError::Storage(e.to_string()))?;
        Ok(())
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    fs::create_dir_all(dst).map_err(|e| AppError::Storage(e.to_string()))?;
    for entry in fs::read_dir(src).map_err(|e| AppError::Storage(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::Storage(e.to_string()))?;
        let name = entry.file_name();
        let src_child = entry.path();
        let dst_child = dst.join(&name);
        // Use `file_type()` (symlink-aware, does not follow) instead of
        // `metadata()` (follows symlinks). Following a symlinked directory
        // here would recurse into a loop and fill the disk; skipping
        // symlinks matches `apply_permissions_recursive` below.
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Storage(e.to_string()))?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_dir_recursive(&src_child, &dst_child)?;
        } else {
            fs::copy(&src_child, &dst_child).map_err(|e| AppError::Storage(e.to_string()))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn app_move_local_path(source_path: String, destination_path: String) -> Result<(), AppError> {
    if source_path == destination_path {
        return Ok(());
    }
    if let Some(parent) = Path::new(&destination_path).parent() {
        fs::create_dir_all(parent).map_err(|e| AppError::Storage(e.to_string()))?;
    }
    let result = match fs::rename(&source_path, &destination_path) {
        Ok(()) => Ok(()),
        Err(error) => {
            if error.raw_os_error() == Some(18) {
                // EXDEV — cross-device rename
                copy_recursive(Path::new(&source_path), Path::new(&destination_path))?;
                remove_path(Path::new(&source_path))
            } else {
                Err(AppError::Storage(error.to_string()))
            }
        }
    };
    log_local_result("move path", &result, None);
    result
}

#[tauri::command]
pub fn app_rename_local_path(target_path: String, new_name: String) -> Result<(), AppError> {
    let parent = Path::new(&target_path)
        .parent()
        .ok_or_else(|| AppError::Storage("Cannot rename root".to_string()))?;
    let dest = parent.join(&new_name);
    let result = fs::rename(&target_path, &dest).map_err(|e| AppError::Storage(e.to_string()));
    log_local_result("rename path", &result, None);
    result
}

#[tauri::command]
pub fn app_delete_local_path(target_path: String) -> Result<(), AppError> {
    let result = remove_path(Path::new(&target_path));
    log_local_result("delete path", &result, None);
    result
}

fn log_local_result(operation: &str, result: &Result<(), AppError>, bytes: Option<usize>) {
    match result {
        Ok(()) => crate::services::logging::info_global(
            "local",
            bytes.map_or_else(
                || format!("{operation} completed"),
                |count| format!("{operation} completed bytes={count}"),
            ),
        ),
        Err(error) => crate::services::logging::error_global(
            "local",
            format!("{operation} failed error={error}"),
        ),
    }
}

fn remove_path(p: &Path) -> Result<(), AppError> {
    let meta = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return Ok(());
            }
            return Err(AppError::Storage(e.to_string()));
        }
    };
    if meta.is_dir() {
        fs::remove_dir_all(p).map_err(|e| AppError::Storage(e.to_string()))
    } else {
        fs::remove_file(p).map_err(|e| AppError::Storage(e.to_string()))
    }
}

#[tauri::command]
pub fn app_change_local_permissions(
    target_path: String,
    options: PermissionChangeOptions,
) -> Result<(), AppError> {
    let mode = parse_mode(&options.mode)?;
    if !options.recursive {
        return apply_permissions(&target_path, mode);
    }

    let meta = fs::symlink_metadata(&target_path).map_err(|e| AppError::Storage(e.to_string()))?;
    if meta.file_type().is_symlink() {
        return Err(AppError::Storage(
            "递归修改权限不允许以符号链接作为根路径".to_string(),
        ));
    }
    let apply_to = options.apply_to.unwrap_or(PermissionApplyTarget::All);
    if apply_to.includes(meta.is_dir()) {
        apply_permissions(&target_path, mode)?;
    }
    if !meta.is_dir() {
        return Ok(());
    }
    apply_permissions_recursive(&target_path, mode, apply_to)
}

fn parse_mode(mode: &str) -> Result<u32, AppError> {
    let trimmed = mode.trim();
    if !trimmed.chars().all(|c| ('0'..='7').contains(&c)) || !(3..=4).contains(&trimmed.len()) {
        return Err(AppError::Storage(
            "权限值必须是 3 到 4 位八进制数字，例如 755".to_string(),
        ));
    }
    u32::from_str_radix(trimmed, 8).map_err(|e| AppError::Storage(e.to_string()))
}

#[cfg(unix)]
fn apply_permissions(path: &str, mode: u32) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| AppError::Storage(e.to_string()))
}

#[cfg(not(unix))]
fn apply_permissions(_path: &str, _mode: u32) -> Result<(), AppError> {
    Ok(())
}

#[cfg(unix)]
fn apply_permissions_recursive(
    target: &str,
    mode: u32,
    apply_to: PermissionApplyTarget,
) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt;
    let entries = fs::read_dir(target).map_err(|e| AppError::Storage(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let is_dir = meta.is_dir();
        if apply_to.includes(is_dir) {
            fs::set_permissions(&path, fs::Permissions::from_mode(mode))
                .map_err(|e| AppError::Storage(e.to_string()))?;
        }
        if is_dir {
            apply_permissions_recursive(&path.to_string_lossy(), mode, apply_to)?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_permissions_recursive(
    _target: &str,
    _mode: u32,
    _apply_to: PermissionApplyTarget,
) -> Result<(), AppError> {
    Ok(())
}
