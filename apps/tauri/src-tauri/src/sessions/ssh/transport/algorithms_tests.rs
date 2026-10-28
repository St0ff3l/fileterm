use super::build_ssh_algorithm_config;
use russh::{
    client,
    kex::{
        self,
        dh::groups::{DhGroup, DH_GROUP1, DH_GROUP16},
    },
    server,
};
use std::{
    borrow::Cow,
    sync::{Arc, Mutex},
    time::Duration,
};

struct TestClient;

impl client::Handler for TestClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

type GexRequest = (usize, usize, usize);

struct FixedGroupServer {
    group: DhGroup,
    request: Arc<Mutex<Option<GexRequest>>>,
}

impl server::Handler for FixedGroupServer {
    type Error = russh::Error;

    async fn lookup_dh_gex_group(
        &mut self,
        params: &client::GexParams,
    ) -> Result<Option<DhGroup>, Self::Error> {
        *self.request.lock().unwrap() = Some((
            params.min_group_size(),
            params.preferred_group_size(),
            params.max_group_size(),
        ));
        // Reproduce a server returning its fixed group even outside the requested range.
        Ok(Some(self.group.clone()))
    }

    async fn auth_none(&mut self, _: &str) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }
}

async fn handshake(
    legacy: bool,
    banner: &'static str,
    algorithm: kex::Name,
    group: DhGroup,
    expected_request: GexRequest,
    expect_success: bool,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let request = Arc::new(Mutex::new(None));
    let mut config = server::Config {
        server_id: russh::SshId::Standard(Cow::Borrowed(banner)),
        ..Default::default()
    };
    config.preferred.kex = Cow::Owned(vec![algorithm]);
    config.keys.push(
        russh::keys::PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519)
            .unwrap(),
    );
    let handler = FixedGroupServer {
        group,
        request: request.clone(),
    };
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        if let Ok(session) = server::run_stream(Arc::new(config), stream, handler).await {
            let _ = session.await;
        }
    });
    let result = tokio::time::timeout(Duration::from_secs(15), async {
        let mut client = client::connect(
            Arc::new(build_ssh_algorithm_config(legacy).unwrap()),
            address,
            TestClient,
        )
        .await?;
        let authenticated = client.authenticate_none("fixture").await?.success();
        client
            .disconnect(russh::Disconnect::ByApplication, "done", "en")
            .await?;
        Ok::<_, russh::Error>(authenticated)
    })
    .await;
    server.abort();
    let _ = server.await;

    let result = result.expect("local GEX handshake timed out");
    if expect_success {
        assert!(result.expect("GEX handshake should succeed"));
    } else {
        assert!(matches!(result, Err(russh::Error::KexInit)), "{result:?}");
    }
    assert_eq!(*request.lock().unwrap(), Some(expected_request));
}

#[tokio::test]
async fn legacy_gex_accepts_1024_bit_groups_for_both_hashes() {
    for algorithm in [kex::DH_GEX_SHA1, kex::DH_GEX_SHA256] {
        handshake(
            true,
            "SSH-2.0-fixed-group-fixture",
            algorithm,
            DH_GROUP1,
            (1024, 8192, 8192),
            true,
        )
        .await;
    }
}

#[tokio::test]
async fn default_gex_rejects_1024_bit_groups() {
    handshake(
        false,
        "SSH-2.0-fixed-group-fixture",
        kex::DH_GEX_SHA256,
        DH_GROUP1,
        (3072, 8192, 8192),
        false,
    )
    .await;
}

#[tokio::test]
async fn legacy_gex_still_rejects_out_of_range_groups() {
    for algorithm in [kex::DH_GEX_SHA1, kex::DH_GEX_SHA256] {
        for bytes in [64, 1025] {
            // Deliberately invalid 512/8200-bit parameters must be rejected
            // by the size guard, before any DH calculation or authentication.
            let group = DhGroup {
                prime: vec![0xff; bytes].into(),
                generator: vec![2].into(),
            };
            handshake(
                true,
                "SSH-2.0-fixed-group-fixture",
                algorithm,
                group,
                (1024, 8192, 8192),
                false,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn modern_gex_still_works_with_either_policy() {
    for legacy in [false, true] {
        handshake(
            legacy,
            "SSH-2.0-fixed-group-fixture",
            kex::DH_GEX_SHA256,
            DH_GROUP16,
            (if legacy { 1024 } else { 3072 }, 8192, 8192),
            true,
        )
        .await;
    }
}

#[tokio::test]
async fn comware_sha1_keeps_its_existing_preferred_group() {
    handshake(
        true,
        "SSH-2.0-Comware-7.1",
        kex::DH_GEX_SHA1,
        DH_GROUP1,
        (1024, 1024, 8192),
        true,
    )
    .await;
}

#[test]
fn legacy_policy_preserves_modern_algorithm_preference_and_default_policy() {
    let default = build_ssh_algorithm_config(false).unwrap();
    assert!(!default.comware_legacy_gex);
    assert_eq!(default.preferred.kex, russh::Preferred::DEFAULT.kex);
    assert_eq!(default.preferred.mac, russh::Preferred::DEFAULT.mac);
    let legacy = build_ssh_algorithm_config(true).unwrap();
    assert!(legacy.comware_legacy_gex);
    assert!(legacy
        .preferred
        .kex
        .starts_with(&russh::Preferred::DEFAULT.kex));
    assert!(legacy
        .preferred
        .mac
        .starts_with(&russh::Preferred::DEFAULT.mac));
}
