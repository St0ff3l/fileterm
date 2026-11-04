use super::*;

#[test]
fn validates_ssh_forward_rules() {
    let rules = vec![serde_json::json!({
        "id": "forward-1",
        "kind": "local",
        "bindHost": "127.0.0.1",
        "bindPort": 8080,
        "targetHost": "internal.example.com",
        "targetPort": 443,
        "autoStart": true
    })];
    validate_forwards(&rules).unwrap();
}

#[test]
fn rejects_invalid_http_tunnel_url() {
    assert!(validate_script_url("127.0.0.1:8080/tunnel").is_err());
    assert!(validate_script_url("https://example.com/tunnel.php").is_ok());
}

#[test]
fn serializes_tunnel_profile_without_token() {
    let tunnel = StoredTunnelProfile {
        id: "tunnel-1".to_string(),
        name: "HTTP Relay".to_string(),
        tunnel_type: "http".to_string(),
        forwards: Vec::new(),
        script_url: Some("https://example.com/tunnel.php".to_string()),
        timeout_seconds: Some(30),
        created_at: 1,
        updated_at: 2,
    };
    let json = serde_json::to_string(&tunnel).unwrap();
    assert!(!json.contains("token"));
    assert!(json.contains("scriptUrl"));
    assert_eq!(
        serde_json::from_str::<StoredTunnelProfile>(&json).unwrap(),
        tunnel
    );
}
