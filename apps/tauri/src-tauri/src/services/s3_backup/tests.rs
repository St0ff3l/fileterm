#[cfg(test)]
mod tests {
    use super::{
        apply_provider_defaults, normalize_bucket, normalize_provider, normalize_remote_path,
        percent_encode, read_config_at, signature, target, validate_config, write_config_at,
        StoredConfig, BITIFUL_S4_ENDPOINT, BITIFUL_S4_REGION, DEFAULT_REMOTE_PATH,
        PROVIDER_BITIFUL_S4, PROVIDER_CLOUDFLARE_R2,
    };
    use std::fs;

    #[test]
    fn uses_cloudflare_r2_defaults_from_its_endpoint() {
        assert_eq!(
            normalize_provider(None, "https://account.r2.cloudflarestorage.com"),
            PROVIDER_CLOUDFLARE_R2
        );
    }

    #[test]
    fn recognizes_the_bitiful_s4_endpoint() {
        assert_eq!(
            normalize_provider(None, BITIFUL_S4_ENDPOINT),
            PROVIDER_BITIFUL_S4
        );
        let mut config = StoredConfig {
            provider: PROVIDER_BITIFUL_S4.to_string(),
            endpoint: "https://not-bitiful.example".to_string(),
            region: "us-east-1".to_string(),
            bucket: "fileterm-backups".to_string(),
            remote_path: "sync/profiles.json".to_string(),
            path_style_access_enabled: true,
            ..StoredConfig::default()
        };
        apply_provider_defaults(&mut config);
        assert_eq!(config.endpoint, BITIFUL_S4_ENDPOINT);
        assert_eq!(config.region, BITIFUL_S4_REGION);
        assert!(!config.path_style_access_enabled);
        let target = target(&config, Some(&config.remote_path)).unwrap();
        assert_eq!(
            target.url.as_str(),
            "https://fileterm-backups.s3.bitiful.net/sync/profiles.json"
        );
    }

    #[test]
    fn validates_bucket_and_object_key() {
        assert_eq!(StoredConfig::default().remote_path, DEFAULT_REMOTE_PATH);
        assert!(normalize_bucket("FileTerm").is_err());
        assert!(normalize_bucket("a").is_err());
        assert_eq!(
            normalize_bucket("fileterm-backups").unwrap(),
            "fileterm-backups"
        );
        assert!(normalize_remote_path("../profiles.json").is_err());
        assert!(normalize_remote_path("sync/../profiles.json").is_err());
        assert_eq!(
            normalize_remote_path("sync/profiles.json").unwrap(),
            "sync/profiles.json"
        );
    }

    #[test]
    fn allows_connection_tests_without_enabling_s3_backup() {
        let config = StoredConfig {
            enabled: false,
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            region: "auto".to_string(),
            bucket: "fileterm-backups".to_string(),
            access_key_id: Some("access-key".to_string()),
            secret_access_key: Some("secret-key".to_string()),
            ..StoredConfig::default()
        };
        assert!(validate_config(&config, false).is_ok());
        assert!(validate_config(&config, true).is_err());
    }

    #[test]
    fn plaintext_s3_credentials_are_migrated_to_encrypted_storage() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-s3-secret-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        let path = directory.join("s3-backup.json");
        let legacy = StoredConfig {
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            access_key_id: Some("s3-access-key".to_string()),
            secret_access_key: Some("s3-secret-key".to_string()),
            ..StoredConfig::default()
        };
        fs::write(
            &path,
            serde_json::to_vec(&legacy).expect("legacy config json"),
        )
        .expect("legacy config write");

        let (config, migrated) = read_config_at(&path).expect("legacy config read");
        assert!(migrated);
        assert_eq!(config.access_key_id.as_deref(), Some("s3-access-key"));
        assert_eq!(config.secret_access_key.as_deref(), Some("s3-secret-key"));
        write_config_at(&path, &config).expect("migrated config write");
        let raw = fs::read_to_string(&path).expect("migrated config read");
        assert!(!raw.contains("s3-access-key"));
        assert!(!raw.contains("s3-secret-key"));

        let (decoded, migrated_again) = read_config_at(&path).expect("encrypted config read");
        assert!(!migrated_again);
        assert_eq!(decoded.access_key_id.as_deref(), Some("s3-access-key"));
        assert_eq!(decoded.secret_access_key.as_deref(), Some("s3-secret-key"));
        fs::remove_dir_all(directory).expect("fixture directory should be removed");
    }

    #[test]
    fn canonical_uri_keeps_key_path_separators() {
        assert_eq!(
            percent_encode("backup folder/profiles.json", true),
            "backup%20folder/profiles.json"
        );
        assert_eq!(
            percent_encode("backup folder/profiles.json", false),
            "backup%20folder%2Fprofiles.json"
        );
    }

    #[test]
    fn bucket_preflight_targets_the_bucket_not_the_backup_object() {
        let config = StoredConfig {
            endpoint: "https://account.r2.cloudflarestorage.com".to_string(),
            bucket: "fileterm-backups".to_string(),
            remote_path: "sync/profiles.json".to_string(),
            ..StoredConfig::default()
        };
        let bucket = target(&config, None).unwrap();
        let object = target(&config, Some(&config.remote_path)).unwrap();
        assert_eq!(bucket.canonical_uri, "/fileterm-backups");
        assert_eq!(object.canonical_uri, "/fileterm-backups/sync/profiles.json");
    }

    #[test]
    fn matches_the_aws_sigv4_get_object_reference_signature() {
        // AWS's documented GET Object example, using the fixed credentials
        // and canonical request timestamp from the SigV4 guide.
        let string_to_sign = concat!(
            "AWS4-HMAC-SHA256\n",
            "20130524T000000Z\n",
            "20130524/us-east-1/s3/aws4_request\n",
            "7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972"
        );
        assert_eq!(
            signature(
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
                "20130524",
                "us-east-1",
                string_to_sign
            ),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }
}
