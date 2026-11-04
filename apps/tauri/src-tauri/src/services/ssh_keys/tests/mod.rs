#[cfg(unix)]
use super::write_bytes_atomic;
use super::{
    inspect_private_key_text, probably_encrypted, read_secrets, KeyPaths, StoredSshKeySecrets,
    MAX_PRIVATE_KEY_BYTES,
};
use std::collections::HashMap;
use std::fs;

#[test]
fn inspects_pasted_openssh_private_key_text() {
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let text = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();

    let inspected = inspect_private_key_text(&text).unwrap();

    assert_eq!(inspected.name, "pasted-key");
    assert_eq!(inspected.algorithm, "ssh-ed25519");
    assert!(!inspected.encrypted);
    assert_eq!(inspected.bytes, text.as_bytes());
}

#[test]
fn rejects_invalid_or_oversized_private_key_text() {
    assert!(inspect_private_key_text("not a private key").is_err());

    let oversized = "x".repeat(MAX_PRIVATE_KEY_BYTES as usize + 1);
    let error = match inspect_private_key_text(&oversized) {
        Err(error) => error,
        Ok(_) => panic!("oversized private key text should be rejected"),
    };
    assert!(error.to_string().contains("超过 1 MB"));
}

#[test]
fn plaintext_ssh_key_passphrase_is_migrated_to_encrypted_storage() {
    let directory =
        std::env::temp_dir().join(format!("fileterm-ssh-key-secret-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).expect("fixture directory should be created");
    let paths = KeyPaths {
        storage_root: directory.clone(),
        keys_dir: directory.join("ssh-keys"),
        index_path: directory.join("ssh-keys.json"),
        secrets_path: directory.join("ssh-key-secrets.json"),
    };
    let legacy = StoredSshKeySecrets {
        version: 1,
        passphrases: HashMap::from([("key-1".to_string(), "ssh-key-passphrase".to_string())]),
    };
    fs::write(
        &paths.secrets_path,
        serde_json::to_vec(&legacy).expect("legacy secrets json"),
    )
    .expect("legacy secrets write");

    let secrets = read_secrets(&paths).expect("legacy secrets read");
    assert_eq!(
        secrets.passphrases.get("key-1").map(String::as_str),
        Some("ssh-key-passphrase")
    );
    let raw = fs::read_to_string(&paths.secrets_path).expect("migrated secrets read");
    assert!(!raw.contains("ssh-key-passphrase"));

    let decoded = read_secrets(&paths).expect("encrypted secrets read");
    assert_eq!(
        decoded.passphrases.get("key-1").map(String::as_str),
        Some("ssh-key-passphrase")
    );
    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}

#[test]
fn recognizes_private_key_headers_without_treating_public_keys_as_private() {
    assert!(!probably_encrypted("-----BEGIN RSA PRIVATE KEY-----"));
    assert!(probably_encrypted("-----BEGIN OPENSSH PRIVATE KEY-----"));
}

#[cfg(unix)]
#[test]
fn managed_plaintext_key_files_replace_atomically_with_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "fileterm-managed-key-permissions-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("managed.key");

    write_bytes_atomic(&path, b"first plaintext key").unwrap();
    write_bytes_atomic(&path, b"replacement plaintext key").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"replacement plaintext key");
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    std::fs::remove_dir_all(root).unwrap();
}
