use super::*;

#[test]
fn rejects_disk_images_translocation_and_development_binaries() {
    for path in [
        "/Volumes/FileTerm/FileTerm.app",
        "/private/var/folders/a/AppTranslocation/id/d/FileTerm.app",
        "/tmp/target/debug/fileterm",
    ] {
        assert!(validate_app_path(Path::new(path)).is_err());
    }
    assert!(validate_app_path(Path::new("/Applications/FileTerm.app")).is_ok());
}

#[test]
fn failed_install_restores_the_original_app() {
    for failure in ["missing", "partial", "panic"] {
        let root = std::env::temp_dir().join(format!(
            "fileterm-macos-update-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&root).unwrap();
        let app = root.join("FileTerm.app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(app.join("version"), "old").unwrap();
        let result = install_with_backup(&app, || {
            std::fs::remove_dir_all(&app).unwrap();
            if failure == "panic" {
                panic!("installer panic");
            }
            if failure == "partial" {
                std::fs::create_dir(&app).unwrap();
                std::fs::write(app.join("version"), "partial").unwrap();
            }
            Err("installation failed".to_string())
        });
        assert!(result.unwrap_err().contains("已恢复旧版"));
        assert_eq!(std::fs::read_to_string(app.join("version")).unwrap(), "old");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn successful_install_removes_the_backup() {
    let root = std::env::temp_dir().join(format!(
        "fileterm-macos-update-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&root).unwrap();
    let app = root.join("FileTerm.app");
    std::fs::create_dir(&app).unwrap();
    std::fs::write(app.join("version"), "old").unwrap();
    install_with_backup(&app, || {
        std::fs::write(app.join("version"), "new").map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(std::fs::read_to_string(app.join("version")).unwrap(), "new");
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
