use std::collections::{HashMap, HashSet};
use std::fs;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;
use zeroize::Zeroize;

use super::{
    ensure_store, inspect_private_key, inspect_private_key_bytes, lock_down_file, read_index,
    read_secrets, write_bytes_atomic, write_index, write_secrets, InspectedPrivateKey, KeyPaths,
    StoredSshKey,
};
use crate::services::backup_crypto::zeroize_values;
use crate::AppError;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupSshKey {
    #[serde(flatten)]
    pub key: StoredSshKey,
    pub private_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
}

impl Drop for BackupSshKey {
    fn drop(&mut self) {
        self.private_key.zeroize();
        if let Some(passphrase) = &mut self.passphrase {
            passphrase.zeroize();
        }
    }
}

#[derive(Default)]
pub(crate) struct SshKeyRestoreSummary {
    pub imported: u64,
    pub updated: u64,
}

pub(crate) fn export_for_backup(app: &AppHandle) -> Result<Vec<Value>, AppError> {
    let paths = ensure_store(app)?;
    let index = read_index(&paths)?;
    let secrets = read_secrets(&paths)?;
    let mut backups = Vec::with_capacity(index.keys.len());

    for key in index.keys {
        let key_path = paths.key_path(&key.id)?;
        lock_down_file(&key_path)?;
        let mut inspected = inspect_private_key(&key_path)?;
        ensure_key_metadata_matches(&key, &inspected)?;
        let private_key = String::from_utf8(std::mem::take(&mut inspected.bytes))
            .map_err(|error| AppError::Storage(format!("SSH 私钥编码无效: {error}")))?;
        backups.push(BackupSshKey {
            passphrase: secrets.passphrases.get(&key.id).cloned(),
            key,
            private_key,
        });
    }

    validate_backup_records(&backups)?;
    backups
        .iter()
        .map(|backup| {
            serde_json::to_value(backup).map_err(|error| AppError::Serialization(error.to_string()))
        })
        .collect()
}

pub(crate) fn merge_for_upload(
    remote: Option<&[Value]>,
    mut local: Vec<Value>,
) -> Result<Vec<Value>, AppError> {
    let mut remote = remote.unwrap_or_default().to_vec();
    if let Err(error) = validate_backup_values(&remote) {
        zeroize_values(&mut remote);
        zeroize_values(&mut local);
        return Err(error);
    }
    if let Err(error) = validate_backup_values(&local) {
        zeroize_values(&mut remote);
        zeroize_values(&mut local);
        return Err(error);
    }

    let mut positions = HashMap::with_capacity(remote.len() + local.len());
    for (position, value) in remote.iter().enumerate() {
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .expect("validated SSH key has an id")
            .to_string();
        positions.insert(id, position);
    }

    let result = (|| {
        for value in &local {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .expect("validated SSH key has an id");
            if let Some(position) = positions.get(id).copied() {
                if remote[position].get("fingerprint") != value.get("fingerprint") {
                    return Err(AppError::Command(
                        "云端与本地存在 ID 相同但内容不同的 SSH 密钥，已停止合并。".to_string(),
                    ));
                }
                remote[position] = value.clone();
            } else {
                positions.insert(id.to_string(), remote.len());
                remote.push(value.clone());
            }
        }
        Ok(())
    })();
    zeroize_values(&mut local);
    match result {
        Ok(()) => Ok(remote),
        Err(error) => {
            zeroize_values(&mut remote);
            Err(error)
        }
    }
}

pub(crate) fn restore_for_backup(
    app: &AppHandle,
    incoming: &[Value],
) -> Result<SshKeyRestoreSummary, AppError> {
    let backups: Vec<BackupSshKey> = serde_json::from_value(Value::Array(incoming.to_vec()))
        .map_err(|error| AppError::Command(format!("备份中的 SSH 密钥格式无效: {error}")))?;
    validate_backup_records(&backups)?;
    let paths = ensure_store(app)?;
    restore_backup_at(&paths, &backups)
}

fn validate_backup_values(values: &[Value]) -> Result<(), AppError> {
    let backups: Vec<BackupSshKey> = serde_json::from_value(Value::Array(values.to_vec()))
        .map_err(|error| AppError::Command(format!("备份中的 SSH 密钥格式无效: {error}")))?;
    validate_backup_records(&backups)
}

fn validate_backup_records(backups: &[BackupSshKey]) -> Result<(), AppError> {
    let mut ids = HashSet::with_capacity(backups.len());
    for backup in backups {
        uuid::Uuid::parse_str(&backup.key.id)
            .map_err(|_| AppError::Command("备份包含无效的 SSH 密钥 ID。".to_string()))?;
        if !ids.insert(&backup.key.id) {
            return Err(AppError::Command(
                "备份包含重复的 SSH 密钥 ID。".to_string(),
            ));
        }
        let inspected = inspect_private_key_bytes(
            backup.private_key.as_bytes().to_vec(),
            backup.key.name.clone(),
            "备份中的 SSH 私钥",
        )?;
        ensure_key_metadata_matches(&backup.key, &inspected)?;
    }
    Ok(())
}

fn ensure_key_metadata_matches(
    key: &StoredSshKey,
    inspected: &InspectedPrivateKey,
) -> Result<(), AppError> {
    if key.algorithm != inspected.algorithm
        || key.fingerprint != inspected.fingerprint
        || key.encrypted != inspected.encrypted
    {
        return Err(AppError::Command(
            "SSH 私钥内容与密钥元数据不匹配。".to_string(),
        ));
    }
    Ok(())
}

fn restore_backup_at(
    paths: &KeyPaths,
    backups: &[BackupSshKey],
) -> Result<SshKeyRestoreSummary, AppError> {
    let mut index = read_index(paths)?;
    let mut secrets = read_secrets(paths)?;
    let previous_secrets = secrets.clone();
    let mut positions = HashMap::with_capacity(index.keys.len() + backups.len());
    for (position, key) in index.keys.iter().enumerate() {
        if positions.insert(key.id.clone(), position).is_some() {
            return Err(AppError::Storage("SSH 密钥索引包含重复 ID。".to_string()));
        }
    }

    let mut summary = SshKeyRestoreSummary::default();
    let mut new_keys = Vec::new();
    let mut files_to_write = Vec::new();
    let mut index_changed = false;
    let mut secrets_changed = false;

    for backup in backups {
        let existing_position = positions.get(&backup.key.id).copied();
        if let Some(position) = existing_position {
            let existing = &index.keys[position];
            if existing.fingerprint != backup.key.fingerprint {
                return Err(AppError::Command(
                    "本地与备份存在 ID 相同但内容不同的 SSH 密钥，已停止导入。".to_string(),
                ));
            }
        }

        let key_path = paths.key_path(&backup.key.id)?;
        let file_exists = key_path.exists();
        if file_exists {
            lock_down_file(&key_path)?;
            let local_key = inspect_private_key(&key_path)?;
            if local_key.fingerprint != backup.key.fingerprint {
                return Err(AppError::Command(
                    "本地 SSH 私钥文件与备份内容冲突，已停止导入。".to_string(),
                ));
            }
        } else {
            files_to_write.push((key_path, backup.private_key.as_str()));
        }

        let mut key_updated = !file_exists && existing_position.is_some();
        if let Some(position) = existing_position {
            if index.keys[position] != backup.key {
                index.keys[position] = backup.key.clone();
                index_changed = true;
                key_updated = true;
            }
        } else {
            positions.insert(backup.key.id.clone(), index.keys.len() + new_keys.len());
            new_keys.push(backup.key.clone());
            summary.imported += 1;
            index_changed = true;
        }

        let next_passphrase = backup
            .passphrase
            .as_deref()
            .filter(|value| !value.is_empty());
        if secrets.passphrases.get(&backup.key.id).map(String::as_str) != next_passphrase {
            if let Some(mut previous) = secrets.passphrases.remove(&backup.key.id) {
                previous.zeroize();
            }
            if let Some(passphrase) = next_passphrase {
                secrets
                    .passphrases
                    .insert(backup.key.id.clone(), passphrase.to_string());
            }
            secrets_changed = true;
            key_updated = true;
        }
        if key_updated && existing_position.is_some() {
            summary.updated += 1;
        }
    }

    index.keys.splice(0..0, new_keys);
    let mut created_files = Vec::new();
    for (path, private_key) in files_to_write {
        if let Err(error) = write_bytes_atomic(&path, private_key.as_bytes()) {
            for created in &created_files {
                let _ = fs::remove_file(created);
            }
            return Err(error);
        }
        created_files.push(path);
    }

    if secrets_changed {
        if let Err(error) = write_secrets(paths, &secrets) {
            for created in &created_files {
                let _ = fs::remove_file(created);
            }
            return Err(error);
        }
    }
    if index_changed {
        if let Err(error) = write_index(paths, &index) {
            if secrets_changed {
                let _ = write_secrets(paths, &previous_secrets);
            }
            for created in &created_files {
                let _ = fs::remove_file(created);
            }
            return Err(error);
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests;
