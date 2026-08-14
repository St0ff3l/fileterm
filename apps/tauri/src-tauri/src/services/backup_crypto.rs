//! Cross-device encryption for WebDAV/S3 connection backups.
//!
//! This format is deliberately separate from `secret_crypto`: the latter is
//! device-bound storage for local configuration fields, while this module
//! derives a fresh key from a password so a backup can be restored elsewhere.
//! User-initiated JSON export does not use this module and remains plaintext.

use aes_gcm::{
    aead::{Aead, Generate, KeyInit, Payload},
    aes::cipher::consts::U12,
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

type HmacSha256 = Hmac<Sha256>;

pub(crate) const SCHEMA_VERSION: u8 = 3;
const AAD_PREFIX: &[u8] = b"fileterm-remote-backup-v3";
const KEY_BYTES: usize = 32;
const SALT_BYTES: usize = 16;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;
pub(crate) const MIN_BACKUP_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_BYTES: usize = 8 * 1024;

// Argon2id target: 64 MiB, three passes, one lane. The actual latency is
// platform-dependent, but this keeps password guessing materially more
// expensive without making routine backup sync feel blocked on desktop CPUs.
const ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;

// Accepted only for compatibility with packages created by an older or
// external implementation. New uploads always use Argon2id above.
const PBKDF2_MIN_ITERATIONS: u32 = 100_000;
const PBKDF2_MAX_ITERATIONS: u32 = 1_000_000;
#[cfg(test)]
const PBKDF2_DEFAULT_ITERATIONS: u32 = 600_000;

#[derive(Debug, Error)]
pub(crate) enum BackupCryptoError {
    #[error("远程备份需要输入主密码。")]
    PasswordRequired,
    #[error("备份主密码至少需要 {MIN_BACKUP_PASSWORD_CHARS} 个字符。")]
    PasswordTooShort,
    #[error("备份主密码必须同时包含大写字母和小写字母。")]
    PasswordMustContainUpperAndLowerCase,
    #[error("备份主密码无效或备份包已损坏。")]
    InvalidPasswordOrBundle,
    #[error("远程备份格式不受支持。")]
    UnsupportedFormat,
    #[error("远程备份内容无效。")]
    InvalidBundle,
    #[error("远程备份加密参数无效。")]
    InvalidParameters,
    #[error("远程备份加密失败。")]
    EncryptionFailed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EncryptionMetadata {
    algorithm: String,
    kdf: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_kib: Option<u32>,
    iterations: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parallelism: Option<u32>,
    salt: String,
    nonce: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedBundle {
    schema_version: u8,
    contains_secrets: bool,
    generated_at: String,
    encryption: EncryptionMetadata,
    ciphertext: String,
    content_hash: String,
}

#[derive(Debug)]
pub(crate) struct DecodedBundle {
    pub profiles: Vec<Value>,
    pub legacy_plaintext: bool,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let generated = aes_gcm::Key::<Aes256Gcm>::generate();
    let mut output = [0_u8; N];
    output.copy_from_slice(&generated[..N]);
    output
}

fn validate_password(password: &str) -> Result<(), BackupCryptoError> {
    if password.chars().count() < MIN_BACKUP_PASSWORD_CHARS {
        return Err(BackupCryptoError::PasswordTooShort);
    }
    if !password
        .chars()
        .any(|character| character.is_ascii_uppercase())
        || !password
            .chars()
            .any(|character| character.is_ascii_lowercase())
    {
        return Err(BackupCryptoError::PasswordMustContainUpperAndLowerCase);
    }
    if password.is_empty()
        || password.len() > MAX_PASSWORD_BYTES
        || password
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n' | '\u{1b}'))
    {
        return Err(BackupCryptoError::InvalidPasswordOrBundle);
    }
    Ok(())
}

fn decode_base64(value: &str, expected_len: usize) -> Result<Vec<u8>, BackupCryptoError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| BackupCryptoError::InvalidBundle)?;
    if decoded.len() != expected_len {
        return Err(BackupCryptoError::InvalidBundle);
    }
    Ok(decoded)
}

fn aad(metadata: &EncryptionMetadata, generated_at: &str) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        String::from_utf8_lossy(AAD_PREFIX),
        generated_at,
        metadata.algorithm,
        metadata.kdf,
        metadata.version.as_deref().unwrap_or_default(),
        metadata.memory_kib.unwrap_or_default(),
        metadata.iterations,
        metadata.parallelism.unwrap_or_default(),
        metadata.salt,
        metadata.nonce
    )
    .into_bytes()
}

fn derive_argon2_key(
    password: &str,
    metadata: &EncryptionMetadata,
    salt: &[u8],
) -> Result<[u8; KEY_BYTES], BackupCryptoError> {
    let memory_kib = metadata
        .memory_kib
        .ok_or(BackupCryptoError::InvalidParameters)?;
    let parallelism = metadata
        .parallelism
        .ok_or(BackupCryptoError::InvalidParameters)?;
    if !(1..=16).contains(&parallelism)
        || !(8 * parallelism..=256 * 1024).contains(&memory_kib)
        || !(1..=10).contains(&metadata.iterations)
        || metadata.version.as_deref() != Some("0x13")
    {
        return Err(BackupCryptoError::InvalidParameters);
    }
    let params = Params::new(
        memory_kib,
        metadata.iterations,
        parallelism,
        Some(KEY_BYTES),
    )
    .map_err(|_| BackupCryptoError::InvalidParameters)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0_u8; KEY_BYTES];
    let mut password_bytes = password.as_bytes().to_vec();
    let result = argon.hash_password_into(&password_bytes, salt, &mut key);
    password_bytes.zeroize();
    result.map_err(|_| {
        key.zeroize();
        BackupCryptoError::InvalidPasswordOrBundle
    })?;
    Ok(key)
}

fn derive_pbkdf2_key(
    password: &str,
    iterations: u32,
    salt: &[u8],
) -> Result<[u8; KEY_BYTES], BackupCryptoError> {
    if !(PBKDF2_MIN_ITERATIONS..=PBKDF2_MAX_ITERATIONS).contains(&iterations) {
        return Err(BackupCryptoError::InvalidParameters);
    }
    let mut key = [0_u8; KEY_BYTES];
    let mut block = Vec::with_capacity(salt.len() + 4);
    block.extend_from_slice(salt);
    block.extend_from_slice(&1_u32.to_be_bytes());

    let mut mac = HmacSha256::new_from_slice(password.as_bytes())
        .map_err(|_| BackupCryptoError::InvalidPasswordOrBundle)?;
    mac.update(&block);
    let mut u = mac.finalize().into_bytes();
    key.copy_from_slice(&u);

    for _ in 1..iterations {
        let mut next_mac = HmacSha256::new_from_slice(password.as_bytes())
            .map_err(|_| BackupCryptoError::InvalidPasswordOrBundle)?;
        next_mac.update(&u);
        let next = next_mac.finalize().into_bytes();
        for (target, source) in key.iter_mut().zip(next.iter()) {
            *target ^= source;
        }
        u = next;
    }
    u.zeroize();
    block.zeroize();
    Ok(key)
}

fn derive_key(
    password: &str,
    metadata: &EncryptionMetadata,
    salt: &[u8],
) -> Result<[u8; KEY_BYTES], BackupCryptoError> {
    validate_password(password)?;
    match metadata.kdf.as_str() {
        "Argon2id" => derive_argon2_key(password, metadata, salt),
        "PBKDF2-HMAC-SHA256" => derive_pbkdf2_key(password, metadata.iterations, salt),
        _ => Err(BackupCryptoError::InvalidParameters),
    }
}

fn extract_profiles(value: Value) -> Result<Vec<Value>, BackupCryptoError> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Object(object) => object
            .get("profiles")
            .and_then(Value::as_array)
            .cloned()
            .ok_or(BackupCryptoError::InvalidBundle),
        _ => Err(BackupCryptoError::InvalidBundle),
    }
}

fn decode_legacy(value: Value) -> Result<DecodedBundle, BackupCryptoError> {
    let profiles = extract_profiles(value.clone())?;
    if let Some(expected_hash) = value.get("contentHash").and_then(Value::as_str) {
        let canonical =
            serde_json::to_vec(&profiles).map_err(|_| BackupCryptoError::InvalidBundle)?;
        if sha256_hex(&canonical) != expected_hash {
            return Err(BackupCryptoError::InvalidPasswordOrBundle);
        }
    }
    Ok(DecodedBundle {
        profiles,
        legacy_plaintext: true,
    })
}

fn decrypt_v3(
    envelope: EncryptedBundle,
    password: Option<&str>,
) -> Result<DecodedBundle, BackupCryptoError> {
    if envelope.schema_version != SCHEMA_VERSION
        || !envelope.contains_secrets
        || envelope.generated_at.trim().is_empty()
        || envelope.encryption.algorithm != "AES-256-GCM"
    {
        return Err(BackupCryptoError::InvalidBundle);
    }
    let password = password.ok_or(BackupCryptoError::PasswordRequired)?;
    let salt = decode_base64(&envelope.encryption.salt, SALT_BYTES)?;
    let nonce_bytes = decode_base64(&envelope.encryption.nonce, NONCE_BYTES)?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&envelope.ciphertext)
        .map_err(|_| BackupCryptoError::InvalidBundle)?;
    if ciphertext.len() < TAG_BYTES || sha256_hex(&ciphertext) != envelope.content_hash {
        return Err(BackupCryptoError::InvalidPasswordOrBundle);
    }
    let mut key = derive_key(password, &envelope.encryption, &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| BackupCryptoError::InvalidPasswordOrBundle)?;
    key.zeroize();
    let nonce = Nonce::<U12>::try_from(nonce_bytes.as_slice())
        .map_err(|_| BackupCryptoError::InvalidBundle)?;
    let metadata_aad = aad(&envelope.encryption, &envelope.generated_at);
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &ciphertext,
                aad: &metadata_aad,
            },
        )
        .map_err(|_| BackupCryptoError::InvalidPasswordOrBundle)?;
    let plaintext = Zeroizing::new(plaintext);
    let value: Value = serde_json::from_slice(&plaintext)
        .map_err(|_| BackupCryptoError::InvalidPasswordOrBundle)?;
    let profiles = extract_profiles(value)?;
    Ok(DecodedBundle {
        profiles,
        legacy_plaintext: false,
    })
}

pub(crate) fn encrypt_profiles(
    profiles: &[Value],
    password: &str,
    generated_at: &str,
) -> Result<(Vec<u8>, String), BackupCryptoError> {
    validate_password(password)?;
    let salt = random_bytes::<SALT_BYTES>();
    let nonce = random_bytes::<NONCE_BYTES>();
    let metadata = EncryptionMetadata {
        algorithm: "AES-256-GCM".to_string(),
        kdf: "Argon2id".to_string(),
        version: Some("0x13".to_string()),
        memory_kib: Some(ARGON2_MEMORY_KIB),
        iterations: ARGON2_ITERATIONS,
        parallelism: Some(ARGON2_PARALLELISM),
        salt: URL_SAFE_NO_PAD.encode(salt),
        nonce: URL_SAFE_NO_PAD.encode(nonce),
    };
    let mut key = derive_argon2_key(password, &metadata, &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|_| BackupCryptoError::EncryptionFailed)?;
    key.zeroize();
    let plaintext =
        serde_json::to_vec(profiles).map_err(|_| BackupCryptoError::EncryptionFailed)?;
    let metadata_aad = aad(&metadata, generated_at);
    let nonce = Nonce::<U12>::try_from(nonce.as_slice())
        .map_err(|_| BackupCryptoError::EncryptionFailed)?;
    let plaintext = Zeroizing::new(plaintext);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: &plaintext,
                aad: &metadata_aad,
            },
        )
        .map_err(|_| BackupCryptoError::EncryptionFailed)?;
    let content_hash = sha256_hex(&ciphertext);
    let envelope = serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "containsSecrets": true,
        "generatedAt": generated_at,
        "encryption": metadata,
        "ciphertext": URL_SAFE_NO_PAD.encode(&ciphertext),
        "contentHash": content_hash,
    });
    let bytes =
        serde_json::to_vec_pretty(&envelope).map_err(|_| BackupCryptoError::EncryptionFailed)?;
    let bundle_hash = sha256_hex(&bytes);
    Ok((bytes, bundle_hash))
}

pub(crate) fn decode_bundle(
    bytes: &[u8],
    password: Option<&str>,
) -> Result<DecodedBundle, BackupCryptoError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| BackupCryptoError::InvalidBundle)?;
    let schema_version = value.get("schemaVersion").and_then(Value::as_u64);
    match schema_version {
        Some(version) if version == SCHEMA_VERSION as u64 => {
            let envelope: EncryptedBundle =
                serde_json::from_value(value).map_err(|_| BackupCryptoError::InvalidBundle)?;
            decrypt_v3(envelope, password)
        }
        Some(1) | Some(2) | None => decode_legacy(value),
        Some(_) => Err(BackupCryptoError::UnsupportedFormat),
    }
}

pub(crate) fn requires_password(bytes: &[u8]) -> Result<bool, BackupCryptoError> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| BackupCryptoError::InvalidBundle)?;
    Ok(value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .is_some_and(|version| version == SCHEMA_VERSION as u64))
}

#[cfg(test)]
mod tests {
    use super::{
        aad, decode_bundle, derive_pbkdf2_key, encrypt_profiles, EncryptionMetadata,
        PBKDF2_DEFAULT_ITERATIONS, PBKDF2_MIN_ITERATIONS,
    };
    use aes_gcm::{aead::Aead, aead::KeyInit, aead::Payload, Aes256Gcm, Nonce};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;

    #[test]
    fn v3_round_trip_does_not_expose_profile_secrets() {
        let profiles = vec![json!({
            "name": "prod",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "password": "super-secret-password"
        })];
        let (bytes, _) = encrypt_profiles(&profiles, "Backup password 8", "now")
            .expect("v3 encryption should succeed");
        let raw = String::from_utf8(bytes.clone()).expect("envelope is json");
        assert!(raw.contains("\"schemaVersion\": 3"));
        assert!(!raw.contains("super-secret-password"));
        let decoded = decode_bundle(&bytes, Some("Backup password 8")).expect("v3 should decrypt");
        assert!(!decoded.legacy_plaintext);
        assert_eq!(decoded.profiles, profiles);
        let wrong_password = "Wrong password 8";
        assert!(decode_bundle(&bytes, Some(wrong_password)).is_err());
    }

    #[test]
    fn v2_plaintext_bundle_is_still_importable() {
        let profiles =
            json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
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
        assert!(!decoded.legacy_plaintext);
    }
}
