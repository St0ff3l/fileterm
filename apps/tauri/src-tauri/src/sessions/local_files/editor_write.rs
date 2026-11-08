fn write_local_file_safely(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    // Resolve links so saving edits updates their target instead of replacing the link.
    let target = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            fs::canonicalize(path).map_err(|error| AppError::Storage(error.to_string()))?
        }
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    let original = match fs::metadata(&target) {
        Ok(metadata) if metadata.is_file() => {
            fs::OpenOptions::new()
                .write(true)
                .open(&target)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            Some(metadata)
        }
        Ok(_) => return Err(AppError::Storage("编辑保存的目标必须是普通文件".into())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(AppError::Storage(error.to_string())),
    };
    let staged = target.with_file_name(format!(".fileterm-edit-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        write_editor_staging(&staged, &target, original.as_ref(), bytes)?;
        commit_editor_staging(&staged, &target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

#[cfg(test)]
mod editor_write_tests {
    use super::*;

    #[test]
    fn saving_replaces_complete_content_and_rejects_non_files() {
        let root = std::env::temp_dir().join(format!("fileterm-editor-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let file = root.join("file");
        fs::write(&file, "original").unwrap();
        write_local_file_safely(&file, b"new").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new");
        assert!(write_local_file_safely(&root, b"bad").is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_preserves_symlink_and_restrictive_permissions() {
        use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
        let root = std::env::temp_dir().join(format!("fileterm-editor-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let target = root.join("secret");
        let link = root.join("link");
        fs::write(&target, "old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        let original = fs::metadata(&target).unwrap();
        symlink(&target, &link).unwrap();
        write_local_file_safely(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let saved = fs::metadata(&target).unwrap();
        assert_eq!((saved.uid(), saved.gid()), (original.uid(), original.gid()));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn saving_preserves_extended_file_attributes() {
        use std::os::fd::AsRawFd;
        let root =
            std::env::temp_dir().join(format!("fileterm-editor-attrs-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        fs::write(&path, b"old").unwrap();
        let file = fs::File::open(&path).unwrap();
        let name = c"com.fileterm.editor-test";
        assert_eq!(
            unsafe {
                libc::fsetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    b"metadata".as_ptr().cast(),
                    8,
                    0,
                    0,
                )
            },
            0
        );
        drop(file);
        write_local_file_safely(&path, b"new").unwrap();
        let file = fs::File::open(&path).unwrap();
        let mut value = [0_u8; 8];
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    file.as_raw_fd(),
                    name.as_ptr(),
                    value.as_mut_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            },
            8
        );
        assert_eq!(&value, b"metadata");
        assert_eq!(fs::read(&path).unwrap(), b"new");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn saving_preserves_windows_named_streams() {
        let root =
            std::env::temp_dir().join(format!("fileterm-editor-streams-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        let stream = root.join("file:metadata");
        fs::write(&path, b"old").unwrap();
        fs::write(&stream, b"metadata").unwrap();
        write_local_file_safely(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read(&stream).unwrap(), b"metadata");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "linux")]
    fn linux_test_acl(owner_permissions: u16) -> Vec<u8> {
        let mut acl = 2_u32.to_le_bytes().to_vec();
        for (tag, permission, id) in [
            (1_u16, owner_permissions, u32::MAX),
            (2, 4, unsafe { libc::geteuid() }.saturating_add(1)),
            (4, 0, u32::MAX),
            (16, 4, u32::MAX),
            (32, 0, u32::MAX),
        ] {
            acl.extend_from_slice(&tag.to_le_bytes());
            acl.extend_from_slice(&permission.to_le_bytes());
            acl.extend_from_slice(&id.to_le_bytes());
        }
        acl
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn saving_does_not_keep_an_inherited_acl_when_original_has_none() {
        use std::os::{fd::AsRawFd, unix::fs::PermissionsExt};
        let root = std::env::temp_dir().join(format!(
            "fileterm-editor-inherited-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let parent = fs::File::open(&root).unwrap();
        let acl = linux_test_acl(7);
        let result = unsafe {
            libc::fsetxattr(
                parent.as_raw_fd(),
                c"system.posix_acl_default".as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EOPNOTSUPP) {
            eprintln!("filesystem does not support default ACLs; ACL fixture skipped");
            fs::remove_dir_all(root).unwrap();
            return;
        }
        assert_eq!(result, 0);
        write_local_file_safely(&path, b"new").unwrap();
        let saved = fs::File::open(&path).unwrap();
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    saved.as_raw_fd(),
                    c"system.posix_acl_access".as_ptr(),
                    std::ptr::null_mut(),
                    0,
                )
            },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ENODATA)
        );
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn saving_does_not_keep_directory_inherited_access() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "fileterm-editor-inherited-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&root).unwrap();
        let file = root.join("file");
        fs::write(&file, b"old").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
        let status = std::process::Command::new("/bin/chmod")
            .args([
                "+a",
                "user:daemon allow read,file_inherit,directory_inherit",
            ])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
        write_local_file_safely(&file, b"new").unwrap();
        let after = std::process::Command::new("/bin/ls")
            .arg("-le")
            .arg(&file)
            .output()
            .unwrap();
        assert!(after.status.success());
        let access = String::from_utf8_lossy(&after.stdout);
        let safe = !access.contains("daemon allow");
        fs::remove_dir_all(root).unwrap();
        assert!(safe, "saving added inherited access: {access}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn saving_preserves_extended_posix_access_control() {
        use std::os::fd::AsRawFd;
        let root =
            std::env::temp_dir().join(format!("fileterm-editor-acl-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        fs::write(&path, b"old").unwrap();
        let source = fs::File::open(&path).unwrap();
        let acl = linux_test_acl(6);
        let name = c"system.posix_acl_access";
        let result = unsafe {
            libc::fsetxattr(
                source.as_raw_fd(),
                name.as_ptr(),
                acl.as_ptr().cast(),
                acl.len(),
                0,
            )
        };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EOPNOTSUPP) {
            eprintln!("filesystem does not support POSIX ACLs; ACL fixture skipped");
            fs::remove_dir_all(root).unwrap();
            return;
        }
        assert_eq!(result, 0);
        drop(source);
        write_local_file_safely(&path, b"new").unwrap();
        let saved = fs::File::open(&path).unwrap();
        let mut actual = vec![0_u8; acl.len()];
        assert_eq!(
            unsafe {
                libc::fgetxattr(
                    saved.as_raw_fd(),
                    name.as_ptr(),
                    actual.as_mut_ptr().cast(),
                    actual.len(),
                )
            },
            acl.len() as isize
        );
        assert_eq!(actual, acl);
        assert_eq!(fs::read(&path).unwrap(), b"new");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_write_failure_leaves_original_bytes_intact() {
        const CHILD: &str = "FILETERM_EDITOR_WRITE_LIMIT_TEST";
        if std::env::var_os(CHILD).is_none() {
            let name = format!(
                "{}::saving_write_failure_leaves_original_bytes_intact",
                module_path!().split_once("::").unwrap().1
            );
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let root =
            std::env::temp_dir().join(format!("fileterm-editor-failure-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("file");
        fs::write(&path, b"original").unwrap();
        // Isolate the OS fault in a child so other tests retain their normal file limits.
        unsafe {
            libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
            let mut limit: libc::rlimit = std::mem::zeroed();
            assert_eq!(libc::getrlimit(libc::RLIMIT_FSIZE, &mut limit), 0);
            limit.rlim_cur = 32;
            assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
        }
        assert!(write_local_file_safely(&path, &[b'x'; 64]).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
