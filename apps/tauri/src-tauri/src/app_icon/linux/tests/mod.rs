use super::*;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("fileterm-launcher-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn update(&self, variant: &str) -> Result<(), String> {
        let desktop_dir = self.0.join("desktop");
        update_launcher_icons(
            &self.0.join("applications"),
            &self.0.join("icons"),
            &self.0.join("theme-icons"),
            Some(&desktop_dir),
            Path::new("/usr/bin/fileterm"),
            variant,
        )
    }
    fn entry(&self, name: &str) -> String {
        fs::read_to_string(self.0.join("applications").join(name)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generated_launchers_use_a_stable_themed_icon_for_live_app_grid_refresh() {
    let fixture = Fixture::new();
    for variant in ["classic", "clean", "material", "classic"] {
        fixture.update(variant).unwrap();
        for (name, _) in DESKTOP_ENTRIES {
            assert!(fixture
                .entry(name)
                .contains("Icon=com.fileterm.desktop.runtime\n"));
        }
        assert_eq!(
            fs::read(
                fixture
                    .0
                    .join("theme-icons/com.fileterm.desktop.runtime.png")
            )
            .unwrap(),
            super::super::icon_bytes(variant)
        );
    }
}

#[test]
fn artwork_changes_notify_the_application_monitor_without_changing_icon_identity() {
    let fixture = Fixture::new();
    fixture.update("classic").unwrap();
    let before = fixture.entry("FileTerm.desktop");
    let path = fixture.0.join("applications/FileTerm.desktop");
    let old_time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    fixture.update("clean").unwrap();
    assert_eq!(fixture.entry("FileTerm.desktop"), before);
    assert!(fs::metadata(&path).unwrap().modified().unwrap() > old_time);
}

#[test]
fn preserves_custom_runtime_icon_without_partially_updating_launchers() {
    let fixture = Fixture::new();
    fixture.update("classic").unwrap();
    let before = fixture.entry("FileTerm.desktop");
    let runtime_icon = fixture
        .0
        .join("theme-icons/com.fileterm.desktop.runtime.png");
    fs::create_dir_all(runtime_icon.parent().unwrap()).unwrap();
    fs::write(&runtime_icon, b"user customized icon").unwrap();
    assert!(fixture.update("clean").is_err());
    assert_eq!(fs::read(runtime_icon).unwrap(), b"user customized icon");
    assert_eq!(fixture.entry("FileTerm.desktop"), before);
}

#[test]
fn upgrades_managed_runtime_artwork_using_its_recorded_digest() {
    let fixture = Fixture::new();
    fixture.update("classic").unwrap();
    let path = fixture
        .0
        .join("theme-icons/com.fileterm.desktop.runtime.png");
    let previous_artwork = b"FileTerm artwork from a previous release";
    fs::write(&path, previous_artwork).unwrap();
    fs::write(
        fixture.0.join("icons/themed-icon.sha256"),
        format!("{:x}", Sha256::digest(previous_artwork)),
    )
    .unwrap();
    fixture.update("clean").unwrap();
    assert_eq!(fs::read(path).unwrap(), super::super::icon_bytes("clean"));
}

#[cfg(unix)]
#[test]
fn preserves_symlinked_runtime_icons_and_their_targets() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.0.join("theme-icons")).unwrap();
    let target = fixture.0.join("custom.png");
    fs::write(&target, super::super::icon_bytes("classic")).unwrap();
    let link = fixture
        .0
        .join("theme-icons/com.fileterm.desktop.runtime.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(fixture.update("clean").is_err());
    assert_eq!(
        fs::read(target).unwrap(),
        super::super::icon_bytes("classic")
    );
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    assert!(!fixture.0.join("applications").exists());
}

#[test]
fn both_desktop_ids_follow_variants_and_restore_classic() {
    let fixture = Fixture::new();
    for variant in ["classic", "material", "clean", "classic"] {
        fixture.update(variant).unwrap();
        let entries = DESKTOP_ENTRIES.map(|(name, _)| fixture.entry(name));
        let icon_line = |entry: &str| {
            entry
                .lines()
                .find(|line| line.starts_with("Icon="))
                .unwrap()
                .to_string()
        };
        let icon = icon_line(&entries[0]);
        assert_eq!(icon, icon_line(&entries[1]));
        assert_eq!(
            fs::read(
                fixture
                    .0
                    .join("theme-icons/com.fileterm.desktop.runtime.png")
            )
            .unwrap(),
            super::super::icon_bytes(variant)
        );
        for entry in &entries {
            assert!(entry.contains("Exec=\"/usr/bin/fileterm\" %U\n"));
            assert!(entry.contains("TryExec=/usr/bin/fileterm\n"));
            assert!(entry.contains("StartupWMClass=fileterm\n"));
        }
        assert!(!entries[0].contains("NoDisplay=true"));
        assert!(entries[1].contains("NoDisplay=true"));
    }
}

#[cfg(unix)]
#[test]
fn updates_matching_desktop_shortcut_and_keeps_its_mode_and_other_fields() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let desktop_dir = fixture.0.join("desktop");
    fs::create_dir_all(&desktop_dir).unwrap();
    let shortcut = desktop_dir.join("FileTerm.desktop");
    fs::write(
        &shortcut,
        "[Desktop Entry]\nName=My FileTerm\nExec=/usr/bin/fileterm %U\nIcon=fileterm\nTerminal=false\n",
    )
    .unwrap();
    fs::set_permissions(&shortcut, fs::Permissions::from_mode(0o755)).unwrap();

    fixture.update("material").unwrap();

    let contents = fs::read_to_string(&shortcut).unwrap();
    let icon = contents
        .lines()
        .find(|line| line.starts_with("Icon="))
        .unwrap()
        .strip_prefix("Icon=")
        .unwrap();
    assert!(icon.contains("launcher-material-"));
    assert_eq!(
        fs::read(icon).unwrap(),
        super::super::icon_bytes("material")
    );
    assert!(contents.contains("Name=My FileTerm\nExec=/usr/bin/fileterm %U\n"));
    assert!(contents.contains("Terminal=false\n"));
    assert_eq!(
        fs::metadata(shortcut).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[test]
fn leaves_unrelated_or_customized_desktop_shortcuts_untouched() {
    let fixture = Fixture::new();
    let desktop_dir = fixture.0.join("desktop");
    fs::create_dir_all(&desktop_dir).unwrap();
    let unrelated = desktop_dir.join("Other.desktop");
    let customized = desktop_dir.join("FileTerm customized.desktop");
    fs::write(
        &unrelated,
        "[Desktop Entry]\nName=Other\nExec=/usr/bin/other %U\nIcon=fileterm\n",
    )
    .unwrap();
    fs::write(
        &customized,
        "[Desktop Entry]\nName=FileTerm\nExec=/usr/bin/fileterm %U\nIcon=my-custom-icon\n",
    )
    .unwrap();
    #[cfg(unix)]
    let linked_target = fixture.0.join("linked.desktop");
    #[cfg(unix)]
    let linked_shortcut = desktop_dir.join("Linked.desktop");
    #[cfg(unix)]
    {
        fs::write(
            &linked_target,
            "[Desktop Entry]\nName=FileTerm\nExec=/usr/bin/fileterm %U\nIcon=fileterm\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(&linked_target, &linked_shortcut).unwrap();
    }
    let unrelated_before = fs::read(&unrelated).unwrap();
    let customized_before = fs::read(&customized).unwrap();
    #[cfg(unix)]
    let linked_target_before = fs::read(&linked_target).unwrap();

    fixture.update("clean").unwrap();

    assert_eq!(fs::read(unrelated).unwrap(), unrelated_before);
    assert_eq!(fs::read(customized).unwrap(), customized_before);
    #[cfg(unix)]
    assert_eq!(fs::read(linked_target).unwrap(), linked_target_before);
}

#[test]
fn refreshes_path_lookup_shortcuts_for_the_installed_launcher() {
    let fixture = Fixture::new();
    let desktop_dir = fixture.0.join("desktop");
    fs::create_dir_all(&desktop_dir).unwrap();
    let shortcut = desktop_dir.join("My FileTerm.desktop");
    for command in ["fileterm", "\"fileterm\""] {
        for suffix in ["", " %U", " %u", " %F", " %f"] {
            let original = format!(
                "[Desktop Entry]\r\nName=My FileTerm\r\nExec={command}{suffix}\r\nIcon=fileterm\r\n\
                 [Desktop Action Other]\r\nExec=other\r\nIcon=custom-action\r\n"
            );
            fs::write(&shortcut, &original).unwrap();
            for variant in ["clean", "material", "classic"] {
                fixture.update(variant).unwrap();
                let updated = fs::read_to_string(&shortcut).unwrap();
                let icon = updated
                    .lines()
                    .find(|line| line.starts_with("Icon="))
                    .unwrap();
                assert_eq!(
                    fs::read(icon.strip_prefix("Icon=").unwrap()).unwrap(),
                    super::super::icon_bytes(variant)
                );
                assert_eq!(
                    updated,
                    original.replacen("Icon=fileterm", icon, 1),
                    "command {command}{suffix}, variant {variant}"
                );
            }
        }
    }
}

#[test]
fn refreshes_shortcuts_using_the_packaged_1024px_icon() {
    let fixture = Fixture::new();
    let desktop_dir = fixture.0.join("desktop");
    fs::create_dir_all(&desktop_dir).unwrap();
    let shortcut = desktop_dir.join("FileTerm.desktop");
    fs::write(
        &shortcut,
        "[Desktop Entry]\nExec=/usr/bin/fileterm %U\nIcon=/usr/share/icons/hicolor/1024x1024/apps/fileterm.png\n",
    )
    .unwrap();
    fixture.update("clean").unwrap();
    let updated = fs::read_to_string(&shortcut).unwrap();
    let icon = updated
        .lines()
        .find(|line| line.starts_with("Icon="))
        .unwrap();
    assert_eq!(
        fs::read(icon.strip_prefix("Icon=").unwrap()).unwrap(),
        super::super::icon_bytes("clean")
    );
    assert_eq!(
        fs::read_to_string(shortcut).unwrap(),
        format!("[Desktop Entry]\nExec=/usr/bin/fileterm %U\n{icon}\n")
    );
}

#[test]
fn path_lookup_matching_does_not_claim_other_installations_or_custom_commands() {
    let icon_dir = Path::new("/home/test/icons");
    for (executable, command, icon) in [
        (
            "\"/usr/bin/fileterm\"",
            "/opt/other/fileterm %U",
            "fileterm",
        ),
        ("\"/usr/bin/fileterm\"", "fileterm-helper %U", "fileterm"),
        ("\"/usr/bin/fileterm\"", "fileterm --custom %U", "fileterm"),
        ("\"/usr/bin/fileterm\"", "env fileterm %U", "fileterm"),
        (
            "\"/usr/bin/fileterm\"",
            "fileterm %U",
            "/home/test/custom.png",
        ),
        (
            "\"/home/test/FileTerm.AppImage\"",
            "fileterm %U",
            "fileterm",
        ),
    ] {
        let contents = format!("[Desktop Entry]\nExec={command}\nIcon={icon}\n");
        assert_eq!(
            update_desktop_shortcut(&contents, executable, icon_dir, "/new.png"),
            None,
            "must preserve {command} with {icon} for {executable}"
        );
    }
}

#[test]
fn preserves_user_managed_companion_without_partially_updating_visible_entry() {
    let fixture = Fixture::new();
    fixture.update("classic").unwrap();
    let before = fixture.entry("FileTerm.desktop");
    let companion = fixture.0.join("applications/com.fileterm.desktop.desktop");
    fs::write(&companion, "[Desktop Entry]\nName=Custom\n").unwrap();
    assert!(fixture.update("material").is_err());
    assert_eq!(fixture.entry("FileTerm.desktop"), before);
    assert_eq!(
        fs::read_to_string(companion).unwrap(),
        "[Desktop Entry]\nName=Custom\n"
    );
}

#[test]
fn migrates_legacy_generated_entry_and_does_not_rewrite_unchanged_files() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("applications")).unwrap();
    fs::write(
        fixture.0.join("applications/FileTerm.desktop"),
        format!("{MARKER}\n[Desktop Entry]\nIcon=/old/launcher.png\n"),
    )
    .unwrap();
    fixture.update("classic").unwrap();
    let path = fixture.0.join("applications/FileTerm.desktop");
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    fixture.update("classic").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert!(fixture
        .entry("FileTerm.desktop")
        .contains("Icon=com.fileterm.desktop.runtime\n"));
}

#[test]
fn package_entry_points_preserve_debian_and_arch_wrappers() {
    for exe in [
        "/usr/lib/fileterm/fileterm",
        "/usr/lib/fileterm/runtime/fileterm",
        "/usr/bin/fileterm",
    ] {
        assert_eq!(
            packaged_launcher(Path::new(exe)),
            Path::new("/usr/bin/fileterm")
        );
    }
    assert_eq!(
        packaged_launcher(Path::new("/opt/FileTerm/fileterm")),
        Path::new("/opt/FileTerm/fileterm")
    );
}

#[test]
fn desktop_exec_uses_both_escape_layers() {
    assert_eq!(
        quote_desktop_exec_arg("/home/a b/File\"Term`$%\\"),
        "\"/home/a b/File\\\\\"Term\\\\`\\\\$%%\\\\\\\\\""
    );
    assert_eq!(
        escape_desktop_value("/home/a b/File\\Term.png"),
        "/home/a b/File\\\\Term.png"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn generated_entries_round_trip_through_native_gio_with_special_path_characters() {
    use gtk::gio::prelude::{AppInfoExt, IconExt};
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let executable = fixture.0.join("File Term-中文-\"`$%\\");
    fs::write(&executable, "#!/bin/sh\nprintf launched > \"$0.ran\"\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let marker = std::path::PathBuf::from(format!("{}.ran", executable.display()));
    update_launcher_icons(
        &fixture.0.join("applications"),
        &fixture.0.join("icons"),
        &fixture.0.join("theme-icons"),
        None,
        &executable,
        "material",
    )
    .unwrap();
    for (name, _) in DESKTOP_ENTRIES {
        let entry =
            gtk::gio::DesktopAppInfo::from_filename(fixture.0.join("applications").join(name))
                .expect("GIO must accept the generated desktop entry including TryExec");
        assert_eq!(entry.executable(), Path::new("/usr/bin/env"));
        entry
            .launch(&[], None::<&gtk::gio::AppLaunchContext>)
            .unwrap();
        for _ in 0..500 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(fs::read(&marker).unwrap(), b"launched");
        fs::remove_file(&marker).unwrap();
        assert!(IconExt::to_string(&entry.icon().unwrap())
            .unwrap()
            .contains("com.fileterm.desktop.runtime"));
    }
}
