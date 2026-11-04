#[cfg(test)]
mod tests {
    use super::{
        commit_pending_files, ensure_portable_marker_for_executable, migrate_legacy_store,
        migrate_portable_data_from_source, portable_config_directory_for_executable,
        replace_file_atomically, select_legacy_directory, PendingFile, LEGACY_MIGRATION_MARKER,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_dirs(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("fileterm-storage-{name}-{}", uuid::Uuid::new_v4()));
        let current = root.join("com.fileterm.desktop");
        let legacy = root.join("FileTerm");
        fs::create_dir_all(&current).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        (root, current, legacy)
    }

    #[test]
    fn portable_executable_uses_a_config_directory_next_to_the_binary() {
        let root =
            std::env::temp_dir().join(format!("fileterm-portable-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();

        let portable_executable = root.join("FileTerm-2.2.4-windows-x64-portable.exe");
        assert_eq!(
            portable_config_directory_for_executable(&portable_executable),
            Some(root.join("config"))
        );

        let regular_executable = root.join("FileTerm.exe");
        assert_eq!(
            portable_config_directory_for_executable(&regular_executable),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_marker_enables_local_config_for_a_renamed_binary() {
        let root =
            std::env::temp_dir().join(format!("fileterm-portable-marker-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("portable"), b"").unwrap();

        assert_eq!(
            portable_config_directory_for_executable(&root.join("FileTerm.exe")),
            Some(root.join("config"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_mode_persists_a_marker_for_future_launches() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-portable-marker-persist-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let executable = root.join("FileTerm-2.2.5-windows-x64-portable.exe");

        let marker = ensure_portable_marker_for_executable(&executable)
            .unwrap()
            .expect("portable executable should get a marker");
        assert!(marker.is_file());
        assert_eq!(fs::read(marker).unwrap(), Vec::<u8>::new());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_data_migration_copies_owned_files_once() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-portable-data-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let source = root.join("app-data");
        let current = root.join("portable").join("config");
        fs::create_dir_all(source.join("ai-conversations")).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(source.join("profiles.json"), b"profiles").unwrap();
        fs::write(source.join("secret-store-v1.key"), b"seed").unwrap();
        fs::write(
            source.join("ai-conversations").join("conversation.json"),
            b"conversation",
        )
        .unwrap();
        fs::write(source.join("mcp-runtime.json"), b"stale runtime").unwrap();
        fs::create_dir_all(source.join("logs")).unwrap();
        fs::write(source.join("logs").join("app.log"), b"diagnostic log").unwrap();

        migrate_portable_data_from_source(&current, &source).unwrap();

        assert_eq!(
            fs::read(current.join("profiles.json")).unwrap(),
            b"profiles"
        );
        assert_eq!(
            fs::read(current.join("secret-store-v1.key")).unwrap(),
            b"seed"
        );
        assert_eq!(
            fs::read(current.join("ai-conversations").join("conversation.json")).unwrap(),
            b"conversation"
        );
        assert!(!current.join("mcp-runtime.json").exists());
        assert!(!current.join("logs").exists());
        assert!(current.join("portable-migration.json").exists());

        fs::write(source.join("profiles.json"), b"changed source").unwrap();
        migrate_portable_data_from_source(&current, &source).unwrap();
        assert_eq!(
            fs::read(current.join("profiles.json")).unwrap(),
            b"profiles"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_directory_uses_config_root_when_data_and_config_roots_differ() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-legacy-root-selection-{}",
            uuid::Uuid::new_v4()
        ));
        let current_data = root.join("data").join("com.fileterm.desktop");
        let current_config = root.join("config").join("com.fileterm.desktop");
        let empty_data_candidate = root.join("data").join("FileTerm");
        let electron = root.join("config").join("FileTerm");
        fs::create_dir_all(&current_data).unwrap();
        fs::create_dir_all(&current_config).unwrap();
        fs::create_dir_all(&empty_data_candidate).unwrap();
        fs::create_dir_all(&electron).unwrap();
        fs::write(electron.join("profiles.json"), b"[]").unwrap();

        assert_eq!(
            select_legacy_directory(&current_data, Some(&current_config), None).unwrap(),
            electron
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_directory_uses_app_data_parent_for_portable_storage() {
        let root = std::env::temp_dir().join(format!(
            "fileterm-portable-legacy-root-selection-{}",
            uuid::Uuid::new_v4()
        ));
        let portable_config = root.join("portable").join("config");
        let app_data = root.join("data").join("com.fileterm.desktop");
        let electron = root.join("data").join("FileTerm");
        fs::create_dir_all(&portable_config).unwrap();
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&electron).unwrap();
        fs::write(electron.join("profiles.json"), b"[]").unwrap();

        assert_eq!(
            select_legacy_directory(&portable_config, None, Some(&app_data)).unwrap(),
            electron
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_replace_updates_an_existing_file_without_losing_the_old_on_staging() {
        let root =
            std::env::temp_dir().join(format!("fileterm-atomic-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let target = root.join("state.json");
        let staged = root.join("state.json.tmp");
        fs::write(&target, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();

        replace_file_atomically(&staged, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn write_json(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }

    #[test]
    fn migration_is_one_time_and_deleted_records_do_not_return() {
        let (root, current, legacy) = test_dirs("one-time");
        write_json(
            &legacy.join("profiles.json"),
            &json!([{ "id": "legacy-profile", "name": "Legacy" }]),
        );

        let first = migrate_legacy_store(&current, &legacy).unwrap();
        assert_eq!(first.status, "completed");
        assert!(current.join(LEGACY_MIGRATION_MARKER).is_file());
        write_json(&current.join("profiles.json"), &json!([]));

        let second = migrate_legacy_store(&current, &legacy).unwrap();
        assert_eq!(first.completed_at, second.completed_at);
        let profiles: Value =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles, json!([]));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_keeps_current_records_and_adds_only_missing_ids() {
        let (root, current, legacy) = test_dirs("conflicts");
        write_json(
            &current.join("profiles.json"),
            &json!([{ "id": "same", "name": "Current" }]),
        );
        write_json(
            &legacy.join("profiles.json"),
            &json!([
                { "id": "same", "name": "Legacy" },
                { "id": "missing", "name": "Imported" }
            ]),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let profiles: Vec<Value> =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0]["name"], "Current");
        assert_eq!(profiles[1]["id"], "missing");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_legacy_json_rolls_back_without_writing_a_marker() {
        let (root, current, legacy) = test_dirs("rollback");
        write_json(
            &current.join("profiles.json"),
            &json!([{ "id": "current", "name": "Keep" }]),
        );
        fs::write(legacy.join("profiles.json"), b"not-json").unwrap();

        assert!(migrate_legacy_store(&current, &legacy).is_err());
        assert!(!current.join(LEGACY_MIGRATION_MARKER).exists());
        let profiles: Value =
            serde_json::from_slice(&fs::read(current.join("profiles.json")).unwrap()).unwrap();
        assert_eq!(profiles[0]["id"], "current");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn commit_failure_restores_files_already_replaced_in_the_transaction() {
        let (root, current, _) = test_dirs("commit-rollback");
        let transaction = current.join("transaction");
        let staged = transaction.join("staged");
        let backup = transaction.join("backup");
        fs::create_dir_all(&staged).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(current.join("first.json"), b"first-old").unwrap();
        fs::write(current.join("second.json"), b"second-old").unwrap();
        fs::write(staged.join("first.json"), b"first-new").unwrap();

        let pending = vec![
            PendingFile {
                target: current.join("first.json"),
                staged: staged.join("first.json"),
                backup: backup.join("first.json"),
                confidential: false,
            },
            PendingFile {
                target: current.join("second.json"),
                staged: staged.join("missing-second.json"),
                backup: backup.join("second.json"),
                confidential: false,
            },
        ];

        assert!(commit_pending_files(&pending).is_err());
        assert_eq!(fs::read(current.join("first.json")).unwrap(), b"first-old");
        assert_eq!(
            fs::read(current.join("second.json")).unwrap(),
            b"second-old"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_copies_only_indexed_ssh_keys_once() {
        let (root, current, legacy) = test_dirs("ssh-keys");
        let key_id = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(legacy.join("ssh-keys")).unwrap();
        write_json(
            &legacy.join("ssh-keys.json"),
            &json!({
                "version": 1,
                "keys": [{
                    "id": key_id,
                    "name": "id_ed25519",
                    "algorithm": "ssh-ed25519",
                    "fingerprint": "SHA256:test",
                    "encrypted": false,
                    "importedAt": 1
                }]
            }),
        );
        fs::write(
            legacy.join("ssh-keys").join(format!("{key_id}.key")),
            b"PRIVATE KEY",
        )
        .unwrap();

        migrate_legacy_store(&current, &legacy).unwrap();
        let target_key = current.join("ssh-keys").join(format!("{key_id}.key"));
        assert_eq!(fs::read(&target_key).unwrap(), b"PRIVATE KEY");

        write_json(
            &current.join("ssh-keys.json"),
            &json!({ "version": 1, "keys": [] }),
        );
        fs::remove_file(&target_key).unwrap();
        migrate_legacy_store(&current, &legacy).unwrap();
        assert!(!target_key.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_prunes_profile_secrets_without_a_matching_profile() {
        let (root, current, legacy) = test_dirs("orphan-profile-secret");
        write_json(
            &legacy.join("profile-secrets.json"),
            &json!({
                "version": 1,
                "profiles": {
                    "deleted-profile": {
                        "password": { "storage": "plain-text-fallback", "value": "secret" }
                    }
                }
            }),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let secrets: Value =
            serde_json::from_slice(&fs::read(current.join("profile-secrets.json")).unwrap())
                .unwrap();
        assert_eq!(secrets["profiles"], json!({}));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn migrated_plaintext_secrets_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (root, current, legacy) = test_dirs("secret-mode");
        write_json(
            &legacy.join("profiles.json"),
            &json!([{ "id": "profile-1", "name": "Secret" }]),
        );
        write_json(
            &legacy.join("profile-secrets.json"),
            &json!({
                "version": 1,
                "profiles": {
                    "profile-1": {
                        "password": { "storage": "plain-text-fallback", "value": "secret" }
                    }
                }
            }),
        );

        migrate_legacy_store(&current, &legacy).unwrap();
        let mode = fs::metadata(current.join("profile-secrets.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(root).unwrap();
    }
}
