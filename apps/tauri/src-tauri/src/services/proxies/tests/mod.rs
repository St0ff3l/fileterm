use super::*;

#[test]
fn validates_stored_proxy_profile_serialization() {
    let proxy = StoredProxyProfile {
        id: "test-id-1".to_string(),
        name: "My SOCKS5".to_string(),
        proxy_type: "socks5".to_string(),
        host: "127.0.0.1".to_string(),
        port: 1080,
        username: Some("user".to_string()),
        created_at: 1000,
        updated_at: 2000,
    };
    let json = serde_json::to_string(&proxy).unwrap();
    assert!(json.contains("\"type\":\"socks5\""));
    assert!(json.contains("\"host\":\"127.0.0.1\""));
    assert!(json.contains("\"port\":1080"));
    let decoded: StoredProxyProfile = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, proxy);
}

#[test]
fn validates_proxies_file_storage_and_secrets() {
    let directory =
        std::env::temp_dir().join(format!("fileterm-proxy-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).expect("fixture directory should be created");
    let paths = ProxyPaths::for_root(&directory);

    // Initially empty
    let list = read_proxies(&paths).unwrap();
    assert!(list.is_empty());

    // Write proxy
    let proxy = StoredProxyProfile {
        id: "p1".to_string(),
        name: "Dev Proxy".to_string(),
        proxy_type: "http".to_string(),
        host: "10.0.0.1".to_string(),
        port: 8080,
        username: Some("admin".to_string()),
        created_at: 12345,
        updated_at: 12345,
    };
    write_proxies(&paths, std::slice::from_ref(&proxy)).unwrap();

    let loaded = read_proxies(&paths).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], proxy);

    // Internal lookup
    let found = get_internal(&directory, "p1").unwrap();
    assert_eq!(found, Some(proxy));
    let not_found = get_internal(&directory, "nonexistent").unwrap();
    assert_eq!(not_found, None);

    // Write and read secrets
    let mut secrets = StoredProxySecrets::default();
    let encrypted =
        crate::services::secret_crypto::encrypt(&directory, "proxy/p1/password", "secret-pass-123")
            .unwrap();
    secrets.passwords.insert("p1".to_string(), encrypted);
    write_secrets(&paths, &secrets).unwrap();

    let secret_val = get_secret_internal(&directory, "p1").unwrap();
    assert_eq!(secret_val.as_deref(), Some("secret-pass-123"));

    fs::remove_dir_all(directory).expect("fixture directory should be removed");
}
