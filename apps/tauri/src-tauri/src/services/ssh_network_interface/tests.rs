use super::*;
use serde_json::json;

const ADAPTER: &str = "19bccd6a-65a8-4852-a11a-0f906af040a9";

fn selected() -> Value {
    json!({
        "id": "selected", "type": "ssh", "host": "192.0.2.1",
        "sshNetworkInterface": {"adapterId": ADAPTER, "displayName": "LAN"}
    })
}

#[test]
fn default_clear_supported_literals_and_address_rejections() {
    assert!(validate(&json!({"type": "ssh", "host": "example.test"}))
        .unwrap()
        .is_none());
    let mut profile = selected();
    assert!(validate(&profile).unwrap().is_some());
    profile["host"] = json!("2001:db8::1");
    assert!(validate(&profile).unwrap().is_some());
    for host in [
        "example.test",
        "fe80::1",
        "fe80::1%15",
        "::ffff:192.0.2.1",
        "127.0.0.1",
        "0.0.0.0",
        "ff02::1",
        "255.255.255.255",
    ] {
        profile["host"] = json!(host);
        assert!(validate(&profile).is_err(), "{host}");
    }
    profile["sshNetworkInterface"] = Value::Null;
    assert!(validate(&profile).unwrap().is_none());
}

#[test]
fn refuses_invalid_ids_and_incompatible_routes() {
    for (key, value) in [
        ("type", json!("sftp")),
        ("authType", json!("sshConfig")),
        ("proxyProfileId", json!("saved")),
        ("proxy", json!({"type": "http"})),
        ("jumpProfileId", json!("old")),
        ("jumpProfileIds", json!(["new"])),
        ("sshNetworkInterface", json!({"adapterId": "invalid"})),
        (
            "sshNetworkInterface",
            json!({"adapterId": ADAPTER, "interfaceIndex": 15}),
        ),
    ] {
        let mut profile = selected();
        profile[key] = value;
        assert!(validate(&profile).is_err(), "{key}");
    }
    let mut overridden = selected();
    overridden["jumpProfileId"] = json!("old");
    overridden["jumpProfileIds"] = json!([]);
    assert!(validate(&overridden).is_ok());
}

#[test]
fn save_rejects_bound_adapter_on_both_sides_of_jump_reference() {
    for dependent in [
        json!({"type":"ssh", "jumpProfileId":"selected"}),
        json!({"type":"ssh", "jumpProfileIds":["other","selected"]}),
    ] {
        let profile = selected();
        assert!(validate_save(&profile, std::slice::from_ref(&dependent)).is_err());
        assert!(validate_save(&dependent, &[profile]).is_err());
    }
}

#[tokio::test]
async fn unknown_adapter_does_not_fall_back() {
    let mut profile = selected();
    profile["sshNetworkInterface"]["adapterId"] = json!("ffffffff-ffff-ffff-ffff-ffffffffffff");
    let preference = validate(&profile).unwrap().unwrap();
    let error = connect(preference, "192.0.2.1", 22).await.unwrap_err();
    assert!(
        error.contains("missing") || error.contains("only supported on Windows"),
        "{error}"
    );
}
