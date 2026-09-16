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
mod copy_safety_tests {
    use super::*;

    #[test]
    fn creating_a_file_never_truncates_an_existing_file() {
        let root =
            std::env::temp_dir().join(format!("fileterm-create-safety-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let directory = root.to_string_lossy().into_owned();
        app_create_local_file(directory.clone(), "file".into()).unwrap();
        fs::write(root.join("file"), b"keep existing contents").unwrap();
        assert!(app_create_local_file(directory, "file".into()).is_err());
        assert_eq!(
            fs::read(root.join("file")).unwrap(),
            b"keep existing contents"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_name_operations_cannot_escape_the_current_directory() {
        for name in ["", ".", "..", "../outside", "nested/file", "bad\0name"] {
            assert!(validate_local_name(name).is_err(), "{name:?}");
        }
        assert!(validate_local_name("safe name.txt").is_ok());
        #[cfg(unix)]
        assert!(validate_local_name(r"literal\name").is_ok());
    }

    #[test]
    fn copy_rejects_self_alias_and_nested_destination() {
        let root =
            std::env::temp_dir().join(format!("fileterm-copy-safety-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("source")).unwrap();
        fs::write(root.join("source/file"), b"keep").unwrap();
        assert!(app_copy_local_path(
            root.join("source").to_string_lossy().into_owned(),
            root.join("source/nested").to_string_lossy().into_owned()
        )
        .is_err());
        assert!(app_copy_local_path(
            root.join("source/file").to_string_lossy().into_owned(),
            root.join("source/./file").to_string_lossy().into_owned()
        )
        .is_err());
        assert_eq!(fs::read(root.join("source/file")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_rejects_a_hard_link_to_the_source_file() {
        let root =
            std::env::temp_dir().join(format!("fileterm-copy-hard-link-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let alias = root.join("alias");
        fs::write(&source, b"keep").unwrap();
        fs::hard_link(&source, &alias).unwrap();
        assert!(app_copy_local_path(
            source.to_string_lossy().into_owned(),
            alias.to_string_lossy().into_owned()
        )
        .is_err());
        assert_eq!(fs::read(&source).unwrap(), b"keep");
        assert_eq!(fs::read(&alias).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_rejects_a_nested_hard_link_destination_without_mutating_it() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-copy-nested-hard-link-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        let outside = root.join("outside");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&destination).unwrap();
        fs::write(source.join("config"), b"new data").unwrap();
        fs::write(&outside, b"keep existing contents").unwrap();
        fs::hard_link(&outside, destination.join("config")).unwrap();

        assert!(app_copy_local_path(
            source.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned()
        )
        .is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"keep existing contents");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_rejects_a_symlink_destination_instead_of_following_it() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "fileterm-copy-symlink-destination-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("source");
        let outside = root.join("outside");
        let alias = root.join("alias");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"safe source").unwrap();
        fs::write(&outside, b"must stay unchanged").unwrap();
        symlink(&outside, &alias).unwrap();
        assert!(app_copy_local_path(
            source.to_string_lossy().into_owned(),
            alias.to_string_lossy().into_owned()
        )
        .is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"must stay unchanged");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_rejects_special_files_instead_of_opening_them() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let root = std::env::temp_dir().join(format!(
            "fileterm-copy-special-file-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&source).unwrap();
        let fifo = source.join("pipe");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        assert!(app_copy_local_path(
            source.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned()
        )
        .is_err());
        assert!(!destination.join("pipe").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_rejects_special_destinations_instead_of_opening_them() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::FileTypeExt;
        let root = std::env::temp_dir().join(format!(
            "fileterm-copy-special-destination-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"safe source").unwrap();
        let destination_c = CString::new(destination.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(destination_c.as_ptr(), 0o600) }, 0);
        assert!(app_copy_local_path(
            source.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned()
        )
        .is_err());
        assert!(fs::symlink_metadata(&destination)
            .unwrap()
            .file_type()
            .is_fifo());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cross_volume_copy_preserves_dangling_and_cyclic_links() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("fileterm-move-links-{}", uuid::Uuid::new_v4()));
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("file"), b"keep").unwrap();
        symlink("missing", src.join("dangling")).unwrap();
        symlink(".", src.join("loop")).unwrap();
        copy_for_move(&src, &dst).unwrap();
        assert_eq!(
            fs::read_link(dst.join("dangling")).unwrap(),
            Path::new("missing")
        );
        assert_eq!(fs::read_link(dst.join("loop")).unwrap(), Path::new("."));
        assert_eq!(fs::read(dst.join("file")).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mount_cleanup_never_removes_contents_of_a_remaining_share() {
        let root =
            std::env::temp_dir().join(format!("fileterm-mount-safety-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("keep"), b"remote data").unwrap();
        remove_empty_mount_directory(&root);
        assert_eq!(fs::read(root.join("keep")).unwrap(), b"remote data");
        fs::remove_file(root.join("keep")).unwrap();
        remove_empty_mount_directory(&root);
        assert!(!root.exists());
    }
}
