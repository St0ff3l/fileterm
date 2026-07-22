use crate::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

#[cfg(any(target_os = "macos", target_os = "windows"))]
const SMB_CREDENTIALS_REQUIRED: &str = "SMB_CREDENTIALS_REQUIRED";

/// Synthetic path for the Windows "This PC" drive list.
pub const WINDOWS_DRIVES_PATH: &str = "fileterm://windows-drives";

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

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LocalNetworkShareConnectionResult {
    pub kind: String,
    pub path: String,
    pub shares: Vec<String>,
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn smb_credentials_required(path: &str, detail: impl std::fmt::Display) -> AppError {
    AppError::Storage(format!("{SMB_CREDENTIALS_REQUIRED}: {path}: {detail}"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn network_path_components(path: &str) -> Option<Vec<String>> {
    let trimmed = path.trim();
    let without_prefix = if trimmed
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("smb://"))
    {
        &trimmed[6..]
    } else if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        trimmed.trim_start_matches(['\\', '/'])
    } else {
        return None;
    };

    let components: Vec<String> = without_prefix
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect();
    if components.first().is_none_or(String::is_empty)
        || components
            .iter()
            .any(|component| component == "." || component == "..")
    {
        return None;
    }
    Some(components)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn is_network_path(path: &str) -> bool {
    network_path_components(path).is_some()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_network_host_path(path: &str) -> bool {
    network_path_components(path).is_some_and(|components| components.len() == 1)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn network_path_as_unc(path: &str) -> Option<String> {
    network_path_components(path).map(|components| format!("\\\\{}", components.join("\\")))
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug)]
struct MacSmbMount {
    remote_root: String,
    local_root: PathBuf,
    mounted_paths: Vec<PathBuf>,
}

#[cfg(target_os = "macos")]
static MAC_SMB_MOUNTS: OnceLock<Mutex<Vec<MacSmbMount>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn mac_smb_mounts() -> &'static Mutex<Vec<MacSmbMount>> {
    MAC_SMB_MOUNTS.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(target_os = "macos")]
fn normalize_mac_smb_path(path: &str) -> Option<String> {
    let components = network_path_components(path)?;
    Some(format!("\\\\{}", components.join("\\")))
}

#[cfg(target_os = "macos")]
fn resolve_mac_smb_path(path: &str) -> Option<PathBuf> {
    let normalized = normalize_mac_smb_path(path)?;
    let mounts = mac_smb_mounts().lock().ok()?;
    mounts
        .iter()
        .filter_map(|mount| {
            let remote = mount.remote_root.to_ascii_lowercase();
            let requested = normalized.to_ascii_lowercase();
            if requested == remote {
                return Some((remote.len(), mount.local_root.clone(), String::new()));
            }
            let prefix = format!("{remote}\\");
            requested.strip_prefix(&prefix).map(|_| {
                (
                    remote.len(),
                    mount.local_root.clone(),
                    normalized[prefix.len()..].to_string(),
                )
            })
        })
        .max_by_key(|(length, _, _)| *length)
        .map(|(_, local_root, suffix)| {
            if suffix.is_empty() {
                local_root
            } else {
                suffix
                    .split('\\')
                    .fold(local_root, |path, component| path.join(component))
            }
        })
}

#[cfg(target_os = "macos")]
fn parse_existing_mac_smb_mount(line: &str, remote_root: &str) -> Option<PathBuf> {
    let (source, mounted) = line.split_once(" on ")?;
    if !mounted.contains(" (smbfs") {
        return None;
    }
    let source_path = source.strip_prefix("//")?;
    let source_path = source_path
        .split_once('@')
        .map_or(source_path, |(_, path)| path);
    let candidate = normalize_mac_smb_path(&format!("\\\\{}", source_path.replace('/', "\\")))?;
    if !candidate.eq_ignore_ascii_case(remote_root) {
        return None;
    }
    Some(PathBuf::from(mounted.split_once(" (")?.0))
}

#[cfg(target_os = "macos")]
fn find_existing_mac_smb_mount(remote_root: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("/sbin/mount").output().ok()?;
    let output = String::from_utf8_lossy(&output.stdout);
    output
        .lines()
        .find_map(|line| parse_existing_mac_smb_mount(line, remote_root))
}

#[cfg(target_os = "macos")]
fn register_mac_smb_mount(remote_root: String, local_root: PathBuf, mounted_paths: Vec<PathBuf>) {
    if let Ok(mut mounts) = mac_smb_mounts().lock() {
        mounts.retain(|mount| mount.remote_root != remote_root);
        mounts.push(MacSmbMount {
            remote_root,
            local_root,
            mounted_paths,
        });
    }
}

#[cfg(target_os = "macos")]
fn local_macos_smb_path(mount_root: PathBuf, components: &[String]) -> PathBuf {
    components
        .get(2..)
        .unwrap_or(&[])
        .iter()
        .fold(mount_root, |current, component| current.join(component))
}

#[cfg(target_os = "macos")]
fn run_macos_smb_command(
    program: &str,
    args: &[String],
    password: &str,
) -> Result<(i32, String), AppError> {
    use std::ffi::CString;

    let c_program =
        CString::new(program).map_err(|_| AppError::Command("SMB 命令路径无效".to_string()))?;
    let c_args: Vec<CString> = args
        .iter()
        .map(|arg| {
            CString::new(arg.as_str())
                .map_err(|_| AppError::Command("SMB 参数包含无效字符".to_string()))
        })
        .collect::<Result<_, _>>()?;
    let mut argv: Vec<*const libc::c_char> = Vec::with_capacity(c_args.len() + 2);
    argv.push(c_program.as_ptr());
    argv.extend(c_args.iter().map(|arg| arg.as_ptr()));
    argv.push(std::ptr::null());

    let mut master_fd = -1;
    let pid = unsafe {
        libc::forkpty(
            &mut master_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if pid < 0 {
        return Err(AppError::Command("无法创建 SMB 认证终端".to_string()));
    }
    if pid == 0 {
        unsafe {
            libc::execvp(c_program.as_ptr(), argv.as_ptr());
            libc::_exit(127);
        }
    }

    let flags = unsafe { libc::fcntl(master_fd, libc::F_GETFL) };
    if flags >= 0 {
        unsafe {
            libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }

    let mut output = Vec::new();
    let mut buffer = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(20);
    let password_fallback_deadline = Instant::now() + Duration::from_secs(2);
    let password_input = {
        let mut input = password.as_bytes().to_vec();
        input.push(b'\n');
        input
    };
    let mut password_sent = false;
    let mut timed_out = false;
    let send_password = || {
        let mut offset = 0;
        while offset < password_input.len() {
            let written = unsafe {
                libc::write(
                    master_fd,
                    password_input[offset..].as_ptr().cast(),
                    password_input.len() - offset,
                )
            };
            if written <= 0 {
                return false;
            }
            offset += written as usize;
        }
        true
    };
    loop {
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }

        let mut poll_fd = libc::pollfd {
            fd: master_fd,
            events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
            revents: 0,
        };
        let poll_result = unsafe { libc::poll(&mut poll_fd, 1, 100) };
        if poll_result < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            break;
        }
        if poll_result == 0 {
            if !password_sent && Instant::now() >= password_fallback_deadline {
                password_sent = send_password();
            }
            continue;
        }

        let read = unsafe { libc::read(master_fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        if read > 0 {
            output.extend_from_slice(&buffer[..read as usize]);
            if !password_sent {
                let prompt = String::from_utf8_lossy(&output).to_ascii_lowercase();
                if prompt.contains("password") || prompt.contains("passphrase") {
                    password_sent = send_password();
                }
            }
            continue;
        }
        if read < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EAGAIN)
                || error.raw_os_error() == Some(libc::EWOULDBLOCK)
            {
                continue;
            }
        }
        break;
    }

    let mut status = 0;
    unsafe {
        if timed_out {
            libc::kill(pid, libc::SIGKILL);
        }
        libc::waitpid(pid, &mut status, 0);
        libc::close(master_fd);
    }
    if timed_out {
        return Err(AppError::Storage(
            "SMB 连接超时，请确认服务器可访问后重试。".to_string(),
        ));
    }
    let exit_code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        1
    };
    Ok((exit_code, String::from_utf8_lossy(&output).into_owned()))
}

#[cfg(target_os = "macos")]
fn macos_smb_failure() -> AppError {
    AppError::Storage("SMB 连接失败，请检查服务器、共享名、用户名和密码。".to_string())
}

#[cfg(target_os = "macos")]
fn macos_smb_failure_with_output(output: &str) -> AppError {
    let normalized_output = output.replace('\r', " ");
    let detail = normalized_output
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default();
    if detail.is_empty() {
        macos_smb_failure()
    } else {
        AppError::Storage(format!("SMB 连接失败：{detail}"))
    }
}

#[cfg(target_os = "macos")]
fn mount_macos_smb_share(
    host: &str,
    share: &str,
    username: &str,
    password: &str,
    mount_path: &Path,
) -> Result<(), AppError> {
    fs::create_dir_all(mount_path).map_err(|error| AppError::Storage(error.to_string()))?;
    let remote = format!("//{username}@{host}/{share}");
    let args = vec![
        "-t".to_string(),
        "smbfs".to_string(),
        "-o".to_string(),
        "nobrowse".to_string(),
        remote,
        mount_path.to_string_lossy().into_owned(),
    ];
    let (exit_code, output) = run_macos_smb_command("/sbin/mount", &args, password)?;
    if exit_code != 0 {
        return Err(macos_smb_failure_with_output(&output));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn parse_macos_smb_shares(output: &str) -> Vec<String> {
    let mut shares = Vec::new();
    for line in output.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() >= 2
            && columns[1].eq_ignore_ascii_case("disk")
            && !columns[0].eq_ignore_ascii_case("share")
        {
            let share = columns[0].to_string();
            if !shares.iter().any(|existing| existing == &share) {
                shares.push(share);
            }
        }
    }
    shares
}

#[cfg(target_os = "macos")]
fn connect_macos_smb(
    path: &str,
    username: &str,
    password: &str,
    selected_share: Option<&str>,
) -> Result<LocalNetworkShareConnectionResult, AppError> {
    let components = network_path_components(path).ok_or_else(|| {
        AppError::Storage("SMB 路径无效，请使用 \\\\服务器 或 \\\\服务器\\共享名。".to_string())
    })?;
    if username.trim().is_empty() || password.is_empty() {
        return Err(AppError::Storage("请输入 SMB 用户名和密码。".to_string()));
    }

    let host = &components[0];
    let mount_root = std::env::temp_dir().join(format!("fileterm-smb-{}", uuid::Uuid::new_v4()));
    let share = components
        .get(1)
        .map(String::as_str)
        .or(selected_share.map(str::trim));
    if let Some(share) = share {
        if share.is_empty() || share.contains(['/', '\\']) || share == "." || share == ".." {
            return Err(AppError::Storage("SMB 共享目录名称无效。".to_string()));
        }
        let remote_root = normalize_mac_smb_path(&format!("\\\\{host}\\{share}"))
            .ok_or_else(macos_smb_failure)?;
        if let Some(mapped) = resolve_mac_smb_path(&remote_root) {
            return Ok(LocalNetworkShareConnectionResult {
                kind: "connected".to_string(),
                path: local_macos_smb_path(mapped, &components)
                    .to_string_lossy()
                    .into_owned(),
                shares: Vec::new(),
            });
        }
        if let Some(mapped) = find_existing_mac_smb_mount(&remote_root) {
            register_mac_smb_mount(remote_root.clone(), mapped.clone(), vec![mapped.clone()]);
            return Ok(LocalNetworkShareConnectionResult {
                kind: "connected".to_string(),
                path: local_macos_smb_path(mapped, &components)
                    .to_string_lossy()
                    .into_owned(),
                shares: Vec::new(),
            });
        }
        if let Err(error) =
            mount_macos_smb_share(host, share, username.trim(), password, &mount_root)
        {
            let _ = std::process::Command::new("/sbin/umount")
                .arg(&mount_root)
                .status();
            let _ = fs::remove_dir_all(&mount_root);
            return Err(error);
        }
        register_mac_smb_mount(remote_root, mount_root.clone(), vec![mount_root.clone()]);
        let path = local_macos_smb_path(mount_root, &components);
        return Ok(LocalNetworkShareConnectionResult {
            kind: "connected".to_string(),
            path: path.to_string_lossy().into_owned(),
            shares: Vec::new(),
        });
    }

    let view_target = format!("//{}@{}", username.trim(), host);
    let view_args = vec!["view".to_string(), view_target];
    let (exit_code, output) = run_macos_smb_command("/usr/bin/smbutil", &view_args, password)?;
    if exit_code != 0 {
        return Err(macos_smb_failure_with_output(&output));
    }
    let shares = parse_macos_smb_shares(&output);
    if shares.is_empty() {
        return Err(AppError::Storage(
            "SMB 服务器没有可访问的共享目录。".to_string(),
        ));
    }

    let _ = fs::remove_dir_all(&mount_root);
    Ok(LocalNetworkShareConnectionResult {
        kind: "select-share".to_string(),
        path: path.to_string(),
        shares,
    })
}

#[cfg(target_os = "macos")]
pub fn cleanup_network_mounts() {
    let mounts = mac_smb_mounts()
        .lock()
        .map(|mut mounts| std::mem::take(&mut *mounts))
        .unwrap_or_default();
    for mount in mounts.into_iter().rev() {
        for mounted_path in mount.mounted_paths.into_iter().rev() {
            let _ = std::process::Command::new("/sbin/umount")
                .arg(mounted_path)
                .status();
        }
        let _ = fs::remove_dir_all(&mount.local_root);
    }
}

#[cfg(target_os = "windows")]
fn connect_windows_smb(
    path: &str,
    username: &str,
    password: &str,
    selected_share: Option<&str>,
) -> Result<LocalNetworkShareConnectionResult, AppError> {
    use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NetApiBufferFree, MAX_PREFERRED_LENGTH,
    };
    use windows_sys::Win32::NetworkManagement::WNet::{
        WNetAddConnection2W, CONNECT_TEMPORARY, NETRESOURCEW, RESOURCETYPE_ANY, RESOURCETYPE_DISK,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        NetShareEnum, SHARE_INFO_1, STYPE_DISKTREE, STYPE_MASK, STYPE_SPECIAL,
    };

    let components = network_path_components(path).ok_or_else(|| {
        AppError::Storage("SMB 路径无效，请使用 \\\\服务器 或 \\\\服务器\\共享名。".to_string())
    })?;
    if username.trim().is_empty() || password.is_empty() {
        return Err(AppError::Storage("请输入 SMB 用户名和密码。".to_string()));
    }
    let host = &components[0];
    let requested_share = components
        .get(1)
        .map(String::as_str)
        .or(selected_share.map(str::trim));
    if let Some(share) = requested_share {
        if share.is_empty() || share.contains(['/', '\\']) || share == "." || share == ".." {
            return Err(AppError::Storage("SMB 共享目录名称无效。".to_string()));
        }
    }

    // A bare UNC host is not a directory on Windows (read_dir returns
    // ERROR_BAD_NET_NAME / 67). Authenticate against IPC$ first, then ask the
    // server for the disk shares the user may open. A concrete share still
    // connects directly so existing \\server\\share navigation is unchanged.
    let remote = match requested_share {
        Some(share) => format!("\\\\{host}\\{share}"),
        None => format!("\\\\{host}\\IPC$"),
    };
    let mut remote_w: Vec<u16> = remote.encode_utf16().chain(std::iter::once(0)).collect();
    let server_w: Vec<u16> = format!("\\\\{host}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let username_w: Vec<u16> = username
        .trim()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let password_w: Vec<u16> = password.encode_utf16().chain(std::iter::once(0)).collect();
    let resource = NETRESOURCEW {
        dwType: if requested_share.is_some() {
            RESOURCETYPE_DISK
        } else {
            RESOURCETYPE_ANY
        },
        lpRemoteName: remote_w.as_mut_ptr(),
        ..Default::default()
    };
    let result = unsafe {
        WNetAddConnection2W(
            &resource,
            password_w.as_ptr(),
            username_w.as_ptr(),
            CONNECT_TEMPORARY,
        )
    };
    if result != NO_ERROR {
        return Err(AppError::Storage(format!(
            "SMB 连接失败（Windows 错误码 {result}），请检查服务器、共享名、用户名和密码。"
        )));
    }

    if requested_share.is_none() {
        let mut shares = Vec::new();
        let mut resume_handle = 0u32;
        loop {
            let mut buffer = std::ptr::null_mut();
            let mut entries_read = 0u32;
            let mut total_entries = 0u32;
            let status = unsafe {
                NetShareEnum(
                    server_w.as_ptr(),
                    1,
                    &mut buffer,
                    MAX_PREFERRED_LENGTH,
                    &mut entries_read,
                    &mut total_entries,
                    &mut resume_handle,
                )
            };
            if status != NO_ERROR && status != ERROR_MORE_DATA {
                if !buffer.is_null() {
                    unsafe {
                        NetApiBufferFree(buffer.cast());
                    }
                }
                return Err(AppError::Storage(format!(
                    "无法读取 SMB 共享目录（Windows 错误码 {status}）。"
                )));
            }

            if !buffer.is_null() {
                let entries = unsafe {
                    std::slice::from_raw_parts(buffer.cast::<SHARE_INFO_1>(), entries_read as usize)
                };
                for entry in entries {
                    let is_disk_share = entry.shi1_type & STYPE_MASK == STYPE_DISKTREE;
                    let is_special_share = entry.shi1_type & STYPE_SPECIAL != 0;
                    if !is_disk_share || is_special_share || entry.shi1_netname.is_null() {
                        continue;
                    }
                    let mut length = 0usize;
                    unsafe {
                        while *entry.shi1_netname.add(length) != 0 {
                            length += 1;
                        }
                    }
                    let share = String::from_utf16_lossy(unsafe {
                        std::slice::from_raw_parts(entry.shi1_netname, length)
                    });
                    if !share.is_empty() && !shares.iter().any(|existing| existing == &share) {
                        shares.push(share);
                    }
                }
                unsafe {
                    NetApiBufferFree(buffer.cast());
                }
            }

            if status != ERROR_MORE_DATA {
                break;
            }
        }
        shares.sort_unstable_by_key(|share| share.to_ascii_lowercase());
        if shares.is_empty() {
            return Err(AppError::Storage(
                "SMB 服务器没有可访问的共享目录。".to_string(),
            ));
        }
        return Ok(LocalNetworkShareConnectionResult {
            kind: "select-share".to_string(),
            path: format!("\\\\{host}"),
            shares,
        });
    }

    Ok(LocalNetworkShareConnectionResult {
        kind: "connected".to_string(),
        path: components
            .get(2..)
            .filter(|tail| !tail.is_empty())
            .map(|tail| format!("{}\\{}", remote, tail.join("\\")))
            .unwrap_or(remote),
        shares: Vec::new(),
    })
}

#[tauri::command]
pub async fn app_connect_local_network_share(
    path: String,
    username: String,
    password: String,
    share: Option<String>,
) -> Result<LocalNetworkShareConnectionResult, AppError> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            connect_macos_smb(&path, &username, &password, share.as_deref())
        }
        #[cfg(target_os = "windows")]
        {
            connect_windows_smb(&path, &username, &password, share.as_deref())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (path, username, password, share);
            Err(AppError::Storage(
                "当前平台不支持通过 SMB 凭据连接网络路径。".to_string(),
            ))
        }
    })
    .await
    .map_err(|error| AppError::Command(format!("SMB 连接任务失败: {error}")))?
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
    #[cfg(target_os = "windows")]
    if dir_path.as_deref() == Some(WINDOWS_DRIVES_PATH) {
        let mut items = Vec::new();
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            if fs::metadata(&drive).is_ok() {
                items.push(LocalFileItem {
                    path: drive,
                    name: format!("{}:", letter as char),
                    r#type: "folder".to_string(),
                    modified: String::new(),
                    size: "-".to_string(),
                    permission: String::new(),
                    owner_group: String::new(),
                });
            }
        }
        return Ok(DirectorySnapshot {
            path: WINDOWS_DRIVES_PATH.to_string(),
            items,
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[cfg(test)]
mod smb_tests {
    #[cfg(target_os = "macos")]
    use std::path::PathBuf;

    use super::network_path_components;

    #[test]
    fn parses_unc_and_smb_paths_without_traversal_components() {
        assert_eq!(
            network_path_components(r"\\server\share\folder"),
            Some(
                vec!["server", "share", "folder"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
        assert_eq!(
            network_path_components("smb://server/share"),
            Some(
                vec!["server", "share"]
                    .into_iter()
                    .map(String::from)
                    .collect()
            )
        );
        assert!(network_path_components(r"\\server\share\..\secret").is_none());
    }

    #[test]
    fn recognizes_a_bare_unc_host_for_share_selection() {
        assert!(super::is_network_host_path(r"\\server"));
        assert!(super::is_network_host_path("smb://server"));
        assert!(!super::is_network_host_path(r"\\server\share"));
    }

    #[test]
    fn normalizes_smb_urls_to_windows_unc_paths() {
        assert_eq!(
            super::network_path_as_unc("smb://server/share/folder"),
            Some(r"\\server\share\folder".to_string())
        );
        assert_eq!(
            super::network_path_as_unc(r"\\server\share"),
            Some(r"\\server\share".to_string())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sends_mac_smb_password_through_the_pty() {
        let args = vec![
            "-c".to_string(),
            "printf 'Password: '; read value; printf 'accepted:%s\\n' \"$value\"".to_string(),
        ];
        let (exit_code, output) = super::run_macos_smb_command("/bin/sh", &args, "secret").unwrap();
        assert_eq!(exit_code, 0);
        assert!(output.contains("accepted:secret"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn host_only_smb_path_keeps_the_mount_root() {
        let components = vec!["100.100.10.2".to_string()];
        assert_eq!(
            super::local_macos_smb_path(PathBuf::from("/tmp/fileterm-smb"), &components),
            PathBuf::from("/tmp/fileterm-smb")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn reuses_existing_mount_for_a_selected_share() {
        assert_eq!(
            super::parse_existing_mac_smb_mount(
                "//Stoffel@100.100.10.2/fnOSNAS_CN on /private/var/tmp/fileterm-smb (smbfs, nodev)",
                r"\\100.100.10.2\fnOSNAS_CN"
            ),
            Some(PathBuf::from("/private/var/tmp/fileterm-smb"))
        );
    }
}
