use super::*;

#[test]
fn routes_only_supported_installed_formats_to_exact_targets() {
    for (architecture, target_arch) in [("x86_64", "x86_64"), ("aarch64", "aarch64")] {
        for (bundle, suffix) in [
            (BundleType::AppImage, "appimage"),
            (BundleType::Deb, "deb"),
            (BundleType::Rpm, "rpm"),
        ] {
            assert_eq!(
                target_for_installation(Some(bundle.clone()), architecture, true, true),
                Some(format!("linux-{target_arch}-{suffix}").as_str())
            );
            assert_eq!(
                target_for_installation(Some(bundle), "arm", true, true),
                None
            );
        }
    }
    assert_eq!(target_for_installation(None, "x86_64", true, true), None);
    assert_eq!(
        target_for_installation(Some(BundleType::AppImage), "x86_64", false, true),
        None
    );
    for bundle in [BundleType::Deb, BundleType::Rpm] {
        assert_eq!(
            target_for_installation(Some(bundle), "x86_64", false, false),
            None,
            "Arch/repackaged binaries must not invoke the DEB/RPM installer"
        );
    }
}

#[test]
fn package_owner_must_match_the_installed_executable() {
    let executable = Path::new("/usr/lib/fileterm/fileterm");
    for owner in ["file-term", "file-term:amd64"] {
        assert!(owner_matches(
            &BundleType::Deb,
            &format!("{owner}: {}\n", executable.display()),
            executable
        ));
    }
    assert!(!owner_matches(
        &BundleType::Deb,
        "file-term: /usr/bin/other",
        executable
    ));
    assert!(!owner_matches(
        &BundleType::Deb,
        "another-package: /usr/lib/fileterm/fileterm",
        executable
    ));
    assert!(!owner_matches(&BundleType::Deb, "", executable));
    assert!(owner_matches(&BundleType::Rpm, "file-term", executable));
    assert!(!owner_matches(
        &BundleType::Rpm,
        "file-term-devel",
        executable
    ));
}

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "fileterm-linux-update-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("config.json"), "user-data").unwrap();
        let fixture = Self(root);
        std::fs::write(fixture.image(), "old").unwrap();
        std::fs::set_permissions(fixture.image(), std::fs::Permissions::from_mode(0o751)).unwrap();
        fixture
    }

    fn image(&self) -> std::path::PathBuf {
        self.0.join("自定义 FileTerm.AppImage")
    }

    fn backups(&self) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(&self.0)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.join("original.AppImage").is_file())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn appimage(architecture: &str) -> Vec<u8> {
    let mut bytes = vec![0_u8; 80];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    bytes[8..11].copy_from_slice(b"AI\x02");
    let machine = if architecture == "x86_64" {
        62_u16
    } else {
        183_u16
    };
    bytes[18..20].copy_from_slice(&machine.to_le_bytes());
    bytes
}

#[test]
fn rejects_wrong_format_architecture_and_truncated_appimage_headers() {
    for architecture in ["x86_64", "aarch64"] {
        let valid = appimage(architecture);
        assert!(validate_appimage(&valid, architecture).is_ok());
        for length in 0..64 {
            assert!(validate_appimage(&valid[..length], architecture).is_err());
        }
        assert!(validate_appimage(
            &valid,
            if architecture == "x86_64" {
                "aarch64"
            } else {
                "x86_64"
            }
        )
        .is_err());
        for offset in [0, 4, 5, 8, 10, 18] {
            let mut invalid = valid.clone();
            invalid[offset] = 0xff;
            assert!(validate_appimage(&invalid, architecture).is_err());
        }
    }
    assert!(validate_appimage(&appimage("aarch64"), "riscv64").is_err());
    let fixture = Fixture::new();
    assert!(
        install_appimage_with_backup(&fixture.image(), b"!<arch>\n", "x86_64", |_| panic!(
            "must not prepare"
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(fixture.image()).unwrap(), b"old");
    assert!(fixture.backups().is_empty());
}

#[test]
fn atomically_replaces_renamed_appimage_and_preserves_permissions_and_data() {
    let fixture = Fixture::new();
    let image = fixture.image();
    let mut running_image = std::fs::File::open(&image).unwrap();
    let bytes = appimage("x86_64");
    install_appimage_with_backup(&image, &bytes, "x86_64", |staged| {
        assert_eq!(std::fs::read(&image).unwrap(), b"old");
        std::fs::write(staged, &bytes).map_err(|error| error.to_string())
    })
    .unwrap();
    let mut old_bytes = Vec::new();
    running_image.read_to_end(&mut old_bytes).unwrap();
    assert_eq!(
        old_bytes, b"old",
        "the old inode must never be truncated or overwritten"
    );
    assert_eq!(std::fs::read(&image).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&image).unwrap().permissions().mode() & 0o777,
        0o751
    );
    assert_eq!(
        std::fs::read(fixture.0.join("config.json")).unwrap(),
        b"user-data"
    );
    assert!(fixture.backups().is_empty());
}

#[test]
fn incomplete_corrupt_or_panicking_staging_never_changes_the_current_image() {
    for failure in ["disk-full", "corrupt", "panic", "missing"] {
        let fixture = Fixture::new();
        let bytes = appimage("x86_64");
        let result = install_appimage_with_backup(&fixture.image(), &bytes, "x86_64", |staged| {
            if failure != "missing" {
                std::fs::write(staged, b"partial").unwrap();
            }
            match failure {
                "disk-full" => Err("No space left on device".to_string()),
                "panic" => panic!("staging panic"),
                _ => Ok(()),
            }
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read(fixture.image()).unwrap(), b"old");
        assert_eq!(
            std::fs::metadata(fixture.image())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
        assert!(fixture.backups().is_empty());
    }
}

#[test]
fn refuses_to_overwrite_a_concurrently_changed_image_and_keeps_the_backup() {
    for change in ["changed", "missing", "directory"] {
        let fixture = Fixture::new();
        let image = fixture.image();
        let bytes = appimage("x86_64");
        let result = install_appimage_with_backup(&image, &bytes, "x86_64", |staged| {
            std::fs::write(staged, &bytes).unwrap();
            std::fs::remove_file(&image).unwrap();
            match change {
                "changed" => std::fs::write(&image, "other-update").unwrap(),
                "directory" => std::fs::create_dir(&image).unwrap(),
                _ => {}
            }
            Ok(())
        });
        let error = result.unwrap_err();
        let backups = fixture.backups();
        assert_eq!(backups.len(), 1);
        assert!(error.contains(backups[0].join("original.AppImage").to_str().unwrap()));
        assert_eq!(
            std::fs::read(backups[0].join("original.AppImage")).unwrap(),
            b"old"
        );
        if change == "changed" {
            assert_eq!(std::fs::read(&image).unwrap(), b"other-update");
        }
        if change == "missing" {
            assert!(!image.exists());
        }
        if change == "directory" {
            assert!(image.is_dir());
        }
    }
}

#[test]
fn symlinks_cannot_be_replaced_by_the_appimage_installer() {
    let fixture = Fixture::new();
    let link = fixture.0.join("link.AppImage");
    std::os::unix::fs::symlink(fixture.image(), &link).unwrap();
    assert!(
        install_appimage_with_backup(&link, &appimage("x86_64"), "x86_64", |_| panic!(
            "must not prepare"
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(fixture.image()).unwrap(), b"old");
}

#[test]
fn refuses_to_restart_missing_nonexecutable_or_directory_targets() {
    let fixture = Fixture::new();
    assert!(validate_restart_path(&fixture.image()).is_ok());
    std::fs::set_permissions(fixture.image(), std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(validate_restart_path(&fixture.image()).is_err());
    assert!(validate_restart_path(&fixture.0).is_err());
    assert!(validate_restart_path(&fixture.0.join("missing")).is_err());
}

#[test]
fn rollback_restores_permissions_and_never_discards_a_blocked_backup() {
    for blocked in [false, true] {
        let fixture = Fixture::new();
        let image = fixture.image();
        let original = fixture.0.join("original.AppImage");
        std::fs::copy(&image, &original).unwrap();
        if blocked {
            std::fs::remove_file(&image).unwrap();
            std::fs::create_dir(&image).unwrap();
            std::fs::write(image.join("user-file"), "keep").unwrap();
        } else {
            std::fs::write(&image, "failed-new-version").unwrap();
            std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let result = restore_appimage(&original, &image);
        if blocked {
            assert!(result.unwrap_err().contains(original.to_str().unwrap()));
            assert_eq!(std::fs::read(&original).unwrap(), b"old");
            assert_eq!(std::fs::read(image.join("user-file")).unwrap(), b"keep");
        } else {
            result.unwrap();
            assert_eq!(std::fs::read(&image).unwrap(), b"old");
            assert_eq!(
                std::fs::metadata(&image).unwrap().permissions().mode() & 0o777,
                0o751
            );
        }
    }
}

#[test]
fn package_owner_queries_finish_or_are_killed_at_the_deadline() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let output = command_output(
        Command::new("sh").args(["-c", "printf file-term"]),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"file-term");
    let output = command_output(
        Command::new("sh").args(["-c", "exit 7"]),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(7));
    let start = Instant::now();
    let error =
        command_output(Command::new("sleep").arg("3"), Duration::from_millis(25)).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn interrupted_staging_never_truncates_the_current_image() {
    const CHILD_ENV: &str = "FILETERM_APPIMAGE_STAGING_TEST";
    if let Some(root) = std::env::var_os(CHILD_ENV) {
        let fixture = Fixture(root.into());
        let bytes = appimage("x86_64");
        let _ = install_appimage_with_backup(&fixture.image(), &bytes, "x86_64", |staged| {
            std::fs::write(staged, b"partial").unwrap();
            std::fs::write(fixture.0.join("ready"), b"ready").unwrap();
            loop {
                std::thread::park();
            }
        });
        panic!("child must wait during staging");
    }
    let fixture = Fixture::new();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "services::updates::linux::tests::interrupted_staging_never_truncates_the_current_image", "--test-threads=1"])
        .env(CHILD_ENV, &fixture.0).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !fixture.0.join("ready").is_file() && std::time::Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let ready = fixture.0.join("ready").is_file();
    let _ = child.kill();
    let _ = child.wait();
    assert!(ready, "child did not reach staging");
    assert_eq!(std::fs::read(fixture.image()).unwrap(), b"old");
    let backups = fixture.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read(backups[0].join("original.AppImage")).unwrap(),
        b"old"
    );
}
