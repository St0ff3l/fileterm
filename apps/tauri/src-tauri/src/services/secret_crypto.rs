//! Device-bound encryption for small local credential fields.
//!
//! This deliberately does not use Keychain, DPAPI, or any other OS credential
//! store: FileTerm must not trigger platform permission prompts just to read a
//! saved connection. The per-install seed is owner-only and combined with a
//! stable machine identifier before it becomes an AES-256-GCM key.

use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::{
    aead::{Aead, Generate, KeyInit, Payload},
    aes::cipher::consts::U12,
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::AppError;

type HmacSha256 = Hmac<Sha256>;

pub(crate) const ENCRYPTED_STORAGE: &str = "fileterm-aes-gcm-v1";
const CIPHERTEXT_PREFIX: &str = "ftsec:v1:";
const SEED_FILE: &str = "secret-store-v1.key";
const SEED_HEADER: &[u8] = b"FTSKv1\0";
const SEED_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;
const KEY_CONTEXT: &[u8] = b"fileterm-local-secret-store-v1\0";

fn encryption_error() -> AppError {
    AppError::Storage("无法加密本机凭据。".to_string())
}

fn decryption_error() -> AppError {
    AppError::Storage("无法解密本机凭据。请在此设备重新配置该凭据。".to_string())
}

fn seed_path(storage_root: &Path) -> PathBuf {
    storage_root.join(SEED_FILE)
}

fn decode_seed(bytes: &[u8]) -> Result<[u8; SEED_BYTES], AppError> {
    if bytes.len() != SEED_HEADER.len() + SEED_BYTES || !bytes.starts_with(SEED_HEADER) {
        return Err(encryption_error());
    }
    let mut seed = [0_u8; SEED_BYTES];
    seed.copy_from_slice(&bytes[SEED_HEADER.len()..]);
    Ok(seed)
}

fn create_seed(storage_root: &Path, path: &Path) -> Result<[u8; SEED_BYTES], AppError> {
    fs::create_dir_all(storage_root).map_err(|error| AppError::Storage(error.to_string()))?;
    let generated = aes_gcm::Key::<Aes256Gcm>::generate();
    let mut seed = [0_u8; SEED_BYTES];
    seed.copy_from_slice(&generated);

    let mut content = Vec::with_capacity(SEED_HEADER.len() + SEED_BYTES);
    content.extend_from_slice(SEED_HEADER);
    content.extend_from_slice(&seed);
    let temporary = path.with_file_name(format!(".{SEED_FILE}.{}.tmp", uuid::Uuid::new_v4()));

    match crate::storage::write_restricted_file(&temporary, &content) {
        Ok(()) => match crate::storage::replace_file_atomically(&temporary, path) {
            Ok(()) => Ok(seed),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error)
            }
        },
        Err(_error) if path.exists() => {
            let _ = fs::remove_file(&temporary);
            fs::read(path)
                .map_err(|read_error| AppError::Storage(read_error.to_string()))
                .and_then(|existing| decode_seed(&existing))
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error)
        }
    }
}

fn load_or_create_seed(storage_root: &Path) -> Result<[u8; SEED_BYTES], AppError> {
    let path = seed_path(storage_root);
    match fs::read(&path) {
        Ok(seed) => decode_seed(&seed),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            create_seed(storage_root, &path)
        }
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

#[cfg(target_os = "linux")]
fn machine_identifier() -> Vec<u8> {
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(value) = fs::read_to_string(path) {
            let value = value.trim();
            if !value.is_empty() {
                return value.as_bytes().to_vec();
            }
        }
    }
    b"linux-machine-id-unavailable".to_vec()
}

#[cfg(target_os = "macos")]
fn machine_identifier() -> Vec<u8> {
    let mut identifier = [0_u8; 16];
    // SAFETY: `gethostuuid` writes exactly sixteen bytes to the provided UUID
    // buffer and the null timeout asks the OS for its normal host UUID lookup.
    if unsafe { libc::gethostuuid(identifier.as_mut_ptr(), std::ptr::null()) } == 0 {
        return identifier.to_vec();
    }
    b"macos-host-uuid-unavailable".to_vec()
}

#[cfg(target_os = "windows")]
fn machine_identifier() -> Vec<u8> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let subkey: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0"
        .encode_utf16()
        .collect();
    let value_name: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
    let mut buffer = [0_u16; 128];
    let mut size = (buffer.len() * std::mem::size_of::<u16>()) as u32;
    // SAFETY: the registry API receives valid NUL-terminated UTF-16 strings
    // and a writable buffer with its byte length in `size`.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value_name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status == 0 {
        let length = (size as usize / std::mem::size_of::<u16>()).min(buffer.len());
        let value = String::from_utf16_lossy(&buffer[..length]);
        let value = value.trim_end_matches('\0').trim();
        if !value.is_empty() {
            return value.as_bytes().to_vec();
        }
    }
    b"windows-machine-guid-unavailable".to_vec()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn machine_identifier() -> Vec<u8> {
    b"fileterm-machine-identifier-unavailable".to_vec()
}

fn derive_key(storage_root: &Path) -> Result<[u8; SEED_BYTES], AppError> {
    let mut seed = load_or_create_seed(storage_root)?;
    let mut mac = HmacSha256::new_from_slice(&seed).map_err(|_| encryption_error())?;
    seed.zeroize();
    mac.update(KEY_CONTEXT);
    mac.update(&machine_identifier());
    let mut digest = mac.finalize().into_bytes();
    let mut key = [0_u8; SEED_BYTES];
    key.copy_from_slice(&digest);
    digest.zeroize();
    Ok(key)
}

pub(crate) fn is_encrypted(value: &str) -> bool {
    value.starts_with(CIPHERTEXT_PREFIX)
}

pub(crate) fn encrypt(
    storage_root: &Path,
    scope: &str,
    plaintext: &str,
) -> Result<String, AppError> {
    let mut key = derive_key(storage_root)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| encryption_error())?;
    key.zeroize();
    let nonce = Nonce::<U12>::generate();
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext.as_bytes(),
                aad: scope.as_bytes(),
            },
        )
        .map_err(|_| encryption_error())?;
    let mut payload = Vec::with_capacity(NONCE_BYTES + ciphertext.len());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&ciphertext);
    Ok(format!(
        "{CIPHERTEXT_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(payload)
    ))
}

pub(crate) fn decrypt(
    storage_root: &Path,
    scope: &str,
    ciphertext: &str,
) -> Result<String, AppError> {
    let encoded = ciphertext
        .strip_prefix(CIPHERTEXT_PREFIX)
        .ok_or_else(decryption_error)?;
    let payload = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| decryption_error())?;
    if payload.len() < NONCE_BYTES + TAG_BYTES {
        return Err(decryption_error());
    }
    let mut key = derive_key(storage_root)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| decryption_error())?;
    key.zeroize();
    let nonce = Nonce::<U12>::try_from(&payload[..NONCE_BYTES]).map_err(|_| decryption_error())?;
    let plaintext = cipher
        .decrypt(
            &nonce,
            Payload {
                msg: &payload[NONCE_BYTES..],
                aad: scope.as_bytes(),
            },
        )
        .map_err(|_| decryption_error())?;
    String::from_utf8(plaintext).map_err(|_| decryption_error())
}

/// Returns the cleartext value together with whether a plaintext legacy value
/// should be persisted again through the encrypted writer.
pub(crate) fn decrypt_or_migrate(
    storage_root: &Path,
    scope: &str,
    value: &str,
) -> Result<(String, bool), AppError> {
    if is_encrypted(value) {
        Ok((decrypt(storage_root, scope, value)?, false))
    } else {
        Ok((value.to_string(), true))
    }
}

#[cfg(test)]
mod tests {
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
}
