use super::super::{
    inspect_private_key_text, read_secrets, KeyPaths, StoredSshKey, StoredSshKeyIndex,
    StoredSshKeySecrets,
};
use super::{restore_backup_at, validate_backup_records, BackupSshKey};
use crate::services::backup_crypto::zeroize_values;
use std::fs;

#[test]
fn restores_validated_private_key_and_encrypts_its_saved_passphrase_locally() {
    let root =
        std::env::temp_dir().join(format!("fileterm-ssh-key-backup-{}", uuid::Uuid::new_v4()));
    let keys_dir = root.join("ssh-keys");
    fs::create_dir_all(&keys_dir).unwrap();
    let paths = KeyPaths {
        storage_root: root.clone(),
        keys_dir,
        index_path: root.join("ssh-keys.json"),
        secrets_path: root.join("ssh-key-secrets.json"),
    };
    super::write_index(
        &paths,
        &StoredSshKeyIndex {
            version: 1,
            keys: Vec::new(),
        },
    )
    .unwrap();
    super::write_secrets(&paths, &StoredSshKeySecrets::default()).unwrap();

    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let private_key = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let inspected = inspect_private_key_text(&private_key).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let backup = BackupSshKey {
        key: StoredSshKey {
            id: id.clone(),
            name: "id_ed25519".to_string(),
            note: Some("work".to_string()),
            algorithm: inspected.algorithm.clone(),
            fingerprint: inspected.fingerprint.clone(),
            encrypted: inspected.encrypted,
            imported_at: 123,
        },
        private_key: private_key.to_string(),
        passphrase: Some("key passphrase".to_string()),
    };
    validate_backup_records(std::slice::from_ref(&backup)).unwrap();

    let summary = restore_backup_at(&paths, std::slice::from_ref(&backup)).unwrap();
    assert_eq!(summary.imported, 1);
    assert_eq!(summary.updated, 0);
    assert_eq!(
        fs::read_to_string(paths.key_path(&id).unwrap()).unwrap(),
        private_key.as_str()
    );
    assert_eq!(
        read_secrets(&paths)
            .unwrap()
            .passphrases
            .get(&id)
            .map(String::as_str),
        Some("key passphrase")
    );
    let raw_secrets = fs::read_to_string(&paths.secrets_path).unwrap();
    assert!(!raw_secrets.contains("key passphrase"));

    let second_key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let second_private_key = second_key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let second_inspected = inspect_private_key_text(&second_private_key).unwrap();
    let second_id = uuid::Uuid::new_v4().to_string();
    let second_backup = BackupSshKey {
        key: StoredSshKey {
            id: second_id,
            name: "id_ed25519_secondary".to_string(),
            note: None,
            algorithm: second_inspected.algorithm.clone(),
            fingerprint: second_inspected.fingerprint.clone(),
            encrypted: second_inspected.encrypted,
            imported_at: 456,
        },
        private_key: second_private_key.to_string(),
        passphrase: None,
    };
    let merge_summary = restore_backup_at(&paths, std::slice::from_ref(&second_backup)).unwrap();
    assert_eq!(merge_summary.imported, 1);
    assert_eq!(super::read_index(&paths).unwrap().keys.len(), 2);
    assert!(paths.key_path(&id).unwrap().exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_backup_key_metadata_that_does_not_match_its_private_key() {
    let key =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let private_key = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let backup = BackupSshKey {
        key: StoredSshKey {
            id: uuid::Uuid::new_v4().to_string(),
            name: "id_ed25519".to_string(),
            note: None,
            algorithm: "ssh-ed25519".to_string(),
            fingerprint: "SHA256:tampered".to_string(),
            encrypted: false,
            imported_at: 123,
        },
        private_key: private_key.to_string(),
        passphrase: None,
    };
    assert!(validate_backup_records(std::slice::from_ref(&backup)).is_err());
}

#[test]
fn cloud_merge_keeps_the_union_and_prefers_local_metadata() {
    let generated =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let private_key = generated
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let inspected = inspect_private_key_text(&private_key).unwrap();
    let key_id = uuid::Uuid::new_v4().to_string();
    let key = StoredSshKey {
        id: key_id.clone(),
        name: "id_ed25519".to_string(),
        note: Some("remote note".to_string()),
        algorithm: inspected.algorithm.clone(),
        fingerprint: inspected.fingerprint.clone(),
        encrypted: inspected.encrypted,
        imported_at: 123,
    };
    let remote_record = BackupSshKey {
        key: key.clone(),
        private_key: private_key.to_string(),
        passphrase: None,
    };
    let local_record = BackupSshKey {
        key: StoredSshKey {
            note: Some("local note".to_string()),
            ..key
        },
        private_key: private_key.to_string(),
        passphrase: Some("local passphrase".to_string()),
    };
    let other_generated =
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap();
    let other_private_key = other_generated
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .unwrap();
    let other_inspected = inspect_private_key_text(&other_private_key).unwrap();
    let other_record = BackupSshKey {
        key: StoredSshKey {
            id: uuid::Uuid::new_v4().to_string(),
            name: "id_ed25519_other".to_string(),
            note: None,
            algorithm: other_inspected.algorithm.clone(),
            fingerprint: other_inspected.fingerprint.clone(),
            encrypted: other_inspected.encrypted,
            imported_at: 456,
        },
        private_key: other_private_key.to_string(),
        passphrase: None,
    };
    let mut remote = vec![serde_json::to_value(&remote_record).unwrap()];
    let local = vec![
        serde_json::to_value(&local_record).unwrap(),
        serde_json::to_value(&other_record).unwrap(),
    ];

    let merged = super::merge_for_upload(Some(&remote), local).unwrap();

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0]["note"], "local note");
    assert_eq!(merged[0]["passphrase"], "local passphrase");
    let conflicting = BackupSshKey {
        key: StoredSshKey {
            id: key_id,
            ..other_record.key.clone()
        },
        private_key: other_record.private_key.clone(),
        passphrase: None,
    };
    assert!(super::merge_for_upload(
        Some(&remote),
        vec![serde_json::to_value(&conflicting).unwrap()]
    )
    .is_err());
    zeroize_values(&mut remote);
    let mut merged = merged;
    zeroize_values(&mut merged);
}

#[test]
fn failed_profile_commit_restores_key_metadata_passphrases_and_created_files() {
    for validation_failure in [true, false] {
        let root =
            std::env::temp_dir().join(format!("fileterm-key-transaction-{}", uuid::Uuid::new_v4()));
        let paths = KeyPaths {
            storage_root: root.clone(),
            keys_dir: root.join("ssh-keys"),
            index_path: root.join("ssh-keys.json"),
            secrets_path: root.join("ssh-key-secrets.json"),
        };
        fs::create_dir_all(&paths.keys_dir).unwrap();
        super::write_index(
            &paths,
            &StoredSshKeyIndex {
                version: 1,
                keys: Vec::new(),
            },
        )
        .unwrap();
        super::write_secrets(&paths, &StoredSshKeySecrets::default()).unwrap();
        let make_backup = || {
            let key = russh::keys::PrivateKey::random(
                &mut rand::rng(),
                russh::keys::ssh_key::Algorithm::Ed25519,
            )
            .unwrap();
            let private_key = key
                .to_openssh(russh::keys::ssh_key::LineEnding::LF)
                .unwrap();
            let inspected = inspect_private_key_text(&private_key).unwrap();
            BackupSshKey {
                key: StoredSshKey {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: "original".into(),
                    note: None,
                    algorithm: inspected.algorithm.clone(),
                    fingerprint: inspected.fingerprint.clone(),
                    encrypted: inspected.encrypted,
                    imported_at: 123,
                },
                private_key: private_key.to_string(),
                passphrase: Some("current saved passphrase".into()),
            }
        };
        let mut existing = make_backup();
        restore_backup_at(&paths, std::slice::from_ref(&existing)).unwrap();
        let previous_index = fs::read(&paths.index_path).unwrap();
        let previous_secrets = fs::read(&paths.secrets_path).unwrap();
        let previous_key = fs::read(paths.key_path(&existing.key.id).unwrap()).unwrap();
        existing.key.name = "older backup name".into();
        existing.passphrase = None;
        let incoming = [existing, make_backup()];
        let blocked_profile = root.join("profiles.json");
        fs::create_dir(&blocked_profile).unwrap();
        let result = super::restore_and_commit_at(&paths, &incoming, || {
            assert_eq!(super::read_index(&paths).unwrap().keys.len(), 2);
            assert!(!read_secrets(&paths)
                .unwrap()
                .passphrases
                .contains_key(&incoming[0].key.id));
            if validation_failure {
                crate::services::totp::validate_profile(
                    serde_json::json!({"otpSecret":"invalid!"})
                        .as_object()
                        .unwrap(),
                )
            } else {
                fs::write(&blocked_profile, b"profile data")
                    .map_err(|error| crate::AppError::Storage(error.to_string()))
            }
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&paths.index_path).unwrap(), previous_index);
        assert_eq!(fs::read(&paths.secrets_path).unwrap(), previous_secrets);
        assert_eq!(
            fs::read(paths.key_path(&incoming[0].key.id).unwrap()).unwrap(),
            previous_key
        );
        assert!(!paths.key_path(&incoming[1].key.id).unwrap().exists());
        assert_eq!(
            read_secrets(&paths).unwrap().passphrases[&incoming[0].key.id],
            "current saved passphrase"
        );
        let success = super::restore_and_commit_at(&paths, &incoming, || Ok(())).unwrap();
        assert_eq!(success.imported, 1);
        assert_eq!(success.updated, 1);
        assert!(paths.key_path(&incoming[1].key.id).unwrap().exists());
        assert_eq!(super::read_index(&paths).unwrap().keys.len(), 2);
        assert!(!read_secrets(&paths)
            .unwrap()
            .passphrases
            .contains_key(&incoming[0].key.id));
        fs::remove_dir_all(root).unwrap();
    }
}
