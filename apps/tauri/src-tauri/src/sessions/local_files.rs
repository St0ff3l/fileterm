use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalFileItem {
    pub path: String,
    pub name: String,
    pub r#type: String,
    pub modified: String,
    pub size: String,
    pub permission: String,
    pub owner_group: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirectorySnapshot {
    pub path: String,
    pub items: Vec<LocalFileItem>,
}

#[derive(Clone, Copy, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PermissionApplyTarget {
    All,
    Files,
    Directories,
}

impl PermissionApplyTarget {
    fn includes(self, is_directory: bool) -> bool {
        matches!(self, Self::All)
            || matches!(self, Self::Files) && !is_directory
            || matches!(self, Self::Directories) && is_directory
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionChangeOptions {
    mode: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    apply_to: Option<PermissionApplyTarget>,
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn initial_path() -> PathBuf {
    home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_idx = 0usize;
    while value >= 1000.0 && unit_idx < units.len() - 1 {
        value /= 1000.0;
        unit_idx += 1;
    }
    let decimals = if value >= 10.0 { 0 } else { 1 };
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

#[tauri::command]
pub fn app_list_local_directory(dir_path: Option<String>) -> Result<DirectorySnapshot, AppError> {
    let root = match dir_path {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => initial_path(),
    };

    let entries = match fs::read_dir(&root) {
        Ok(e) => e,
        Err(error) => {
            crate::services::logging::error_global("local", format!("list failed error={error}"));
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
        let meta = entry
            .metadata()
            .map_err(|e| AppError::Storage(e.to_string()))?;
        if meta.is_dir() {
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

#[tauri::command]
pub async fn app_select_local_files(
    _app: AppHandle,
    default_path: Option<String>,
) -> Result<Vec<String>, AppError> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    // 不加 "All files" filter（&["*"] 在某些平台不匹配任何文件，导致
    // 对话框里所有文件灰显不可选——用户报告"点上传选不到任何文件"）。
    // 不加 filter 默认显示所有文件。
    let result = dialog.pick_files().await.unwrap_or_default();
    Ok(result
        .into_iter()
        .map(|h| h.path().to_string_lossy().into_owned())
        .collect())
}

#[tauri::command]
pub async fn app_select_local_directory(
    _app: AppHandle,
    default_path: Option<String>,
) -> Result<Option<String>, AppError> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(p) = default_path {
        dialog = dialog.set_directory(p);
    }
    let result = dialog.pick_folder().await;
    Ok(result.map(|h| h.path().to_string_lossy().into_owned()))
}

// ── Encoding helpers ────────────────────────────────────────────────────────

fn encoding_for(label: &str) -> &'static encoding_rs::Encoding {
    encoding_rs::Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::UTF_8)
}

fn decode_bytes(bytes: &[u8], encoding: &str) -> String {
    let enc = encoding_for(encoding);
    let (cow, _, _) = enc.decode(bytes);
    cow.into_owned()
}

fn encode_text(text: &str, encoding: &str) -> Vec<u8> {
    let enc = encoding_for(encoding);
    let (cow, _, _) = enc.encode(text);
    cow.into_owned()
}

#[cfg(test)]
mod permission_tests {
    #[cfg(unix)]
    use super::app_change_local_permissions;
    use super::{LocalFileItem, PermissionApplyTarget, PermissionChangeOptions};

    #[test]
    fn local_file_items_serialize_with_core_camel_case_fields() {
        let item = LocalFileItem {
            path: "/tmp/demo".to_string(),
            name: "demo".to_string(),
            r#type: "file".to_string(),
            modified: "-".to_string(),
            size: "1 B".to_string(),
            permission: "0644".to_string(),
            owner_group: "user:staff".to_string(),
        };
        let value = serde_json::to_value(item).unwrap();
        assert_eq!(value["ownerGroup"], "user:staff");
        assert!(value.get("owner_group").is_none());
    }

    #[test]
    fn reads_camel_case_apply_to() {
        let options: PermissionChangeOptions = serde_json::from_value(serde_json::json!({
            "mode": "644",
            "recursive": true,
            "applyTo": "files"
        }))
        .expect("camelCase local permission options should deserialize");
        assert_eq!(options.apply_to, Some(PermissionApplyTarget::Files));

        let snake_case = serde_json::from_value::<PermissionChangeOptions>(serde_json::json!({
            "mode": "644",
            "recursive": true,
            "apply_to": "files"
        }));
        assert!(snake_case.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn recursive_files_only_preserves_directory_traverse_bits() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "fileterm-local-permissions-{}",
            uuid::Uuid::new_v4()
        ));
        let nested = root.join("nested");
        let file = nested.join("config.txt");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(&file, b"config").unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();

        app_change_local_permissions(
            root.to_string_lossy().into_owned(),
            PermissionChangeOptions {
                mode: "644".to_string(),
                recursive: true,
                apply_to: Some(PermissionApplyTarget::Files),
            },
        )
        .unwrap();

        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode(&root), 0o755);
        assert_eq!(mode(&nested), 0o755);
        assert_eq!(mode(&file), 0o644);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recursive_permissions_do_not_follow_directory_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = std::env::temp_dir().join(format!(
            "fileterm-local-permission-symlink-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "fileterm-local-permission-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("keep.txt");
        std::fs::write(&outside_file, b"keep").unwrap();
        std::fs::set_permissions(&outside_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&outside, root.join("outside-link")).unwrap();

        app_change_local_permissions(
            root.to_string_lossy().into_owned(),
            PermissionChangeOptions {
                mode: "644".to_string(),
                recursive: true,
                apply_to: Some(PermissionApplyTarget::Files),
            },
        )
        .unwrap();

        assert_eq!(
            std::fs::metadata(&outside_file)
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o600
        );
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}
