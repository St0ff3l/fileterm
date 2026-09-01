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

#[cfg(test)]
mod copy_tests {
    use super::copy_dir_recursive;
    use std::fs;

    fn scratch_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "fileterm-copy-test-{}-{}-{}",
            std::process::id(),
            label,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).expect("create scratch dir");
        path
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_recursive_skips_symlink_loops() {
        // Regression for M3: a symlinked directory loop must not send the
        // copy into infinite recursion. The symlink is skipped and the real
        // file is copied once.
        use std::os::unix::fs::symlink;

        let src = scratch_dir("src");
        fs::write(src.join("real.txt"), b"hello").unwrap();
        // `self` → src, creating a cycle through a symlinked directory.
        symlink(&src, src.join("self")).unwrap();

        let dst = scratch_dir("dst");
        copy_dir_recursive(&src, &dst).expect("copy must complete");

        assert_eq!(fs::read(dst.join("real.txt")).unwrap(), b"hello");
        assert!(
            !dst.join("self").exists(),
            "symlinked directory must not be followed"
        );
    }

    #[test]
    fn copy_dir_recursive_copies_nested_real_files() {
        let src = scratch_dir("nested-src");
        let nested = src.join("sub");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("a.txt"), b"a").unwrap();
        fs::write(nested.join("b.txt"), b"b").unwrap();

        let dst = scratch_dir("nested-dst");
        copy_dir_recursive(&src, &dst).expect("copy must complete");

        assert_eq!(fs::read(dst.join("sub").join("a.txt")).unwrap(), b"a");
        assert_eq!(fs::read(dst.join("sub").join("b.txt")).unwrap(), b"b");
    }

    #[test]
    fn copy_dir_recursive_handles_empty_directory() {
        let src = scratch_dir("empty-src");
        let dst = scratch_dir("empty-dst");
        copy_dir_recursive(&src, &dst).expect("copy must complete");
        assert!(dst.is_dir());
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

#[cfg(test)]
mod format_tests {
    use super::{format_modified, format_size};

    #[test]
    fn format_modified_handles_epoch_zero() {
        assert_eq!(format_modified(0), "1970/01/01 00:00");
    }

    #[test]
    fn format_modified_renders_minute_resolution() {
        // 1970-01-01 00:01:00 UTC
        assert_eq!(format_modified(60), "1970/01/01 00:01");
        // 1970-01-01 01:00:00 UTC
        assert_eq!(format_modified(3600), "1970/01/01 01:00");
        // 1970-01-02 00:00:00 UTC (跨日)
        assert_eq!(format_modified(86400), "1970/01/02 00:00");
    }

    #[test]
    fn format_modified_truncates_seconds() {
        // 1970-01-01 00:00:59 UTC → 截断到分钟，仍为 00:00
        assert_eq!(format_modified(59), "1970/01/01 00:00");
    }

    #[test]
    fn format_modified_crosses_year_boundary() {
        // 2025-01-01 00:00:00 UTC
        assert_eq!(format_modified(1_735_689_600), "2025/01/01 00:00");
        // 2024-12-31 23:59:00 UTC（年末，验证 12 月 31 日 23:59）
        assert_eq!(format_modified(1_735_689_540), "2024/12/31 23:59");
    }

    #[test]
    fn format_modified_handles_leap_day() {
        // 2024-02-29 00:00:00 UTC（闰日，验证闰年 2 月 29 日存在）
        assert_eq!(format_modified(1_709_164_800), "2024/02/29 00:00");
        // 2024-03-01 00:00:00 UTC（闰日次日）
        assert_eq!(format_modified(1_709_251_200), "2024/03/01 00:00");
        // 2023-03-01 00:00:00 UTC（平年 2 月只有 28 天，验证不误判闰年）
        assert_eq!(format_modified(1_677_628_800), "2023/03/01 00:00");
    }

    #[test]
    fn format_size_uses_si_units_consistently() {
        // 阈值与除法统一为 1000 进制，与 ssh.rs / ftp.rs 对齐。
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1000), "1.0 KB");
        // 1024 字节：1000 进制下仍是 1.0 KB（旧实现错误输出 "1.0 MB"）
        assert_eq!(format_size(1024), "1.0 KB");
        // 10_000_000 字节 = 10 MB（value=10.0，decimals=0）
        assert_eq!(format_size(10_000_000), "10 MB");
        // 1_500_000 字节 = 1.5 MB
        assert_eq!(format_size(1_500_000), "1.5 MB");
        // 1_073_741_824 字节 = 1.07 GB（旧 binary 实现下是 "1.0 GB"）
        assert_eq!(format_size(1_073_741_824), "1.1 GB");
    }
}
