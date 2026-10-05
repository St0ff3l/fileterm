mod root_listing_compatibility_tests {
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

    #[cfg(unix)]
    #[test]
    fn lists_hidden_files_and_symlinks_with_native_stat_and_preserves_errors() {
        use std::process::Command;
        let fixture = Fixture::new();
        std::fs::write(fixture.path().join(".hidden"), "abc").unwrap();
        std::fs::write(fixture.path().join("name ' with | pipe"), "12345").unwrap();
        let folder = fixture.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::os::unix::fs::symlink(&folder, fixture.path().join("linked-folder")).unwrap();
        std::os::unix::fs::symlink(
            fixture.path().join("absent"),
            fixture.path().join("broken-link"),
        )
        .unwrap();
        let alias = fixture.path().with_extension("alias");
        std::os::unix::fs::symlink(fixture.path(), &alias).unwrap();
        let script = root_list_shell_command(alias.to_str().unwrap());
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();
        std::fs::remove_file(&alias).unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let items = parse_root_file_list(
            &String::from_utf8_lossy(&output.stdout),
            fixture.path().to_str().unwrap(),
        );
        let hidden = items.iter().find(|item| item["name"] == ".hidden").unwrap();
        assert_eq!(hidden["size"], "3 B");
        let named = items
            .iter()
            .find(|item| item["name"] == "name ' with | pipe")
            .unwrap();
        assert_eq!(named["size"], "5 B");
        let linked = items
            .iter()
            .find(|item| item["name"] == "linked-folder")
            .unwrap();
        assert_eq!(linked["isSymlink"], true);
        assert_eq!(linked["type"], "folder");
        let broken = items
            .iter()
            .find(|item| item["name"] == "broken-link")
            .unwrap();
        assert_eq!(broken["isSymlink"], true);
        assert_eq!(broken["type"], "file");

        let script = root_list_shell_command(fixture.path().join("absent-dir").to_str().unwrap());
        let output = Command::new("sh").args(["-c", &script]).output().unwrap();
        assert!(!output.status.success());
        assert!(!output.stderr.is_empty());
    }
}
