#[cfg(test)]
mod tests {
    use super::{
        build_export_bundle, download_payload, merge_synced_profile, normalize_remote_path,
        parse_bundle, parse_download_mode, parse_upload_mode, read_config_at,
        sanitize_import_profile, sha256_hex, upload_payload, validate_config, write_config_at,
        DownloadMode, StoredConfig, UploadMode,
    };
    use reqwest::Client;
    use serde_json::json;
    use std::fs;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Build a `Client` that ignores `HTTP_PROXY`/`HTTPS_PROXY` env vars.
    ///
    /// The real WebDAV `client()` uses reqwest's default behavior, which honors
    /// system proxy settings. On developer machines running Clash/V2Ray (typical
    /// `127.0.0.1:7897` setup), the loopback WebDAV fixture would be routed
    /// through the Go-based proxy, which rewrites header capitalization
    /// (`if-match` → `If-Match`) and breaks the assertion. The proxy is also
    /// irrelevant for the in-process fixture, so we disable it explicitly.
    fn test_client() -> Client {
        Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("test client must build")
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert!(count > 0, "client closed before completing HTTP headers");
            request.extend_from_slice(&byte[..count]);
        }

        let headers = String::from_utf8(request.clone()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or_default();
        if content_length > 0 {
            let mut body = vec![0_u8; content_length];
            socket.read_exact(&mut body).await.unwrap();
            request.extend_from_slice(&body);
        }

        String::from_utf8(request).unwrap()
    }

    #[test]
    fn rejects_traversal_remote_paths() {
        assert!(normalize_remote_path("../profiles.json").is_err());
        assert!(normalize_remote_path("profiles/../secret.json").is_err());
        assert_eq!(
            normalize_remote_path("sync/profiles.json").unwrap(),
            "sync/profiles.json"
        );
    }

    #[test]
    fn allows_connection_tests_without_enabling_webdav_sync() {
        let config = StoredConfig {
            enabled: false,
            url: "https://dav.example.test/remote.php/dav/files/fileterm".to_string(),
            remote_path: "fileterm/connections.json".to_string(),
            ..StoredConfig::default()
        };
        assert!(validate_config(&config, false).is_ok());
        assert!(validate_config(&config, true).is_err());
    }

    #[test]
    fn backup_sync_modes_have_directional_defaults_and_reject_unknown_values() {
        assert_eq!(parse_upload_mode(None).unwrap(), UploadMode::OverwriteCloud);
        assert_eq!(parse_download_mode(None).unwrap(), DownloadMode::MergeLocal);
        assert_eq!(
            parse_upload_mode(Some("merge-cloud")).unwrap(),
            UploadMode::MergeCloud
        );
        assert_eq!(
            parse_download_mode(Some("overwrite-local")).unwrap(),
            DownloadMode::OverwriteLocal
        );
        assert!(parse_upload_mode(Some("merge-local")).is_err());
        assert!(parse_download_mode(Some("overwrite-cloud")).is_err());
    }

    #[test]
    fn plaintext_webdav_password_is_migrated_to_encrypted_storage() {
        let directory =
            std::env::temp_dir().join(format!("fileterm-webdav-secret-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).expect("fixture directory should be created");
        let path = directory.join("webdav-sync.json");
        let legacy = StoredConfig {
            url: "https://dav.example.test".to_string(),
            password: Some("webdav-password".to_string()),
            ..StoredConfig::default()
        };
        fs::write(
            &path,
            serde_json::to_vec(&legacy).expect("legacy config json"),
        )
        .expect("legacy config write");

        let (config, migrated) = read_config_at(&path).expect("legacy config read");
        assert!(migrated);
        assert_eq!(config.password.as_deref(), Some("webdav-password"));
        write_config_at(&path, &config).expect("migrated config write");
        let raw = fs::read_to_string(&path).expect("migrated config read");
        assert!(!raw.contains("webdav-password"));

        let (decoded, migrated_again) = read_config_at(&path).expect("encrypted config read");
        assert!(!migrated_again);
        assert_eq!(decoded.password.as_deref(), Some("webdav-password"));
        fs::remove_dir_all(directory).expect("fixture directory should be removed");
    }

    #[test]
    fn verifies_profile_bundle_hash() {
        let profiles =
            json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
        let hash = sha256_hex(&serde_json::to_vec(&profiles).unwrap());
        let payload = json!({ "profiles": profiles, "contentHash": hash });
        assert_eq!(
            parse_bundle(&serde_json::to_vec(&payload).unwrap(), None)
                .unwrap()
                .profiles
                .len(),
            1
        );
    }

    #[test]
    fn keeps_connection_credentials_when_sanitizing_webdav_profiles() {
        let profile = sanitize_import_profile(&json!({
            "id": "remote-id",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "secret",
            "passphrase": "key-secret",
            "proxy": { "type": "http", "password": "proxy-secret" }
        }))
        .unwrap();
        assert_eq!(profile["password"], "secret");
        assert_eq!(profile["passphrase"], "key-secret");
        assert_eq!(profile["proxy"]["password"], "proxy-secret");
        assert!(profile.get("id").is_none());
    }

    #[test]
    fn webdav_upload_bundle_contains_saved_credentials() {
        let profiles = vec![json!({
            "id": "profile-1",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "secret",
            "passphrase": "key-secret",
            "proxy": { "type": "http", "password": "proxy-secret" }
        })];
        let (bytes, _) = build_export_bundle(&profiles, "Backup password 8").unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["schemaVersion"], 3);
        assert_eq!(payload["containsSecrets"], true);
        assert!(payload.get("profiles").is_none());
        assert!(!bytes
            .windows("proxy-secret".len())
            .any(|window| window == b"proxy-secret"));
    }

    #[test]
    fn remote_duplicate_updates_credentials_without_replacing_local_identity() {
        let existing = json!({
            "id": "local-id",
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "old",
            "order": 42
        });
        let incoming = json!({
            "name": "dev",
            "type": "ssh",
            "host": "example.test",
            "port": 22,
            "username": "ops",
            "password": "new"
        });
        let merged = merge_synced_profile(&existing, &incoming).unwrap();
        assert_eq!(merged["id"], "local-id");
        assert_eq!(merged["order"], 42);
        assert_eq!(merged["password"], "new");
    }

    #[tokio::test]
    async fn real_webdav_server_rejects_stale_etag_with_if_match() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut head, _) = listener.accept().await.unwrap();
            let head_request = read_request(&mut head).await;
            assert!(head_request.starts_with("HEAD /profiles.json HTTP/1.1\r\n"));
            head.write_all(
                b"HTTP/1.1 200 OK\r\nETag: \"etag-before-write\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();

            let (mut put, _) = listener.accept().await.unwrap();
            let put_request = read_request(&mut put).await;
            assert!(put_request.starts_with("PUT /profiles.json HTTP/1.1\r\n"));
            assert!(put_request.contains("if-match: \"etag-before-write\"\r\n"));
            put.write_all(
                b"HTTP/1.1 412 Precondition Failed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: Some("\"etag-before-write\"".to_string()),
            content_hash: None,
        };
        let result = upload_payload(&test_client(), &config, b"{}".to_vec()).await;
        server.await.unwrap();
        let error = result.unwrap_err();
        assert!(error.to_string().contains("ETag 冲突"), "{error}");
    }

    #[tokio::test]
    async fn real_webdav_server_uploads_payload_and_returns_fresh_etag() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let payload = br#"{"profiles":[]}"#.to_vec();
        let expected_payload = payload.clone();
        let server = tokio::spawn(async move {
            let (mut head, _) = listener.accept().await.unwrap();
            let head_request = read_request(&mut head).await;
            assert!(head_request.starts_with("HEAD /profiles.json HTTP/1.1\r\n"));
            head.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();

            let (mut put, _) = listener.accept().await.unwrap();
            let put_request = read_request(&mut put).await;
            assert!(put_request.starts_with("PUT /profiles.json HTTP/1.1\r\n"));
            assert!(put_request.contains("if-none-match: *\r\n"));
            assert!(
                put_request.contains(&format!("content-length: {}\r\n", expected_payload.len()))
            );
            let body = put_request.split_once("\r\n\r\n").unwrap().1.as_bytes();
            assert_eq!(body, expected_payload);
            put.write_all(
                b"HTTP/1.1 201 Created\r\nETag: \"etag-after-write\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        };
        assert_eq!(
            upload_payload(&test_client(), &config, payload)
                .await
                .unwrap(),
            Some("\"etag-after-write\"".to_string())
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn real_webdav_server_downloads_payload_and_hash_is_verified() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let profiles =
            json!([{ "name": "dev", "type": "ssh", "host": "example.test", "port": 22 }]);
        let profile_bytes = serde_json::to_vec(&profiles).unwrap();
        let payload = serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "contentHash": sha256_hex(&profile_bytes),
            "profiles": profiles,
        }))
        .unwrap();
        let server_payload = payload.clone();
        let server = tokio::spawn(async move {
            let (mut get, _) = listener.accept().await.unwrap();
            let get_request = read_request(&mut get).await;
            assert!(get_request.starts_with("GET /profiles.json HTTP/1.1\r\n"));
            get.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nETag: \"etag-download\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    server_payload.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
            get.write_all(&server_payload).await.unwrap();
        });

        let config = StoredConfig {
            enabled: true,
            url: format!("http://{address}"),
            username: None,
            remote_path: "profiles.json".to_string(),
            allow_insecure_tls: Some(true),
            password: None,
            last_synced_at: None,
            last_etag: None,
            content_hash: None,
        };
        let (downloaded, etag) = download_payload(&test_client(), &config).await.unwrap();
        assert_eq!(downloaded, payload);
        assert_eq!(etag.as_deref(), Some("\"etag-download\""));
        assert_eq!(parse_bundle(&downloaded, None).unwrap().profiles.len(), 1);
        server.await.unwrap();
    }
}
