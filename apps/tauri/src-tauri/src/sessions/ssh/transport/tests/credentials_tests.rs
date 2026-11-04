use super::*;

#[test]
fn explicit_private_key_choice_replaces_only_transient_authentication() {
    let mut profile = serde_json::json!({"authType": "password", "username": "old", "password": "old-password", "privateKeyPath": "/old/key", "passphrase": "old-passphrase", "host": "server"});
    apply_ssh_credentials_response(&mut profile, &serde_json::json!({"authType": "privateKey", "username": " ops ", "privateKeyId": "key-1"})).unwrap();
    assert_eq!(profile["authType"], "privateKey");
    assert_eq!(profile["username"], "ops");
    assert_eq!(profile["privateKeyId"], "key-1");
    assert_eq!(profile["host"], "server");
    for field in ["password", "privateKeyPath", "passphrase"] {
        assert!(profile.get(field).is_none());
    }
}

#[test]
fn password_responses_remain_compatible_and_preserve_kubernetes_mode() {
    for method in ["password", "kubernetes"] {
        let mut profile = serde_json::json!({"authType": method, "useEmptyPassword": true});
        apply_ssh_credentials_response(
            &mut profile,
            &serde_json::json!({"username": "ops", "password": "  secret  "}),
        )
        .unwrap();
        assert_eq!(profile["authType"], method);
        assert_eq!(password_for_authentication(&profile), Some("  secret  "));
    }
}

#[test]
fn incomplete_or_unsupported_responses_do_not_mutate_the_profile() {
    for response in [
        serde_json::json!({"username": "", "password": "secret"}),
        serde_json::json!({"username": "ops", "password": ""}),
        serde_json::json!({"username": "ops", "authType": "privateKey", "privateKeyId": " "}),
        serde_json::json!({"username": "ops", "authType": "system"}),
        serde_json::json!({"username": "ops", "authType": "privateKey", "privateKeyPath": "/unmanaged/key"}),
    ] {
        let original = serde_json::json!({"authType": "password", "host": "server"});
        let mut profile = original.clone();
        assert!(apply_ssh_credentials_response(&mut profile, &response).is_err());
        assert_eq!(profile, original);
    }
    let mut kubernetes = serde_json::json!({"authType": "kubernetes"});
    assert!(apply_ssh_credentials_response(&mut kubernetes, &serde_json::json!({"username": "ops", "authType": "privateKey", "privateKeyId": "key-1"})).is_err());
}
