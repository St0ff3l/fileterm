use super::*;

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("fileterm-update-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("config")).unwrap();
        fs::write(root.join("config/profiles.json"), b"user-data").unwrap();
        fs::write(root.join("portable"), b"").unwrap();
        fs::write(root.join("自定义 FileTerm.exe"), b"old").unwrap();
        Self(root)
    }
    fn target(&self) -> std::path::PathBuf {
        self.0.join("自定义 FileTerm.exe")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn replaces_renamed_executable_and_preserves_portable_data() {
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let transaction = read_transaction(&stage).unwrap();
    replace_and_launch(&stage, &transaction, |target| {
        assert_eq!(target, fixture.target());
        assert_eq!(fs::read(target).unwrap(), b"new");
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read(stage.join(BACKUP)).unwrap(), b"old");
    assert_eq!(
        fs::read(fixture.0.join("config/profiles.json")).unwrap(),
        b"user-data"
    );
    assert!(fixture.0.join("portable").is_file());
}

#[test]
fn failed_launch_restores_original_executable() {
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let transaction = read_transaction(&stage).unwrap();
    assert!(replace_and_launch(&stage, &transaction, |_| Err("launch failed".into())).is_err());
    assert_eq!(fs::read(fixture.target()).unwrap(), b"old");
    assert_eq!(fs::read(stage.join(PAYLOAD)).unwrap(), b"new");
}

#[test]
fn changed_payload_or_original_is_never_installed() {
    for change_payload in [true, false] {
        let fixture = Fixture::new();
        let stage = prepare(&fixture.target(), b"new").unwrap();
        let transaction = read_transaction(&stage).unwrap();
        let path = if change_payload {
            stage.join(PAYLOAD)
        } else {
            fixture.target()
        };
        fs::write(path, b"tampered").unwrap();
        assert!(replace_and_launch(&stage, &transaction, |_| panic!("must not launch")).is_err());
        assert!(!stage.join(BACKUP).exists());
    }
}

#[test]
fn rejects_targets_outside_original_directory() {
    for name in [
        "../FileTerm.exe",
        "..\\FileTerm.exe",
        "C:\\FileTerm.exe",
        "config",
        "a.exe:stream",
    ] {
        assert!(!valid_target(name));
    }
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let mut transaction = read_transaction(&stage).unwrap();
    transaction.target = "../FileTerm.exe".into();
    fs::write(
        stage.join("transaction.json"),
        serde_json::to_vec(&transaction).unwrap(),
    )
    .unwrap();
    assert!(read_transaction(&stage).is_err());
}

#[cfg(target_os = "windows")]
#[test]
fn occupied_executable_is_kept_without_installing_payload() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let transaction = read_transaction(&stage).unwrap();
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
        .open(fixture.target())
        .unwrap();
    assert!(replace_and_launch(&stage, &transaction, |_| panic!("must not launch")).is_err());
    assert!(fixture.target().exists());
    assert!(!stage.join(BACKUP).exists());
    assert_eq!(fs::read(stage.join(PAYLOAD)).unwrap(), b"new");
}

#[cfg(target_os = "windows")]
#[test]
fn helper_signals_readiness_and_waits_for_real_process_exit() {
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/C", "ping -n 2 127.0.0.1 > nul"])
        .spawn()
        .unwrap();
    wait_for_parent(&stage, child.id()).unwrap();
    assert!(stage.join("ready").is_file());
    assert!(child.try_wait().unwrap().is_some());
}

#[cfg(target_os = "windows")]
#[test]
fn occupied_payload_rolls_back_after_original_was_backed_up() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new();
    let stage = prepare(&fixture.target(), b"new").unwrap();
    let transaction = read_transaction(&stage).unwrap();
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
        .open(stage.join(PAYLOAD))
        .unwrap();
    assert!(replace_and_launch(&stage, &transaction, |_| panic!("must not launch")).is_err());
    assert_eq!(fs::read(fixture.target()).unwrap(), b"old");
    assert!(!stage.join(BACKUP).exists());
    assert_eq!(fs::read(stage.join(PAYLOAD)).unwrap(), b"new");
}
