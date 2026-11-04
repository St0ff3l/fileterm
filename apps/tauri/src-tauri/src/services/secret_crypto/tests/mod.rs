use super::*;

fn fixture_directory() -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("fileterm-secret-crypto-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).expect("fixture directory should be created");
    directory
}

#[test]
fn encrypt_decrypt_round_trip_is_scope_bound() {
    let directory = fixture_directory();
    let encrypted = encrypt(&directory, "ai/provider-1/api-key", "provider-secret")
        .expect("encryption should succeed");

    assert!(is_encrypted(&encrypted));
    assert!(!encrypted.contains("provider-secret"));
    assert_eq!(
        decrypt(&directory, "ai/provider-1/api-key", &encrypted)
            .expect("matching scope should decrypt"),
        "provider-secret"
    );
    assert!(decrypt(&directory, "ai/provider-2/api-key", &encrypted).is_err());

    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}

#[test]
fn encryption_uses_a_fresh_nonce_and_rejects_tampering() {
    let directory = fixture_directory();
    let first = encrypt(&directory, "scope", "same-value").expect("first encryption");
    let second = encrypt(&directory, "scope", "same-value").expect("second encryption");
    assert_ne!(first, second);

    let mut tampered = first.into_bytes();
    let last = tampered.len() - 1;
    tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
    let tampered = String::from_utf8(tampered).expect("ciphertext stays utf-8");
    assert!(decrypt(&directory, "scope", &tampered).is_err());

    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}

#[test]
fn plaintext_is_marked_for_one_time_migration() {
    let directory = fixture_directory();
    let (value, should_migrate) =
        decrypt_or_migrate(&directory, "scope", "legacy plaintext").expect("legacy read");
    assert_eq!(value, "legacy plaintext");
    assert!(should_migrate);

    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}

#[cfg(unix)]
#[test]
fn installation_seed_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let directory = fixture_directory();
    let _ = encrypt(&directory, "scope", "secret").expect("encryption should create seed");
    let mode = fs::metadata(seed_path(&directory))
        .expect("seed should exist")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}
