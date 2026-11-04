#[test]
fn ssh_route_resolves_outermost_first_and_preserves_each_hops_credentials() {
    let entry = serde_json::json!({"id":"entry", "type":"ssh", "password":"entry-secret"});
    let middle = serde_json::json!({"id":"middle", "type":"ssh", "jumpProfileId":"entry", "password":"middle-secret"});
    let target = serde_json::json!({"id":"target", "type":"ssh", "jumpProfileId":"middle", "password":"target-secret"});
    let route = super::resolve_ssh_route(&target, &[middle, entry]).unwrap();
    assert_eq!(
        route
            .iter()
            .map(|hop| hop["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["entry", "middle", "target"]
    );
    assert_eq!(
        route
            .iter()
            .map(|hop| hop["password"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["entry-secret", "middle-secret", "target-secret"]
    );
    assert_eq!(
        super::resolve_ssh_route(
            &serde_json::json!({"type":"ssh", "jumpProfileId":null}),
            &[]
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn ssh_route_preserves_each_hops_legacy_transport_overrides() {
    let entry = serde_json::json!({
        "id":"entry", "type":"ssh", "useEmptyPassword":false, "legacyAlgorithms":false,
        "connectionOverrides":{"useEmptyPassword":true, "legacyAlgorithms":true}
    });
    let target = serde_json::json!({"id":"target", "type":"ssh", "jumpProfileId":"entry"});
    let route = super::resolve_ssh_route(&target, std::slice::from_ref(&entry)).unwrap();
    assert_eq!(route[0]["useEmptyPassword"], true);
    assert_eq!(route[0]["legacyAlgorithms"], true);
    assert_eq!(entry["useEmptyPassword"], false);
    assert_eq!(entry["legacyAlgorithms"], false);
    assert!(route[1].get("useEmptyPassword").is_none());
}

#[test]
fn ssh_route_rejects_invalid_references_and_conflicts_before_connecting() {
    let target = serde_json::json!({"id":"target", "type":"ssh", "jumpProfileId":"entry"});
    assert!(super::resolve_ssh_route(&target, &[])
        .unwrap_err()
        .contains("not found"));
    for protocol in ["ftp", "serial", "telnet"] {
        let entry = serde_json::json!({"id":"entry", "type":protocol});
        assert!(super::resolve_ssh_route(&target, &[entry])
            .unwrap_err()
            .contains("must be an SSH"));
    }
    let entry = serde_json::json!({"id":"entry", "type":"ssh", "jumpProfileId":"target"});
    assert!(super::resolve_ssh_route(&target, &[entry])
        .unwrap_err()
        .contains("cycle"));
    let self_route = serde_json::json!({"id":"self", "type":"ssh", "jumpProfileId":"self"});
    assert!(super::resolve_ssh_route(&self_route, &[])
        .unwrap_err()
        .contains("cycle"));
    for jump_id in [serde_json::json!(""), serde_json::json!(42)] {
        assert!(super::resolve_ssh_route(
            &serde_json::json!({"type":"ssh", "jumpProfileId":jump_id}),
            &[]
        )
        .is_err());
    }
    let mut profiles = vec![
        serde_json::json!({"id":"entry", "type":"ssh", "jumpProfileId":"outer", "proxy":{"type":"socks5"}}),
    ];
    assert!(super::resolve_ssh_route(&target, &profiles)
        .unwrap_err()
        .contains("mutually exclusive"));
    profiles[0] = serde_json::json!({"id":"entry", "type":"ssh", "port":70000});
    assert!(super::resolve_ssh_route(&target, &profiles).is_err());
    profiles[0] = serde_json::json!({"id":"entry", "type":"ssh", "jumpProfileId":"outer", "proxyProfileId":"saved-proxy"});
    assert!(super::resolve_ssh_route(&target, &profiles)
        .unwrap_err()
        .contains("mutually exclusive"));
}

#[test]
fn ssh_route_accepts_eight_jump_hosts_and_rejects_nine() {
    let mut profiles = (0..9)
        .map(|index| serde_json::json!({"id":format!("hop-{index}"), "type":"ssh"}))
        .collect::<Vec<_>>();
    for (index, profile) in profiles.iter_mut().enumerate().skip(1) {
        profile["jumpProfileId"] = serde_json::json!(format!("hop-{}", index - 1));
    }
    let mut target = serde_json::json!({"id":"target", "type":"ssh", "jumpProfileId":"hop-7"});
    assert_eq!(
        super::resolve_ssh_route(&target, &profiles).unwrap().len(),
        9
    );
    target["jumpProfileId"] = serde_json::json!("hop-8");
    assert!(super::resolve_ssh_route(&target, &profiles)
        .unwrap_err()
        .contains("at most 8"));
    assert_eq!(super::SshAuthenticationTarget::JumpHost(6).hop_index(), 6);
    assert_eq!(super::SshAuthenticationTarget::Target(8).hop_index(), 8);
}

#[cfg(unix)]
#[tokio::test]
async fn real_openssh_multi_hop_routes_authenticate_exec_sftp_and_release_every_session() {
    let fixtures = (0..4).map(|_| start_openssh_fixture()).collect::<Vec<_>>();
    for fixture in &fixtures {
        wait_for_openssh(fixture.port).await;
    }
    for length in 1..=fixtures.len() {
        let first = authenticate_openssh_fixture(&fixtures[0], &serde_json::json!({})).await;
        let mut cleanup = super::SshSessionCleanup {
            handles: vec![Arc::new(first)],
        };
        for fixture in fixtures.iter().take(length).skip(1) {
            let upstream = cleanup.handles.last().unwrap();
            let channel = super::open_jump_channel(
                upstream,
                "127.0.0.1",
                fixture.port,
                Duration::from_secs(5),
            )
            .await
            .unwrap();
            let handle = client::connect_stream(
                Arc::new(client::Config::default()),
                channel.into_stream(),
                AcceptTestServerKey,
            )
            .await
            .unwrap();
            cleanup.handles.push(Arc::new(handle));
            let handle = Arc::get_mut(cleanup.handles.last_mut().unwrap()).unwrap();
            let wrong_key = russh::keys::decode_secret_key(
                &std::fs::read_to_string(&fixtures[0].client_key).unwrap(),
                None,
            )
            .unwrap();
            assert!(
                !handle
                    .authenticate_publickey(
                        current_test_username(),
                        russh::keys::PrivateKeyWithHashAlg::new(Arc::new(wrong_key), None)
                    )
                    .await
                    .unwrap()
                    .success(),
                "a hop accepted another hop's key"
            );
            let key = russh::keys::decode_secret_key(
                &std::fs::read_to_string(&fixture.client_key).unwrap(),
                None,
            )
            .unwrap();
            assert!(handle
                .authenticate_publickey(
                    current_test_username(),
                    russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None)
                )
                .await
                .unwrap()
                .success());
        }
        let target = cleanup.handles.last().unwrap();
        let command =
            crate::sessions::system_metrics::exec_command(target, "printf 'multi-hop-target'")
                .await
                .unwrap();
        assert_eq!(command, "multi-hop-target");
        let channel = target.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .unwrap();
        let path = fixtures[length - 1]
            .remote_dir
            .join("multi-hop.txt")
            .to_string_lossy()
            .into_owned();
        sftp.create(&path).await.unwrap();
        sftp.write(&path, b"multi-hop-sftp").await.unwrap();
        assert_eq!(sftp.read(&path).await.unwrap(), b"multi-hop-sftp");
        // Keep channel owners alive: dropping a Handle alone does not close these sockets.
        let handles = cleanup.handles.clone();
        drop(cleanup);
        timeout(Duration::from_secs(5), async {
            while handles.iter().any(|handle| !handle.is_closed()) {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("SSH route cleanup left a hop connected");
        drop(sftp);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn real_openssh_failed_or_canceled_jump_setup_releases_upstream_sessions() {
    let fixture = start_openssh_fixture();
    wait_for_openssh(fixture.port).await;
    let handle = Arc::new(authenticate_openssh_fixture(&fixture, &serde_json::json!({})).await);
    let cleanup = super::SshSessionCleanup {
        handles: vec![handle.clone()],
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unused_port = listener.local_addr().unwrap().port();
    drop(listener);
    assert!(
        super::open_jump_channel(&handle, "127.0.0.1", unused_port, Duration::from_secs(2))
            .await
            .is_err()
    );
    drop(cleanup);
    timeout(Duration::from_secs(3), async {
        while !handle.is_closed() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let handle = Arc::new(authenticate_openssh_fixture(&fixture, &serde_json::json!({})).await);
    let cleanup = super::SshSessionCleanup {
        handles: vec![handle.clone()],
    };
    let (ready, started) = oneshot::channel();
    let task = tokio::spawn(async move {
        let _cleanup = cleanup;
        ready.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    started.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    timeout(Duration::from_secs(3), async {
        while !handle.is_closed() {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("canceling jump setup left the upstream connected");
}
