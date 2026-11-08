fn write_editor_staging(
    staged: &Path,
    target: &Path,
    original: Option<&fs::Metadata>,
    bytes: &[u8],
) -> Result<(), AppError> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(staged)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    #[cfg(windows)]
    if let Some(metadata) = original {
        drop(file);
        copy_editor_windows_access(target, staged, metadata)?;
        file = fs::OpenOptions::new()
            .write(true)
            .open(staged)
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    #[cfg(unix)]
    if let Some(metadata) = original {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        let current = file
            .metadata()
            .map_err(|error| AppError::Storage(error.to_string()))?;
        if (current.uid(), current.gid()) != (metadata.uid(), metadata.gid())
            && unsafe { libc::fchown(file.as_raw_fd(), metadata.uid(), metadata.gid()) } != 0
        {
            return Err(AppError::Storage(format!(
                "无法保留原文件属主或组，已停止保存: {}",
                std::io::Error::last_os_error()
            )));
        }
        copy_editor_access_metadata(
            &fs::File::open(target).map_err(|error| AppError::Storage(error.to_string()))?,
            &file,
        )?;
        file.set_permissions(metadata.permissions())
            .map_err(|error| AppError::Storage(error.to_string()))?;
    }
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| AppError::Storage(error.to_string()))
}

#[cfg(target_os = "macos")]
fn copy_editor_access_metadata(source: &fs::File, target: &fs::File) -> Result<(), AppError> {
    use std::os::fd::AsRawFd;
    if unsafe {
        libc::fcopyfile(
            source.as_raw_fd(),
            target.as_raw_fd(),
            std::ptr::null_mut(),
            libc::COPYFILE_ACL | libc::COPYFILE_XATTR,
        )
    } != 0
    {
        return Err(AppError::Storage(format!(
            "无法保留原文件访问控制，已停止保存: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_editor_access_metadata(source: &fs::File, target: &fs::File) -> Result<(), AppError> {
    use std::os::fd::AsRawFd;
    let name = c"system.posix_acl_access";
    let size =
        unsafe { libc::fgetxattr(source.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EOPNOTSUPP) {
            return Ok(());
        }
        if error.raw_os_error() == Some(libc::ENODATA) {
            // An inherited staging ACL must not gain access when the original mode is restored.
            if unsafe { libc::fremovexattr(target.as_raw_fd(), name.as_ptr()) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ENODATA) {
                    return Err(AppError::Storage(error.to_string()));
                }
            }
            return Ok(());
        }
        return Err(AppError::Storage(error.to_string()));
    }
    if size > 64 * 1024 {
        return Err(AppError::Storage("原文件访问控制大小超过限制".into()));
    }
    let mut acl = vec![0_u8; size as usize];
    let size = unsafe {
        libc::fgetxattr(
            source.as_raw_fd(),
            name.as_ptr(),
            acl.as_mut_ptr().cast(),
            acl.len(),
        )
    };
    if size < 0
        || unsafe {
            libc::fsetxattr(
                target.as_raw_fd(),
                name.as_ptr(),
                acl.as_ptr().cast(),
                size as usize,
                0,
            )
        } != 0
    {
        return Err(AppError::Storage(format!(
            "无法保留原文件访问控制，已停止保存: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn commit_editor_staging(staged: &Path, target: &Path) -> Result<(), AppError> {
    crate::storage::replace_file_atomically(staged, target)
}

#[cfg(windows)]
fn commit_editor_staging(staged: &Path, target: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    if !target.exists() {
        return fs::rename(staged, target).map_err(|error| AppError::Storage(error.to_string()));
    }
    let backup = target.with_file_name(format!(".fileterm-backup-{}", uuid::Uuid::new_v4()));
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    // ReplaceFileW retains DACLs and streams; do not ignore metadata merge errors.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            wide(target).as_ptr(),
            wide(staged).as_ptr(),
            wide(&backup).as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        if !target.exists() && backup.exists() {
            fs::rename(&backup, target).map_err(|restore| {
                AppError::Storage(format!(
                    "{error}; 恢复失败，原文件位于 {}: {restore}",
                    backup.display()
                ))
            })?;
        }
        return Err(AppError::Storage(error.to_string()));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

#[cfg(windows)]
fn copy_editor_windows_access(
    source: &Path,
    target: &Path,
    metadata: &fs::Metadata,
) -> Result<(), AppError> {
    use std::os::windows::{ffi::OsStrExt, fs::MetadataExt};
    use windows_sys::Win32::{
        Security::{
            GetFileSecurityW, SetFileSecurityW, DACL_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{EncryptFileW, FILE_ATTRIBUTE_ENCRYPTED},
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source = wide(source);
    let target = wide(target);
    let mut needed = 0;
    unsafe {
        GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 || needed > 1024 * 1024 {
        return Err(AppError::Storage(
            "无法读取原文件访问控制，已停止保存".into(),
        ));
    }
    let mut descriptor = vec![0_usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetFileSecurityW(
            source.as_ptr(),
            DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(AppError::Storage(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    // Protect the empty staging file from broader inherited directory permissions before writing any bytes.
    if unsafe {
        SetFileSecurityW(
            target.as_ptr(),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor.as_mut_ptr().cast(),
        )
    } == 0
    {
        return Err(AppError::Storage(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    if metadata.file_attributes() & FILE_ATTRIBUTE_ENCRYPTED != 0
        && unsafe { EncryptFileW(target.as_ptr()) } == 0
    {
        return Err(AppError::Storage(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(())
}
