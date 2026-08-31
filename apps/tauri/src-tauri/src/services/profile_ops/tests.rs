#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn folder(id: &str, name: &str) -> Value {
        json!({ "id": id, "name": name, "type": "folder" })
    }

    fn profile(id: &str, group: &str, parent_id: Option<&str>) -> Value {
        let mut obj = Map::new();
        obj.insert("id".to_string(), Value::String(id.to_string()));
        obj.insert("name".to_string(), Value::String(format!("Profile {}", id)));
        obj.insert("type".to_string(), Value::String("ssh".to_string()));
        obj.insert("group".to_string(), Value::String(group.to_string()));
        obj.insert(
            "parentId".to_string(),
            parent_id
                .map(|s| Value::String(s.to_string()))
                .unwrap_or(Value::Null),
        );
        Value::Object(obj)
    }

    #[test]
    fn heals_when_group_points_to_valid_folder_but_parent_id_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "Alpha", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("parentId").and_then(|v| v.as_str()),
            Some("f1")
        );
    }

    #[test]
    fn heals_when_parent_id_points_to_valid_folder_but_group_wrong() {
        let folders = vec![folder("f1", "Alpha"), folder("f2", "Beta")];
        let mut profiles = vec![profile("p1", "默认", Some("f2"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("group").and_then(|v| v.as_str()),
            Some("Beta")
        );
    }

    #[test]
    fn heals_when_group_points_to_missing_folder() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "Ghost", Some("ghost-id"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(dirty);
        assert_eq!(
            profiles[0].get("group").and_then(|v| v.as_str()),
            Some("默认")
        );
        assert!(profiles[0].get("parentId").unwrap().is_null());
    }

    #[test]
    fn no_change_when_consistent() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "Alpha", Some("f1"))];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(!dirty);
    }

    #[test]
    fn default_group_with_null_parent_id_untouched() {
        let folders = vec![folder("f1", "Alpha")];
        let mut profiles = vec![profile("p1", "默认", None)];
        let dirty = heal_profiles(&mut profiles, &folders);
        assert!(!dirty);
    }

    #[test]
    fn heals_legacy_folder_and_command_entity_shapes() {
        let mut connection_folders = vec![json!({ "id": "f1", "name": "Legacy" })];
        let mut command_folders = vec![json!({ "id": "cf1", "name": "Legacy commands" })];
        let mut commands = vec![json!({ "id": "c1", "name": "Legacy command" })];

        assert!(heal_connection_folders(&mut connection_folders));
        assert!(heal_command_folders(&mut command_folders));
        assert!(heal_command_templates(&mut commands));

        assert_eq!(connection_folders[0]["type"], "folder");
        assert!(connection_folders[0]["order"].is_number());
        assert_eq!(command_folders[0]["type"], "command-folder");
        assert!(command_folders[0]["order"].is_number());
        assert_eq!(commands[0]["type"], "command-template");
        assert!(commands[0]["order"].is_number());
        assert_eq!(commands[0]["command"], "");
        assert_eq!(commands[0]["appendCarriageReturn"], true);

        assert!(!heal_connection_folders(&mut connection_folders));
        assert!(!heal_command_folders(&mut command_folders));
        assert!(!heal_command_templates(&mut commands));
    }

    #[test]
    fn rebuilding_secrets_prunes_deleted_profile_ids_and_encrypts_values() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-profile-secrets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile-secrets.json");
        let profiles = vec![serde_json::json!({
            "id": "profile-current",
            "password": "plain-text-password",
            "sudoPassword": "plain-text-sudo-password",
            "suPassword": "plain-text-su-password",
            "proxy": { "password": "plain-text-proxy-password" }
        })];
        let secrets = build_profile_secrets(&path, &profiles, None).unwrap();
        let stored = secrets["profiles"].as_object().unwrap();

        assert_eq!(stored.len(), 1);
        assert!(stored.contains_key("profile-current"));
        assert!(!stored.contains_key("profile-deleted"));
        assert_eq!(
            stored["profile-current"]["password"]["storage"].as_str(),
            Some(crate::services::secret_crypto::ENCRYPTED_STORAGE)
        );
        assert_ne!(
            stored["profile-current"]["password"]["value"].as_str(),
            Some("plain-text-password")
        );
        assert_eq!(
            stored["profile-current"]["sudoPassword"]["storage"].as_str(),
            Some(crate::services::secret_crypto::ENCRYPTED_STORAGE)
        );
        assert_eq!(
            stored["profile-current"]["suPassword"]["storage"].as_str(),
            Some(crate::services::secret_crypto::ENCRYPTED_STORAGE)
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn hydrates_encrypted_profile_secrets_only_into_internal_profile_values() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-profile-hydrate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile-secrets.json");
        let source = vec![json!({
            "id": "profile-current",
            "password": "login-password",
            "sudoPassword": "sudo-password",
            "suPassword": "su-password",
            "proxy": { "password": "proxy-password" }
        })];
        let stored = build_profile_secrets(&path, &source, None).unwrap();
        write_secure_secret_file(
            &path,
            &serde_json::to_vec_pretty(&stored).expect("secret store should serialize"),
        )
        .unwrap();

        let mut public_profiles = vec![json!({
            "id": "profile-current",
            "name": "Server",
            "proxy": { "type": "http" }
        })];
        hydrate_profile_secrets(&path, &mut public_profiles).unwrap();

        assert_eq!(public_profiles[0]["password"], "login-password");
        assert_eq!(public_profiles[0]["sudoPassword"], "sudo-password");
        assert_eq!(public_profiles[0]["suPassword"], "su-password");
        assert_eq!(public_profiles[0]["proxy"]["password"], "proxy-password");
        let public = strip_secret_fields_public(&public_profiles[0]);
        assert!(!public.as_object().unwrap().contains_key("password"));
        assert!(!public.as_object().unwrap().contains_key("sudoPassword"));
        assert_eq!(public["hasSavedSudoPassword"], true);
        assert_eq!(public["hasSavedSuPassword"], true);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn redacted_edit_placeholders_preserve_stored_profile_secrets() {
        let previous = json!({
            "id": "profile-1",
            "password": "stored-password",
            "passphrase": "stored-passphrase",
            "privateKeyPath": "/keys/id_ed25519",
            "sudoPassword": "stored-sudo-password",
            "suPassword": "stored-su-password",
            "proxy": {
                "type": "http",
                "host": "proxy.example.com",
                "port": 8080,
                "password": "stored-proxy-password"
            }
        });
        let mut edit = json!({
            "id": "profile-1",
            "password": "",
            "passphrase": "",
            "privateKeyPath": "",
            "sudoPassword": "",
            "suPassword": "",
            "proxyPassword": "",
            "proxy": {
                "type": "http",
                "host": "proxy.example.com",
                "port": 8080
            }
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(normalize_profile_secret_input(&mut edit, Some(&previous)));
        assert_eq!(edit["password"], "stored-password");
        assert_eq!(edit["passphrase"], "stored-passphrase");
        assert_eq!(edit["privateKeyPath"], "/keys/id_ed25519");
        assert_eq!(edit["sudoPassword"], "stored-sudo-password");
        assert_eq!(edit["suPassword"], "stored-su-password");
        assert_eq!(edit["proxy"]["password"], "stored-proxy-password");
        assert!(!edit.contains_key("proxyPassword"));

        let public = strip_secret_fields_public(&Value::Object(edit));
        assert!(public.get("password").is_none());
        assert!(public.get("passphrase").is_none());
        assert!(public.get("privateKeyPath").is_none());
        assert!(public.get("proxyPassword").is_none());
        assert!(public["proxy"].get("password").is_none());
        assert_eq!(public["hasSavedPassword"], true);
        assert_eq!(public["hasSavedSudoPassword"], true);
        assert_eq!(public["hasSavedSuPassword"], true);
    }

    #[test]
    fn empty_password_mode_discards_any_saved_password() {
        let previous = json!({
            "id": "profile-1",
            "password": "stored-password"
        });
        let mut edit = json!({
            "id": "profile-1",
            "password": "",
            "useEmptyPassword": true
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(normalize_profile_secret_input(&mut edit, Some(&previous)));
        assert!(!edit.contains_key("password"));
        assert_eq!(edit["useEmptyPassword"], true);
        assert_eq!(
            strip_secret_fields_public(&Value::Object(edit))["hasSavedPassword"],
            false
        );
    }

    #[test]
    fn empty_trusted_host_fingerprint_preserves_saved_value() {
        let previous = json!({
            "id": "profile-1",
            "trustedHostFingerprint": "SHA256:saved-fingerprint"
        });
        let mut edit = json!({
            "trustedHostFingerprint": ""
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(preserve_trusted_host_fingerprint(
            &mut edit,
            Some(&previous)
        ));
        assert_eq!(edit["trustedHostFingerprint"], "SHA256:saved-fingerprint");
    }

    #[test]
    fn null_trusted_host_fingerprint_remains_an_explicit_clear() {
        let previous = json!({
            "id": "profile-1",
            "trustedHostFingerprint": "SHA256:saved-fingerprint"
        });
        let mut clear = json!({
            "trustedHostFingerprint": null
        })
        .as_object()
        .unwrap()
        .clone();

        assert!(!preserve_trusted_host_fingerprint(
            &mut clear,
            Some(&previous)
        ));
        assert!(clear["trustedHostFingerprint"].is_null());
    }

    #[test]
    fn proxy_form_password_is_normalized_and_can_be_explicitly_cleared() {
        let mut create = json!({
            "proxyPassword": "new-proxy-password",
            "proxy": { "type": "socks5", "host": "proxy.example.com", "port": 1080 }
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(normalize_profile_secret_input(&mut create, None));
        assert_eq!(create["proxy"]["password"], "new-proxy-password");
        assert!(!create.contains_key("proxyPassword"));

        let previous = Value::Object(create.clone());
        let mut clear = json!({
            "proxyPassword": null,
            "proxy": { "type": "socks5", "host": "proxy.example.com", "port": 1080 }
        })
        .as_object()
        .unwrap()
        .clone();
        assert!(normalize_profile_secret_input(&mut clear, Some(&previous)));
        assert!(clear["proxy"].get("password").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn plaintext_secret_file_is_written_with_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory =
            std::env::temp_dir().join(format!("fileterm-profile-secrets-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("profile-secrets.json");
        let profiles = vec![serde_json::json!({
            "id": "profile-1",
            "password": "plain-text-password"
        })];

        persist_profile_secrets_at(&path, &profiles).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        lock_down_secret_file(&path).unwrap();
        let healed_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(healed_mode, 0o600);

        std::fs::remove_dir_all(directory).unwrap();
    }
}
