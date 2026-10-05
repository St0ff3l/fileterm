mod access_directory_tests {
    use super::*;

    #[cfg(unix)]
    struct Fixture(std::path::PathBuf);
    #[cfg(unix)]
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("fileterm-root-compat-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    #[cfg(unix)]
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn maps_nas_aliases_from_observed_volume_without_fixed_home() {
        let candidates =
            root_access_directory_candidates("/homes/alice/project", Some("/volume12/homes/alice"));
        assert_eq!(candidates[0], "/homes/alice/project");
        assert_eq!(candidates[1], "/volume12/homes/alice/project");
        assert_eq!(candidates.last().unwrap(), "/");
        assert!(
            root_access_directory_candidates("/photo", Some("/volume3/homes/alice"))
                .contains(&"/volume3/photo".to_string())
        );
        assert!(root_access_directory_candidates(
            "/homes/alice",
            Some("/var/services/homes/alice")
        )
        .contains(&"/var/services/homes/alice".to_string()));
    }

    #[test]
    fn ordinary_linux_directory_is_kept_and_missing_home_uses_system_root() {
        assert_eq!(
            root_access_directory_candidates("/srv/app", Some("/home/alice"))[0],
            "/srv/app"
        );
        let candidates = root_access_directory_candidates("/nonexistent", None);
        assert_eq!(candidates, ["/nonexistent", "/"]);
        assert!(!root_access_directory_command("/nonexistent", None).contains("/root"));
    }

    #[cfg(unix)]
    #[test]
    fn runs_directory_resolution_with_missing_home_symlinks_and_quoted_names() {
        use std::process::Command;
        let fixture = Fixture::new();
        let directory = fixture.path().join("dir ' with $(literal)");
        std::fs::create_dir(&directory).unwrap();
        let link = fixture.path().join("alias");
        std::os::unix::fs::symlink(&directory, &link).unwrap();
        let script = root_access_directory_command(link.to_str().unwrap(), None);
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            parse_root_access_directory(&String::from_utf8_lossy(&output.stdout)).unwrap(),
            directory.canonicalize().unwrap().to_str().unwrap(),
        );
        let absent = fixture.path().join("absent-home");
        let script = root_access_directory_command(absent.to_str().unwrap(), None);
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();
        assert!(output.status.success());
        assert_eq!(
            parse_root_access_directory(&String::from_utf8_lossy(&output.stdout)).unwrap(),
            "/"
        );

        let file = fixture.path().join("not-a-directory");
        std::fs::write(&file, "file").unwrap();
        let script = root_access_directory_command(file.to_str().unwrap(), None);
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("不是目录"));
    }

    #[test]
    fn ignores_profile_noise_but_rejects_invalid_directory_frames() {
        assert_eq!(
            parse_root_access_directory("warning\n__FILETERM_ACCESS_DIRECTORY__/srv/app\r\n")
                .unwrap(),
            "/srv/app"
        );
        assert!(parse_root_access_directory("/srv/app").is_err());
        assert!(parse_root_access_directory("__FILETERM_ACCESS_DIRECTORY__relative").is_err());
        assert!(parse_root_access_directory("__FILETERM_ACCESS_DIRECTORY__/srv\nnoise").is_err());
    }

    #[test]
    fn privileged_file_commands_use_non_login_shells_and_stdin_credentials() {
        for method in [RootFileAccessMethod::Sudo, RootFileAccessMethod::Su] {
            let (command, password) =
                root_file_command(method, &None, &Some("secret".to_string()), "true");
            assert!(!command.contains("secret"));
            assert!(!command.contains("sh -lc"));
            assert_eq!(password.as_deref(), Some("secret"));
        }
    }
}
