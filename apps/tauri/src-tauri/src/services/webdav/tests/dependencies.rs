use super::*;
use serde_json::json;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("fileterm-backup-deps-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("proxies.json"), serde_json::to_vec(&json!([
            {"id":"proxy-1","name":"proxy","type":"http","host":"proxy.test","port":8080,"username":"ops","createdAt":0,"updatedAt":0}
        ])).unwrap()).unwrap();
        fs::write(
            root.join("proxy-secrets.json"),
            br#"{"version":1,"passwords":{"proxy-1":"proxy-secret"}}"#,
        )
        .unwrap();
        fs::write(root.join("tunnels.json"), serde_json::to_vec(&json!([
            {"id":"tunnel-1","name":"tunnel","type":"ssh","forwards":[{"kind":"dynamic","bindHost":"127.0.0.1","bindPort":1080}],"scriptUrl":null,"timeoutSeconds":null,"createdAt":0,"updatedAt":0}
        ])).unwrap()).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn referenced_proxy_credentials_and_tunnel_rules_round_trip_inside_encryption() {
    let fixture = Fixture::new();
    let profiles = vec![json!({"proxyProfileId":"proxy-1","tunnelProfileId":"tunnel-1"})];
    let dependencies = export_at(&fixture.0, &profiles).unwrap();
    assert_eq!(dependencies.proxies[0]["password"], "proxy-secret");
    assert_eq!(dependencies.tunnels[0]["forwards"][0]["bindPort"], 1080);
    validate("proxy", &dependencies.proxies[0]).unwrap();
    validate("tunnel", &dependencies.tunnels[0]).unwrap();
    let (bytes, _) =
        backup_crypto::encrypt_bundle(profiles, vec![], dependencies, "Backup password 8", "now")
            .unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("proxy-secret"));
    let decoded = backup_crypto::decode_bundle(&bytes, Some("Backup password 8")).unwrap();
    assert_eq!(decoded.dependencies.proxies[0]["password"], "proxy-secret");
    assert_eq!(decoded.dependencies.tunnels.len(), 1);
    assert!(export_at(&fixture.0, &[json!({"proxyProfileId":"missing"})]).is_err());
}

#[test]
fn restore_never_reuses_a_colliding_id_with_different_settings_or_password() {
    let fixture = Fixture::new();
    let existing = vec![json!({"id":"proxy-1"})];
    let mut incoming = read_at(&fixture.0, "proxy", "proxy-1").unwrap().unwrap();
    assert_eq!(
        matching_id(&fixture.0, "proxy", &incoming, &existing)
            .unwrap()
            .as_deref(),
        Some("proxy-1")
    );
    incoming["password"] = json!("changed");
    assert!(matching_id(&fixture.0, "proxy", &incoming, &existing)
        .unwrap()
        .is_none());
    incoming["password"] = json!("proxy-secret");
    incoming["host"] = json!("different.test");
    assert!(matching_id(&fixture.0, "proxy", &incoming, &existing)
        .unwrap()
        .is_none());
}

#[test]
fn cloud_merge_retains_remote_only_dependencies_and_validates_inputs() {
    let mut remote = BackupDependencies {
        proxies: vec![
            json!({"id":"shared","password":"old"}),
            json!({"id":"remote-only"}),
        ],
        tunnels: Vec::new(),
    };
    let local = BackupDependencies {
        proxies: vec![json!({"id":"shared","password":"new"})],
        tunnels: Vec::new(),
    };
    merge(&mut remote, local);
    assert_eq!(remote.proxies.len(), 2);
    assert_eq!(remote.proxies[0]["password"], "new");
    assert!(validate(
        "proxy",
        &json!({"id":"x","name":"proxy","type":"http","host":"example.test","port":0})
    )
    .is_err());
    assert!(validate(
        "tunnel",
        &json!({"id":"x","name":"tunnel","type":"ssh","forwards":[{"kind":"invalid"}]})
    )
    .is_err());
}
