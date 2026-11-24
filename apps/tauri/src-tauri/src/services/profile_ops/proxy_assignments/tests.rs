use super::*;
use crate::services::proxies::StoredProxyProfile;
use serde_json::json;

fn shared_proxy() -> StoredProxyProfile {
    serde_json::from_value(json!({
        "id": "shared", "name": "Shared", "type": "socks5", "host": "127.0.0.1", "port": 1180,
        "username": "proxy-user"
    }))
    .unwrap()
}

fn assign(profile_id: &str, expected: ProxySource) -> ProxyAssignment {
    ProxyAssignment {
        profile_id: profile_id.into(),
        assigned: true,
        expected,
    }
}

#[test]
fn assigns_by_id_and_detaches_only_from_the_current_proxy() {
    let custom = json!({
        "id":"a", "name":"Same name", "type":"ssh", "group":"G", "order":3, "backupIdentity":"keep",
        "password":"ssh-password", "sudoPassword":"sudo-password", "trustedHostFingerprint":"keep",
        "proxy":{"type":"http","host":"old","port":1080,"password":"old-proxy-password"}
    });
    let unlink = json!({"id":"b","name":"Same name","type":"ssh","proxyProfileId":"shared",
        "proxy":{"type":"socks5","password":"shared-password"}});
    let other = json!({"id":"c","name":"Same name","type":"ssh","proxyProfileId":"other"});
    let bound_elsewhere =
        json!({"id":"d","name":"Same name","type":"ssh","proxyProfileId":"shared"});
    let mut profiles = vec![
        custom.clone(),
        unlink,
        other.clone(),
        bound_elsewhere.clone(),
    ];

    assert!(apply_assignments(
        &mut profiles,
        &shared_proxy(),
        &[
            assign("a", ProxySource::from_profile(&custom)),
            ProxyAssignment {
                profile_id: "b".into(),
                assigned: false,
                expected: ProxySource::Saved {
                    proxy_id: "shared".into()
                }
            },
            ProxyAssignment {
                profile_id: "c".into(),
                assigned: false,
                expected: ProxySource::Saved {
                    proxy_id: "other".into()
                }
            },
        ]
    )
    .unwrap());
    assert_eq!(
        profiles[0],
        json!({
            "id":"a", "name":"Same name", "type":"ssh", "group":"G", "order":3, "backupIdentity":"keep",
            "password":"ssh-password", "sudoPassword":"sudo-password", "trustedHostFingerprint":"keep",
            "proxyProfileId":"shared", "proxy":{"type":"socks5","host":"127.0.0.1","port":1180,"username":"proxy-user"}
        })
    );
    assert_eq!(
        profiles[1],
        json!({"id":"b","name":"Same name","type":"ssh"})
    );
    assert_eq!(profiles[2], other);
    assert_eq!(profiles[3], bound_elsewhere);
    assert!(apply_assignments(
        &mut profiles,
        &shared_proxy(),
        &[assign("a", ProxySource::Direct)]
    )
    .is_err());
}

#[test]
fn rejects_stale_duplicate_and_ineligible_changes_before_mutating_any_rows() {
    let before = vec![
        json!({"id":"a","type":"ssh"}),
        json!({"id":"b","type":"ssh","proxyProfileId":"other"}),
        json!({"id":"c","type":"ssh","authType":"sshConfig"}),
        json!({"id":"d","type":"sftp"}),
        json!({"id":"e","type":"ssh","jumpProfileId":"a"}),
        json!({"id":"f","type":"ssh","jumpProfileIds":["a"]}),
    ];
    for id in ["a", "b", "c", "d", "e", "f", "deleted"] {
        let mut profiles = before.clone();
        assert!(
            apply_assignments(
                &mut profiles,
                &shared_proxy(),
                &[
                    assign("a", ProxySource::Direct),
                    assign(id, ProxySource::Direct),
                ]
            )
            .is_err(),
            "{id}"
        );
        assert_eq!(profiles, before);
    }
}

#[test]
fn a_proxy_on_the_first_jump_is_allowed_but_on_a_later_jump_is_rejected() {
    let mut profiles = vec![
        json!({"id":"first","type":"ssh","jumpProfileId":"ignored","jumpProfileIds":[]}),
        json!({"id":"later","type":"ssh"}),
        json!({"id":"target","type":"ssh","jumpProfileIds":["first","later"]}),
        json!({"id":"legacy","type":"ssh","jumpProfileId":"first"}),
    ];
    assert!(apply_assignments(
        &mut profiles,
        &shared_proxy(),
        &[assign("later", ProxySource::Direct)]
    )
    .is_err());
    assert!(apply_assignments(
        &mut profiles,
        &shared_proxy(),
        &[assign("first", ProxySource::Direct)]
    )
    .unwrap());
}

#[test]
fn shared_proxy_resolves_without_a_profile_secret_store_and_keeps_its_password_in_the_proxy_store()
{
    let directory =
        std::env::temp_dir().join(format!("fileterm-shared-proxy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("profile-secrets.json");
    std::fs::write(
        directory.join("proxies.json"),
        serde_json::to_vec(&[shared_proxy()]).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.join("proxy-secrets.json"),
        serde_json::to_vec(&json!({"passwords":{"shared":"shared-password"}})).unwrap(),
    )
    .unwrap();
    let mut profiles = vec![
        json!({"id":"a","type":"ssh","proxyProfileId":"shared"}),
        json!({"id":"b","type":"ssh","proxy":{"type":"http","password":"custom-password"}}),
    ];
    hydrate_profile_secrets(&path, &mut profiles).unwrap();
    assert_eq!(profiles[0]["proxy"]["password"], "shared-password");

    let stored = build_profile_secrets(&path, &profiles, None).unwrap();
    assert!(stored["profiles"].get("a").is_none());
    assert!(stored["profiles"]["b"].get("proxyPassword").is_some());
    assert!(strip_secret_fields_public(&profiles[0])["proxy"]
        .get("password")
        .is_none());
    assert_eq!(profiles[0]["proxy"]["username"], "proxy-user");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn decodes_the_renderer_assignment_payload() {
    let input: SshProxyAssignmentInput = serde_json::from_value(json!({
        "proxyId":"shared",
        "changes":[
            {"profileId":"a","assigned":true,"expected":{
                "kind":"custom","type":"http","host":"old","port":1080,"username":"login"
            }},
            {"profileId":"b","assigned":false,"expected":{"kind":"saved","proxyId":"shared"}}
        ]
    }))
    .unwrap();
    assert_eq!(input.changes[0].profile_id, "a");
    assert!(
        matches!(input.changes[0].expected, ProxySource::Custom {ref username, ..} if username.as_deref() == Some("login"))
    );
    assert!(
        matches!(input.changes[1].expected, ProxySource::Saved {ref proxy_id} if proxy_id == "shared")
    );
}
