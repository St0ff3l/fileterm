use super::{
    build_compatible_profile_payload, build_export_payload, collect_import_files, export_filename,
    fingerprint, normalize_external_profile, parse_ssh_config, IMPORT_FILE_LIMIT,
};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn parses_ssh_config_and_skips_wildcards() {
    let entries = parse_ssh_config(
        "Host dev\n HostName dev.example\n User ops\n\nHost *\n User default",
        "config",
        &[],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].preview["name"], "dev");
}

#[test]
fn normalizes_compatible_json() {
    let profile = normalize_external_profile(&json!({"name":"dev","conection_type":"ssh","host":"dev.example","port":22,"user_name":"ops","authentication_type":"key"}), "fallback").unwrap();
    assert_eq!(profile["type"], "ssh");
    assert_eq!(profile["username"], "ops");
    assert_eq!(profile["authType"], "privateKey");
}

#[test]
fn preserves_jumpserver_koko_mfa_when_normalizing_compatible_json() {
    let profile = normalize_external_profile(
        &json!({
            "name": "bastion",
            "conection_type": "ssh",
            "host": "jump.example.com",
            "port": 2222,
            "user_name": "alice@root@192.168.1.100@jump.example.com",
            "authentication_type": "JumpServer / KoKo MFA Interactive"
        }),
        "fallback",
    )
    .unwrap();
    assert_eq!(profile["authType"], "jumpserver-koko-mfa");
}

#[test]
fn canonicalizes_display_label_when_auth_type_key_is_used() {
    let profile = normalize_external_profile(
        &json!({
            "name": "bastion",
            "type": "ssh",
            "host": "jump.example.com",
            "port": 2222,
            "username": "alice@root@192.168.1.100@jump.example.com",
            "authType": "JumpServer / KoKo MFA Interactive"
        }),
        "fallback",
    )
    .unwrap();
    assert_eq!(profile["authType"], "jumpserver-koko-mfa");
}

#[test]
fn preserves_kubernetes_auth_mode_when_normalizing_compatible_json() {
    let profile = normalize_external_profile(
        &json!({
            "name": "pod-shell",
            "type": "ssh",
            "host": "ssh.example.com",
            "port": 22,
            "username": "pod-7f8c9",
            "authType": "Kubernetes mode"
        }),
        "fallback",
    )
    .unwrap();
    assert_eq!(profile["authType"], "kubernetes");
}

#[test]
fn normalizes_hosts_and_detects_endpoint_conflicts() {
    let profile = normalize_external_profile(
        &json!({"name":"ipv6","host":" [2001:db8::1] ","port":22}),
        "fallback",
    )
    .unwrap();
    assert_eq!(profile["host"], "2001:db8::1");
    assert!(normalize_external_profile(
        &json!({"name":"bad","host":"ssh://host","port":22}),
        "fallback"
    )
    .is_err());
    assert_eq!(
        fingerprint(
            &json!({"type":"ssh","name":"old","host":"Example.test","port":22,"username":"OPS"})
        ),
        fingerprint(
            &json!({"type":"ssh","name":"new","host":"example.test","port":22,"username":"ops"})
        )
    );
}

#[test]
fn creates_unique_cross_platform_export_names() {
    let mut used = HashSet::new();
    assert_eq!(
        export_filename("CON", "abcdefgh", &mut used),
        "connection-CON-abcdefgh"
    );
    assert_eq!(
        export_filename("same", "12345678", &mut used),
        "same-12345678"
    );
    assert_eq!(
        export_filename("same", "87654321", &mut used),
        "same-2-87654321"
    );
}

#[test]
fn exports_saved_credentials_in_both_supported_formats() {
    let profiles = vec![json!({
        "id": "profile-1",
        "name": "dev",
        "type": "ssh",
        "host": "example.test",
        "port": 22,
        "username": "ops",
        "password": "secret",
        "passphrase": "key-secret"
    })];
    let native = build_export_payload(&profiles, "fileterm").unwrap();
    assert_eq!(native["containsSecrets"], true);
    assert_eq!(native["profiles"][0]["password"], "secret");
    assert_eq!(native["profiles"][0]["passphrase"], "key-secret");

    let compatible = build_export_payload(&profiles, "compatible").unwrap();
    assert_eq!(compatible[0]["password"], "secret");
    assert_eq!(compatible[0]["passphrase"], "key-secret");
}

#[test]
fn compatible_export_preserves_ssh_device_settings() {
    let profiles = vec![json!({
        "id": "profile-network-device",
        "name": "switch",
        "type": "ssh",
        "host": "switch.example",
        "port": 22,
        "username": "ops",
        "deviceMode": "network-device",
        "terminalType": "vt100",
        "networkDeviceVendor": "huawei"
    })];

    let single = build_compatible_profile_payload(&profiles[0]);
    let compatible = build_export_payload(&profiles, "compatible").unwrap();
    assert_eq!(single, compatible[0]);
    assert_eq!(compatible[0]["deviceMode"], "network-device");
    assert_eq!(compatible[0]["terminalType"], "vt100");
    assert_eq!(compatible[0]["networkDeviceVendor"], "huawei");

    let imported = normalize_external_profile(&compatible[0], "fallback").unwrap();
    assert_eq!(imported["deviceMode"], "network-device");
    assert_eq!(imported["terminalType"], "vt100");
    assert_eq!(imported["networkDeviceVendor"], "huawei");
}

/// Build a unique scratch directory under the system temp dir. Each test
/// gets its own path so parallel `cargo test` runs don't stomp on each
/// other. The directory is intentionally not cleaned up — tests are
/// short-lived and the OS reaps temp files.
fn scratch_dir(label: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "fileterm-import-test-{}-{}-{}",
        std::process::id(),
        label,
        rand::random::<u32>()
    ));
    fs::create_dir_all(&path).expect("create scratch dir");
    path
}

fn write_file(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&path, contents).expect("write scratch file");
    path
}

#[test]
fn collect_import_files_skips_symlink_loops() {
    // Regression for M2: a symlinked directory loop must not send the
    // recursion into a stack overflow. The scan should simply skip the
    // symlink and return the real files in the root.
    let root = scratch_dir("loop");
    write_file(&root, "real.json", "{}");
    // `self` → root, creating a cycle through a symlinked directory.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&root, root.join("self")).expect("create loop symlink");
    }

    let mut files = Vec::new();
    collect_import_files(&root, &mut files).expect("scan must complete");

    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    assert!(
        names.iter().any(|name| name == "real.json"),
        "real file should be collected, got {:?}",
        names
    );
    assert!(
        !names.iter().any(|name| name == "self"),
        "symlinked directory must not be followed"
    );
    // Sanity: the scan didn't duplicate the real file via the loop.
    let real_count = names.iter().filter(|name| name == &"real.json").count();
    assert_eq!(real_count, 1, "real file must only be collected once");
}

#[test]
fn collect_import_files_respects_file_limit() {
    let root = scratch_dir("limit");
    // Create more files than the limit to confirm the scan stops at
    // IMPORT_FILE_LIMIT instead of growing unbounded.
    for index in 0..(IMPORT_FILE_LIMIT + 50) {
        write_file(&root, &format!("file-{index}.json"), "{}");
    }

    let mut files = Vec::new();
    collect_import_files(&root, &mut files).expect("scan must complete");
    assert!(
        files.len() <= IMPORT_FILE_LIMIT,
        "scan must cap at IMPORT_FILE_LIMIT, got {}",
        files.len()
    );
}

#[test]
fn collect_import_files_filters_unsupported_extensions() {
    let root = scratch_dir("filter");
    write_file(&root, "kept.json", "{}");
    write_file(&root, "skipped.bin", "binary");
    write_file(&root, "also-kept.txt", "notes");

    let mut files = Vec::new();
    collect_import_files(&root, &mut files).expect("scan must complete");

    let names: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
        .collect();
    assert!(names.contains(&"kept.json".to_string()));
    assert!(names.contains(&"also-kept.txt".to_string()));
    assert!(!names.contains(&"skipped.bin".to_string()));
}
