#[cfg(test)]
mod ssh_route_policy_tests {
    use super::*;
    use serde_json::json;

    fn target(port: u16) -> SshRouteTarget {
        SshRouteTarget {
            profile_id: uuid::Uuid::new_v4().to_string(),
            host: "127.0.0.1".into(),
            port,
        }
    }

    fn selection(rule: Value, storage_root: PathBuf) -> SshRouteSelection {
        SshRouteSelection {
            scope: "test",
            label: String::new(),
            rule: serde_json::from_value(rule).unwrap(),
            storage_root,
        }
    }

    fn store_http_proxy(port: u16) -> PathBuf {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("proxies.json"), json!([{
            "id": "proxy1", "name": "fixture", "type": "http", "host": "127.0.0.1", "port": port,
        }]).to_string()).unwrap();
        root
    }

    #[test]
    fn ssh_route_policy_resolves_server_tags_and_default() {
        let direct = json!({"mode": "fixed", "route": {"type": "direct"}});
        let proxy = json!({"mode": "fixed", "route": {"type": "proxy", "proxyId": "edge"}});
        let config: SshRoutePolicy = serde_json::from_value(json!({
            "servers": {"explicit": direct},
            "tags": {
                "lan": {"priority": 10, "rule": direct},
                "vpn": {"priority": 10, "rule": proxy},
                "office": {"priority": 11, "rule": proxy}
            },
            "default": direct,
        }))
        .unwrap();
        let select = |profile| config.select(&profile, PathBuf::new());
        let explicit =
            select(json!({"id": "explicit", "proxyProfileId": "legacy", "tags": ["lan", "vpn"]}))
                .unwrap()
                .unwrap();
        assert_eq!(explicit.scope, "server");
        let tagged = select(json!({"tags": ["lan", "vpn", "office"]}))
            .unwrap()
            .unwrap();
        assert_eq!(tagged.label, "office");
        assert!(select(json!({"tags": ["lan", "vpn"]})).is_err());
        assert_eq!(select(json!({})).unwrap().unwrap().scope, "default");
        assert!(select(json!({"proxyProfileId": "legacy", "tags": ["lan"]}))
            .unwrap()
            .is_none());
        assert!(
            select(json!({"sshNetworkInterface": {"adapterId": "legacy"}}))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ssh_route_policy_rejects_empty_and_duplicate_candidates() {
        for rule in [
            json!({"mode": "auto", "routes": []}),
            json!({"mode": "preferred", "primary": {"type": "direct"}, "fallback": [{"type": "direct"}]}),
        ] {
            assert!(selection(rule, PathBuf::new()).rule.validate().is_err());
        }
    }

    #[test]
    fn new_ssh_route_draft_is_validated_before_persisting_or_testing() {
        let fixed = json!({"mode": "fixed", "route": {"type": "direct"}});
        assert!(parse_ssh_route_draft(&json!({"type": "ssh"}), fixed.clone()).is_ok());
        assert!(parse_ssh_route_draft(
            &json!({"type": "ssh", "jumpProfileIds": ["hop"]}),
            fixed.clone()
        )
        .is_err());
        assert!(parse_ssh_route_draft(&json!({"type": "sftp"}), fixed).is_err());
        assert!(parse_ssh_route_draft(
            &json!({"type": "ssh"}),
            json!({"mode": "auto", "routes": []})
        )
        .is_err());
    }

    #[test]
    fn ssh_route_policy_health_penalizes_failures_then_recovers() {
        let now = Instant::now();
        let delay = Duration::from_millis;
        let mut stable = SshRouteHealth::new(now);
        let mut flaky = SshRouteHealth::new(now);
        for sample in [85, 95, 90, 98] {
            stable.observe(now, delay(sample), true);
        }
        for (ms, success) in [(10, true), (200, true), (12, false), (15, false)] {
            flaky.observe(now, delay(ms), success);
        }
        assert!(flaky.cooling(now + Duration::from_secs(5)));
        assert!(!flaky.cooling(now + Duration::from_secs(11)));
        assert!(stable.score(now) < flaky.score(now));
        for _ in 0..12 {
            flaky.observe(now + Duration::from_secs(12), delay(10), true);
        }
        assert!(!flaky.cooling(now + Duration::from_secs(12)));
        assert!(flaky.score(now + Duration::from_secs(12)) < stable.score(now));
    }

    #[test]
    fn ssh_route_policy_discovers_only_local_v2ray_inbounds_without_credentials() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        let path = root.join("test.json");
        std::fs::write(&path, json!({"inbounds": [
            {"listen": "127.0.0.1", "port": 10801, "protocol": "socks"},
            {"listen": "::1", "port": 10802, "protocol": "http"},
            {"listen": "0.0.0.0", "port": 10803, "protocol": "socks"},
            {"listen": "127.0.0.1", "port": 10804, "protocol": "socks", "settings": {"auth": "password"}},
            {"listen": "127.0.0.1", "port": 10805, "protocol": "http", "settings": {"accounts": [{"user": "fixture"}]}}
        ]}).to_string()).unwrap();
        let endpoints = v2ray_route_endpoints(&path);
        assert_eq!(
            endpoints
                .iter()
                .map(|item| (item.kind, item.port))
                .collect::<Vec<_>>(),
            [("socks5", 10801), ("http", 10802)]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    struct RouteFixtureProcess(RouteClientChild);

    impl Drop for RouteFixtureProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[tokio::test]
    #[ignore = "requires FILETERM_V2RAY_QA_EXECUTABLE and FILETERM_SSH_QA_IP"]
    async fn ssh_route_policy_live_v2ray_failover() {
        let executable = PathBuf::from(std::env::var_os("FILETERM_V2RAY_QA_EXECUTABLE").unwrap());
        let host = std::env::var("FILETERM_SSH_QA_IP").unwrap();
        let _ip: std::net::IpAddr = host.parse().unwrap();
        let port_reservation = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_port = port_reservation.local_addr().unwrap().port();
        drop(port_reservation);
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        let config = root.join("v2ray.json");
        std::fs::write(&config, json!({
            "inbounds": [{"listen": "127.0.0.1", "port": proxy_port, "protocol": "socks"}],
            "outbounds": [{"protocol": "blackhole", "tag": "deny"}, {"protocol": "freedom", "tag": "fixture"}],
            "routing": {"rules": [{"type": "field", "ip": [host], "port": 22, "outboundTag": "fixture"}]}
        }).to_string()).unwrap();
        std::fs::write(root.join("proxies.json"), json!([{
            "id": "v2ray-qa", "name": "fixture", "type": "socks5", "host": "127.0.0.1", "port": proxy_port,
        }]).to_string()).unwrap();
        let mut proxy = RouteFixtureProcess(
            make_ssh_route_client_command(
                SshRouteClient::V2ray,
                &SshRouteClientConfig {
                    executable,
                    config: Some(config),
                },
            )
            .unwrap()
            .spawn()
            .unwrap(),
        );
        timeout(Duration::from_secs(5), async {
            while TcpStream::connect(("127.0.0.1", proxy_port)).await.is_err() {
                sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("V2Ray loopback SOCKS5 listener");
        let reference = json!({"type": "proxy", "proxyId": "v2ray-qa"});
        let preferred = selection(
            json!({"mode": "preferred", "primary": reference, "fallback": [{"type": "direct"}]}),
            root.clone(),
        );
        let fixed = selection(json!({"mode": "fixed", "route": reference}), root.clone());
        let target = SshRouteTarget { host, ..target(22) };
        let budget = Duration::from_secs(6);
        let (mut stream, used) = preferred
            .connect_target(target.clone(), budget, |_, _, _, _| {})
            .await
            .unwrap();
        let mut banner = [0; 4];
        timeout(budget, stream.read_exact(&mut banner))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&banner, b"SSH-");
        assert!(matches!(used.route, SshRouteCandidate::Proxy { .. }));
        drop(stream);
        proxy.0.kill().unwrap();
        proxy.0.wait().unwrap();
        let mut observed = Vec::new();
        let (mut stream, used) = preferred
            .connect_target(target.clone(), budget, |_, _, failure, fallback| {
                observed.push((failure, fallback));
            })
            .await
            .unwrap();
        timeout(budget, stream.read_exact(&mut banner))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&banner, b"SSH-");
        assert!(matches!(used.route, SshRouteCandidate::Direct));
        assert!(matches!(
            observed.as_slice(),
            [
                (Some(SshRouteFailure::TransportFailed), false),
                (None, true)
            ]
        ));
        assert!(fixed
            .connect_target(target, budget, |_, _, _, _| {})
            .await
            .is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ssh_route_policy_preferred_falls_back_and_fixed_stops() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = store_http_proxy(proxy_listener.local_addr().unwrap().port());
        let failed_proxy = tokio::spawn(async move {
            let (mut socket, _) = proxy_listener.accept().await.unwrap();
            socket
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n")
                .await
                .unwrap();
        });
        let target = target(listener.local_addr().unwrap().port());
        let missing = json!({"type": "proxy", "proxyId": "missing"});
        let fixed = selection(json!({"mode": "fixed", "route": missing}), root.clone());
        let preferred = selection(
            json!({
                "mode": "preferred", "primary": {"type": "proxy", "proxyId": "proxy1"},
                "fallback": [{"type": "direct"}],
            }),
            root.clone(),
        );

        let mut fixed_attempts = Vec::new();
        assert!(fixed
            .connect_target(
                target.clone(),
                Duration::from_secs(2),
                |_, _, failure, fallback| {
                    fixed_attempts.push((failure, fallback));
                }
            )
            .await
            .is_err());
        assert!(matches!(
            fixed_attempts.as_slice(),
            [(Some(SshRouteFailure::MissingProxy), false)]
        ));

        let mut preferred_attempts = Vec::new();
        let (mut stream, selected) = preferred
            .connect_target(target, Duration::from_secs(2), |_, _, failure, fallback| {
                preferred_attempts.push((failure, fallback));
            })
            .await
            .unwrap();
        assert!(matches!(selected.route, SshRouteCandidate::Direct));
        assert!(matches!(
            preferred_attempts.as_slice(),
            [
                (Some(SshRouteFailure::TransportFailed), false),
                (None, true)
            ]
        ));
        let (mut peer, _) = listener.accept().await.unwrap();
        peer.write_all(b"ssh").await.unwrap();
        let mut bytes = [0; 3];
        stream.read_exact(&mut bytes).await.unwrap();
        assert_eq!(&bytes, b"ssh");
        failed_proxy.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ssh_route_policy_skips_routes_that_failed_during_ssh_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = store_http_proxy(1);
        let primary = SshRouteCandidate::Proxy {
            proxy_id: "proxy1".into(),
        };
        let selection = selection(
            json!({
                "mode": "preferred", "primary": {"type": "proxy", "proxyId": "proxy1"},
                "fallback": [{"type": "direct"}],
            }),
            root.clone(),
        );
        let mut excluded = std::collections::HashSet::from([primary.clone()]);
        let mut attempts = Vec::new();
        let (stream, selected) = selection
            .connect_target_excluding(
                target(listener.local_addr().unwrap().port()),
                Duration::from_secs(2),
                &mut excluded,
                |route, _, _, fallback| attempts.push((route.clone(), fallback)),
            )
            .await
            .unwrap();
        assert!(matches!(selected.route, SshRouteCandidate::Direct));
        assert!(matches!(attempts.as_slice(), [(SshRouteCandidate::Direct, true)]));
        assert!(excluded.contains(&primary));
        let (peer, _) = listener.accept().await.unwrap();
        drop((stream, peer));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn ssh_route_policy_uses_saved_http_proxy_and_preserves_ssh_bytes() {
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = store_http_proxy(proxy_listener.local_addr().unwrap().port());
        let target = SshRouteTarget {
            host: "ssh.example.invalid".into(),
            ..target(2222)
        };
        let proxy = tokio::spawn(async move {
            let (socket, _) = proxy_listener.accept().await.unwrap();
            let mut socket = tokio::io::BufReader::new(socket);
            let mut request = String::new();
            use tokio::io::AsyncBufReadExt;
            while socket.read_line(&mut request).await.unwrap() != 0
                && !request.ends_with("\r\n\r\n")
            {}
            assert!(request.starts_with("CONNECT ssh.example.invalid:2222 HTTP/1.1\r\n"));
            socket
                .into_inner()
                .write_all(b"HTTP/1.1 200 Connected\r\n\r\nSSH-2.0-fixture\r\n")
                .await
                .unwrap();
        });
        let auto = selection(
            json!({
                "mode": "auto", "routes": [{"type": "proxy", "proxyId": "missing"}, {"type": "proxy", "proxyId": "proxy1"}]
            }),
            root.clone(),
        );
        let (mut stream, selected) = auto
            .connect_target(target.clone(), Duration::from_secs(2), |_, _, _, _| {})
            .await
            .unwrap();
        let mut greeting = [0; 17];
        stream.read_exact(&mut greeting).await.unwrap();
        assert_eq!(&greeting, b"SSH-2.0-fixture\r\n");
        proxy.await.unwrap();
        let endpoint = selected.authenticated(&json!({"host": target.host, "port": target.port}));
        assert_ne!(
            endpoint,
            crate::sessions::transfer_endpoint::fingerprint(&[
                json!({"host": target.host, "port": target.port})
            ])
        );
        let reference = SshRouteCandidate::Proxy {
            proxy_id: "proxy1".into(),
        };
        assert!(SSH_ROUTE_HEALTH
            .lock()
            .unwrap()
            .remove(&(target, reference))
            .unwrap()
            .authenticated
            .is_some());
        std::fs::remove_dir_all(root).unwrap();
    }
}
