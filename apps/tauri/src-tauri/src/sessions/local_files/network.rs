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
