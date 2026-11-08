use super::{
    aad, decode_bundle, derive_pbkdf2_key, encrypt_bundle, EncryptionMetadata,
    PBKDF2_DEFAULT_ITERATIONS, PBKDF2_MIN_ITERATIONS,
};
use aes_gcm::{aead::Aead, aead::KeyInit, aead::Payload, Aes256Gcm, Nonce};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::json;

#[test]
fn v3_round_trip_encrypts_profiles_and_ssh_keys() {
    let profiles = vec![json!({
        "name": "prod",
        "type": "ssh",
        "host": "example.test",
        "port": 22,
        "password": "super-secret-password"
    })];
    let ssh_keys = vec![json!({
        "id": "key-1",
        "privateKey": "private-key-content",
        "passphrase": "ssh-key-passphrase"
    })];
    let (bytes, _) = encrypt_bundle(
        profiles.clone(),
        ssh_keys.clone(),
        super::BackupDependencies::default(),
        "Backup password 8",
        "now",
    )
    .expect("v3 encryption should succeed");
    let raw = String::from_utf8(bytes.clone()).expect("envelope is json");
    assert!(raw.contains("\"schemaVersion\": 3"));
    assert!(!raw.contains("super-secret-password"));
    assert!(!raw.contains("private-key-content"));
    assert!(!raw.contains("ssh-key-passphrase"));
    let decoded = decode_bundle(&bytes, Some("Backup password 8")).expect("v3 should decrypt");
    assert!(!decoded.legacy_plaintext);
    assert_eq!(decoded.profiles, profiles);
    assert_eq!(decoded.ssh_keys, Some(ssh_keys));
    let wrong_password = "Wrong password 8";
    assert!(decode_bundle(&bytes, Some(wrong_password)).is_err());
}

#[test]
fn v2_plaintext_bundle_is_still_importable() {
    let profiles = json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
    let hash = {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&profiles).unwrap();
        format!("{:x}", Sha256::digest(bytes))
    };
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "containsSecrets": true,
        "contentHash": hash,
        "profiles": profiles
    }))
    .unwrap();
    let decoded = decode_bundle(&bytes, None).expect("v2 should remain readable");
    assert!(decoded.legacy_plaintext);
    assert_eq!(decoded.profiles.len(), 1);
    assert!(decoded.ssh_keys.is_none());
}

#[test]
fn rejects_unencrypted_bundles_that_contain_ssh_keys() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "profiles": [],
        "sshKeys": [{"privateKey": "private-key-content"}]
    }))
    .unwrap();

    assert!(decode_bundle(&bytes, None).is_err());
}

#[test]
fn pbkdf2_compatibility_iterations_stay_bounded() {
    assert_eq!(PBKDF2_DEFAULT_ITERATIONS, 600_000);
}

#[test]
fn pbkdf2_v3_bundle_is_still_decryptable() {
    let password = "Backup password 8";
    let salt = [7_u8; 16];
    let nonce = [9_u8; 12];
    let metadata = EncryptionMetadata {
        algorithm: "AES-256-GCM".to_string(),
        kdf: "PBKDF2-HMAC-SHA256".to_string(),
        version: Some("sha256".to_string()),
        memory_kib: None,
        iterations: PBKDF2_MIN_ITERATIONS,
        parallelism: None,
        salt: URL_SAFE_NO_PAD.encode(salt),
        nonce: URL_SAFE_NO_PAD.encode(nonce),
    };
    let key = derive_pbkdf2_key(password, metadata.iterations, &salt).unwrap();
    let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
    let profiles = vec![serde_json::json!({ "name": "legacy", "type": "ssh" })];
    let plaintext = serde_json::to_vec(&profiles).unwrap();
    let test_nonce = Nonce::<super::U12>::try_from(nonce.as_slice()).unwrap();
    let ciphertext = cipher
        .encrypt(
            &test_nonce,
            Payload {
                msg: &plaintext,
                aad: &aad(&metadata, "now"),
            },
        )
        .unwrap();
    let envelope = serde_json::json!({
        "schemaVersion": 3,
        "containsSecrets": true,
        "generatedAt": "now",
        "encryption": metadata,
        "ciphertext": URL_SAFE_NO_PAD.encode(&ciphertext),
        "contentHash": super::sha256_hex(&ciphertext),
    });
    let bytes = serde_json::to_vec(&envelope).unwrap();
    let decoded = decode_bundle(&bytes, Some(password)).unwrap();
    assert_eq!(decoded.profiles, profiles);
    assert!(decoded.ssh_keys.is_none());
    assert!(!decoded.legacy_plaintext);
}
