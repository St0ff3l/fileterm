fn validate_local_name(name: &str) -> Result<(), AppError> {
    use std::path::Component;

    if name.is_empty() || name.contains('/') || name.chars().any(|character| character.is_control())
    {
        return Err(AppError::Storage(
            "名称必须是当前目录中的单个有效文件名".to_string(),
        ));
    }
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(AppError::Storage(
            "名称必须是当前目录中的单个有效文件名".to_string(),
        ));
    }

    #[cfg(windows)]
    {
        if name.ends_with(['.', ' '])
            || name.chars().any(|character| "\\:<>\"|?*".contains(character))
            || is_windows_device_name(name)
        {
            return Err(AppError::Storage(
                "名称包含 Windows 不支持的文件名字符或设备名".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_device_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

fn validate_copy_destination(src: &Path, dst: &Path) -> Result<(), AppError> {
    reject_unsafe_copy_destination(dst)?;
    let source = fs::canonicalize(src).map_err(|e| AppError::Storage(e.to_string()))?;
    let destination_exists = dst.exists();
    let destination = if destination_exists {
        fs::canonicalize(dst)
    } else {
        let parent = dst
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::canonicalize(parent).map(|parent| parent.join(dst.file_name().unwrap_or_default()))
    }
    .map_err(|e| AppError::Storage(e.to_string()))?;
    if source == destination || (source.is_dir() && destination.starts_with(&source)) {
        return Err(AppError::Storage(
            "不能将路径复制或移动到自身或其子目录".to_string(),
        ));
    }
    if destination_exists && same_file_identity(&source, &destination)? {
        return Err(AppError::Storage(
            "不能将文件复制到指向同一文件的硬链接".to_string(),
        ));
    }
    Ok(())
}

fn same_file_identity(source: &Path, destination: &Path) -> Result<bool, AppError> {
    #[cfg(unix)]
    {
        let source_metadata = fs::metadata(source).map_err(|e| AppError::Storage(e.to_string()))?;
        let destination_metadata =
            fs::metadata(destination).map_err(|e| AppError::Storage(e.to_string()))?;
        use std::os::unix::fs::MetadataExt;
        Ok(
            source_metadata.dev() == destination_metadata.dev()
                && source_metadata.ino() == destination_metadata.ino(),
        )
    }
    #[cfg(windows)]
    {
        Ok(windows_file_identity(source)
            .map_err(|e| AppError::Storage(e.to_string()))?
            == windows_file_identity(destination)
                .map_err(|e| AppError::Storage(e.to_string()))?)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(false)
    }
}

#[cfg(windows)]
fn windows_file_identity(path: &Path) -> std::io::Result<(u32, u64)> {
    let info = windows_file_information(path)?;
    let file_index = ((info.nFileIndexHigh as u64) << 32) | u64::from(info.nFileIndexLow);
    Ok((info.dwVolumeSerialNumber, file_index))
}

#[cfg(windows)]
fn windows_file_link_count(path: &Path) -> std::io::Result<u32> {
    Ok(windows_file_information(path)?.nNumberOfLinks)
}

#[cfg(windows)]
fn windows_file_information(
    path: &Path,
) -> std::io::Result<windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    // `MetadataExt::volume_serial_number` and `file_index` are still unstable
    // on stable Rust. The Win32 handle query is the supported equivalent and
    // also works for directories when BACKUP_SEMANTICS is present.
    let handle = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    let success = unsafe { GetFileInformationByHandle(handle.as_raw_handle(), &mut info) };
    if success == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(info)
}

// Unlike an ordinary copy that may skip links, a cross-volume move must copy
// every entry before its caller is allowed to delete the original tree.
fn copy_for_move(src: &Path, dst: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(src).map_err(|e| AppError::Storage(e.to_string()))?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(src).map_err(|e| AppError::Storage(e.to_string()))?;
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target, dst);
        #[cfg(windows)]
        let result = {
            use std::os::windows::fs::FileTypeExt;
            if metadata.file_type().is_symlink_dir() {
                std::os::windows::fs::symlink_dir(target, dst)
            } else {
                std::os::windows::fs::symlink_file(target, dst)
            }
        };
        #[cfg(not(any(unix, windows)))]
        let result = Err(std::io::Error::other("symbolic links are unsupported"));
        return result.map_err(|e| AppError::Storage(e.to_string()));
    }
    if metadata.is_dir() {
        reject_unsafe_copy_destination(dst)?;
        fs::create_dir_all(dst).map_err(|e| AppError::Storage(e.to_string()))?;
        for entry in fs::read_dir(src).map_err(|e| AppError::Storage(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::Storage(e.to_string()))?;
            let destination = dst.join(entry.file_name());
            reject_unsafe_copy_destination(&destination)?;
            copy_for_move(&entry.path(), &destination)?;
        }
    } else if metadata.is_file() {
        reject_unsafe_copy_destination(dst)?;
        fs::copy(src, dst).map_err(|e| AppError::Storage(e.to_string()))?;
    } else {
        return Err(AppError::Storage(
            "跨磁盘移动不支持特殊文件，源文件已保留".to_string(),
        ));
    }
    Ok(())
}

fn reject_unsafe_copy_destination(path: &Path) -> Result<(), AppError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::Storage(
            "复制或移动的目标不能是符号链接".to_string(),
        )),
        Ok(metadata) if metadata.is_file() || metadata.is_dir() => {
            if destination_has_multiple_links(path, &metadata)? {
                return Err(AppError::Storage(
                    "复制或移动的目标不能是硬链接".to_string(),
                ));
            }
            Ok(())
        }
        Ok(_) => Err(AppError::Storage(
            "复制或移动的目标不能是特殊文件（例如管道、设备或套接字）".to_string(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn destination_has_multiple_links(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<bool, AppError> {
    if !metadata.is_file() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Ok(metadata.nlink() > 1)
    }
    #[cfg(windows)]
    {
        windows_file_link_count(path)
            .map(|count| count > 1)
            .map_err(|error| AppError::Storage(error.to_string()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(false)
    }
}

#[cfg(test)]
#[path = "tests/copy_safety_tests.rs"]
mod copy_safety_tests;
