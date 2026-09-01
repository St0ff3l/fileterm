#[cfg(test)]
mod tests {
    use super::{
        authentication_result_from_auth_result, build_http_connect_request, build_legacy_preferred,
        capture_root_access_password_input, coalesce_terminal_input, contains_interrupt_byte,
        decode_bytes, default_ssh_key_paths, detect_network_device_family,
        detect_remote_exec_input_kind, effective_exec_channel_enabled, effective_remote_file_type,
        effective_remote_forward_port, effective_resource_monitoring_enabled,
        effective_sftp_enabled, encode_text, enqueue_tunnel_command, exec_channel_enabled,
        finish_shell_setup_suppression, format_sftp_unavailable_reason,
        initial_remote_listing_can_be_fallback, initial_remote_listing_matches_current_session,
        is_implicit_ssh_home_path, is_password_prompt, is_root_upload_staging_path,
        is_sftp_path_not_found_message, looks_like_mfa_prompt, looks_like_root_prompt,
        looks_like_shell_prompt, merge_system_metrics_history, missing_password_credential,
        normalize_ssh_identification, parent_remote_item, parent_remote_path,
        parse_root_file_access_method, parse_root_file_list, password_for_authentication,
        privilege_command_from_terminal_input, profile_with_resolved_device_mode,
        remote_bind_host_matches, resolve_shell_file_access, resolve_ssh_device_mode,
        resource_monitoring_enabled, resource_monitoring_interval_seconds, root_access_auth_failed,
        root_editor_verify_shell_command, root_editor_write_shell_command, root_file_command,
        root_list_shell_command, root_replace_remote_file_command, root_stat_shell_command,
        root_upload_base64_shell_command, root_upload_shell_command, shell_cwd_setup_for_platform,
        shell_cwd_sftp_path_candidates, should_buffer_terminal_input_during_shell_setup,
        should_reinject_root_shell_setup, should_restart_keyboard_interactive,
        spawn_cancellable_file_operation, split_prompt_tail_for_setup_wait, ssh_terminal_type,
        strip_su_exec_output, su_exec_command, suppress_shell_setup_echo, track_cwd_and_user,
        track_root_access_prompt_from_terminal, trim_string_front, trusted_host_fingerprint,
        try_keyboard_interactive_with_responder, tunnel_bind_address,
        validate_root_download_completion, validate_tunnel_rule,
        wait_for_ssh_handshake_with_timeouts, wait_for_ssh_stage, AuthenticationResult,
        KeyboardInteractiveMode, KeyboardInteractiveRequest, ResolvedSshDeviceMode, RootFileAccessMethod,
        ShellSetupEchoSuppression, SshDeviceModeResolution, SshTunnelRule, TunnelCommand,
        SshInteractionContext, SshInteractionFlow, SshAuthenticationTarget,
        SshInteractionWaitResult, wait_for_ssh_interaction,
        BUSYBOX_SHELL_CWD_SETUP, SHELL_CWD_SETUP, SHELL_SETUP_SETTLE_DELAY, SU_EXEC_OUTPUT_MARKER,
    };
    #[cfg(unix)]
    use super::{forward_local_connection, forward_socks5_connection};
    use std::borrow::Cow;
    use std::path::Path;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use std::time::Instant;

    use russh::keys::PrivateKey;
    use russh::{client, server, AuthResult, ChannelMsg, MethodKind, MethodSet};
    #[cfg(unix)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{mpsc, oneshot};
    use tokio::time::{sleep, timeout, Duration};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn detached_file_operation_stops_when_the_command_is_cancelled() {
        let cancellation = CancellationToken::new();
        let (respond_to, response) = oneshot::channel();
        spawn_cancellable_file_operation(cancellation.clone(), respond_to, async {
            sleep(Duration::from_secs(60)).await;
            Ok::<(), String>(())
        });

        cancellation.cancel();

        assert_eq!(
            response.await.unwrap(),
            Err("远程文件操作已取消".to_string())
        );
    }

    #[test]
    fn resource_monitoring_respects_explicit_profile_disable() {
        assert!(resource_monitoring_enabled(&serde_json::json!({})));
        assert!(resource_monitoring_enabled(&serde_json::json!({
            "enableResourceMonitoring": true
        })));
        assert!(!resource_monitoring_enabled(&serde_json::json!({
            "enableResourceMonitoring": false
        })));
    }

    #[test]
    fn partial_metrics_keep_last_filesystem_sections() {
        let previous = serde_json::json!({
            "diskRows": [{"path": "/", "usage": "60 GB/100 GB"}],
            "fileSystemRows": [{"mountPoint": "/", "available": "60 GB", "size": "100 GB"}],
            "networkSamples": [],
            "networkSamplesByInterface": {}
        });
        let next = serde_json::json!({
            "diskRows": [],
            "fileSystemRows": [],
            "networkSamples": [],
            "networkSamplesByInterface": {}
        });

        let merged = merge_system_metrics_history(Some(&previous), next, 600);

        assert_eq!(merged["diskRows"][0]["path"], "/");
        assert_eq!(merged["fileSystemRows"][0]["mountPoint"], "/");
    }

    #[test]
    fn remote_symlinks_use_their_target_type_for_navigation() {
        assert_eq!(effective_remote_file_type(false, true, false), "file");
        assert_eq!(effective_remote_file_type(false, true, true), "folder");
        assert_eq!(effective_remote_file_type(true, false, false), "folder");
    }

    #[test]
    fn editor_encoding_rejects_corrupt_utf8_and_lossy_output() {
        assert_eq!(
            decode_bytes(b"\xef\xbb\xbf# comment\n", "utf-8").unwrap(),
            "# comment\n"
        );
        assert!(decode_bytes(b"valid\xff", "utf-8").is_err());
        assert_eq!(encode_text("中文", "utf-8").unwrap(), "中文".as_bytes());
        assert!(encode_text("emoji 😀", "gbk").is_err());
    }

    #[test]
    fn root_editor_write_is_staged_and_size_checked_before_commit() {
        let write_command = root_editor_write_shell_command("/etc/.fileterm-edit", 42);
        assert!(write_command.contains("base64 -d > '/etc/.fileterm-edit'"));
        assert!(write_command.contains("wc -c < '/etc/.fileterm-edit'"));
        assert!(write_command.contains("-eq 42"));
        assert!(!write_command.contains("tee"));

        let verify_command = root_editor_verify_shell_command("/etc/sysctl.conf", 42);
        assert!(verify_command.contains("wc -c < '/etc/sysctl.conf'"));
        assert!(verify_command.ends_with("-eq 42"));

        let replace_command = root_replace_remote_file_command(
            "/etc",
            "/etc/.fileterm-edit",
            "/etc/sysctl.d/99-sysctl.conf",
        );
        assert!(replace_command.contains("readlink -f"));
        assert!(replace_command.contains("mv -f -- '/etc/.fileterm-edit' \"$target\""));
    }

    #[test]
    fn root_listing_keeps_symlink_metadata_and_parses_target_type() {
        assert!(root_list_shell_command("/etc/sysctl.d")
            .contains("-printf '%y|%Y|%s|%T@|%u:%g|%m|%f\\n'"));

        let items = parse_root_file_list(
            "l|f|12|1710000000.0|root:root|644|99-sysctl.conf\nl|d|8|1710000001.0|root:root|755|linked-dir\n",
            "/etc/sysctl.d",
        );
        let file = items
            .iter()
            .find(|item| item["name"] == "99-sysctl.conf")
            .expect("symlink to file should be listed");
        assert_eq!(file["type"], "file");
        assert_eq!(file["isSymlink"], true);

        let folder = items
            .iter()
            .find(|item| item["name"] == "linked-dir")
            .expect("symlink to directory should be listed");
        assert_eq!(folder["type"], "folder");
        assert_eq!(folder["isSymlink"], true);
    }

    #[test]
    fn exec_channel_defaults_to_enabled_and_respects_explicit_disable() {
        assert!(exec_channel_enabled(&serde_json::json!({})));
        assert!(exec_channel_enabled(&serde_json::json!({
            "enableExecChannel": true
        })));
        assert!(!exec_channel_enabled(&serde_json::json!({
            "enableExecChannel": false
        })));
    }

    #[test]
    fn network_device_mode_disables_exec_even_when_legacy_flags_are_enabled() {
        assert!(!effective_exec_channel_enabled(&serde_json::json!({
            "type": "ssh",
            "deviceMode": "network-device",
            "enableExecChannel": true
        })));
        assert!(effective_exec_channel_enabled(&serde_json::json!({
            "type": "ssh",
            "deviceMode": "server",
            "enableExecChannel": true
        })));
    }

    #[test]
    fn huawei_and_h3c_mock_profiles_keep_only_the_raw_terminal_surface() {
        for vendor in ["huawei", "h3c-comware"] {
            let profile = serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device",
                "networkDeviceVendor": vendor,
                "enableExecChannel": true,
                "enableResourceMonitoring": true,
                "sftpEnabled": true
            });

            assert!(!effective_exec_channel_enabled(&profile));
            assert!(!effective_resource_monitoring_enabled(&profile));
            assert!(!effective_sftp_enabled(&profile));
            assert_eq!(ssh_terminal_type(&profile), "vt100");
        }
    }

    #[test]
    fn ssh_banner_detection_matches_netcatty_conservative_vendor_patterns() {
        let cases = [
            ("SSH-2.0-Cisco-1.25", Some("cisco")),
            ("SSH-2.0-CiscoIOS_1.0", Some("cisco")),
            ("SSH-2.0-CISCO_WLC", Some("cisco")),
            ("SSH-2.0-NetScreen-5.0", Some("juniper")),
            ("SSH-2.0-HUAWEI-VRP", Some("huawei")),
            ("SSH-2.0-VRP-Software", Some("huawei")),
            ("SSH-2.0--", Some("huawei")),
            ("SSH-1.99--", Some("huawei")),
            ("-", Some("huawei")),
            ("SSH-2.0-Comware-7.1", Some("h3c-comware")),
            ("SSH-2.0-H3C-SecPath", Some("h3c-comware")),
            ("SSH-2.0-3Com OS-3.0", Some("h3c-comware")),
            ("SSH-2.0-mpSSH_1.0", Some("hpe")),
            ("SSH-2.0-ROSSSH", Some("mikrotik")),
            ("SSH-2.0-FortiSSH_1.0", Some("fortinet")),
            ("SSH-2.0-PaloAltoNetworks_1.0", Some("paloalto")),
            ("SSH-2.0-Zyxel SSH_1.0", Some("zyxel")),
            ("SSH-2.0-RGOS_SSH", Some("ruijie")),
            ("  SSH-2.0-Cisco-1.25  ", Some("cisco")),
            ("SSH-2.0-OpenSSH_9.9", None),
            ("SSH-2.0-dropbear", None),
            ("SSH-2.0-OpenSSH_CiscoIOS_1.0", None),
            ("SSH-2.0-IPSSH-1.0", None),
            ("SSH-2.0-NotComware-1.0", None),
        ];

        for (identification, expected_family) in cases {
            assert_eq!(
                detect_network_device_family(identification),
                expected_family,
                "unexpected family for {identification}"
            );
        }
    }

    #[test]
    fn ssh_banner_normalization_is_bounded_and_log_safe() {
        assert_eq!(
            normalize_ssh_identification(b"SSH-2.0-Cisco-1.25\r\n"),
            "SSH-2.0-Cisco-1.25"
        );
        assert_eq!(
            normalize_ssh_identification(b"SSH-2.0-Cisco\x1b[31m"),
            "SSH-2.0-Cisco[31m"
        );
        assert!(normalize_ssh_identification(&vec![b'c'; 512]).len() <= 255);
    }

    #[test]
    fn manual_ssh_device_mode_overrides_banner_and_auto_falls_back_safely() {
        let cisco_banner = b"SSH-2.0-Cisco-1.25";
        assert_eq!(
            resolve_ssh_device_mode(&serde_json::json!({}), cisco_banner),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::Server,
                source: "legacy-default",
                family: None,
            }
        );
        assert_eq!(
            resolve_ssh_device_mode(&serde_json::json!({ "deviceMode": "server" }), cisco_banner),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::Server,
                source: "manual",
                family: None,
            }
        );
        assert_eq!(
            resolve_ssh_device_mode(
                &serde_json::json!({ "deviceMode": "network-device" }),
                b"SSH-2.0-OpenSSH_9.9"
            ),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::NetworkDevice,
                source: "manual",
                family: None,
            }
        );
        assert_eq!(
            resolve_ssh_device_mode(&serde_json::json!({ "deviceMode": "auto" }), cisco_banner),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::NetworkDevice,
                source: "banner",
                family: Some("cisco"),
            }
        );
        let unknown = resolve_ssh_device_mode(
            &serde_json::json!({ "deviceMode": "auto" }),
            b"SSH-2.0-OpenSSH_9.9",
        );
        assert_eq!(unknown.mode, ResolvedSshDeviceMode::Server);
        assert_eq!(unknown.source, "auto-fallback");
        let vendor_hint = resolve_ssh_device_mode(
            &serde_json::json!({
                "deviceMode": "auto",
                "networkDeviceVendor": "huawei"
            }),
            b"SSH-2.0-dropbear",
        );
        assert_eq!(
            vendor_hint,
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::NetworkDevice,
                source: "vendor-hint",
                family: Some("huawei"),
            }
        );
        let generic_hint = resolve_ssh_device_mode(
            &serde_json::json!({
                "deviceMode": "auto",
                "networkDeviceVendor": "generic"
            }),
            b"SSH-2.0-OpenSSH_9.9",
        );
        assert_eq!(generic_hint.mode, ResolvedSshDeviceMode::NetworkDevice);
        assert_eq!(generic_hint.source, "vendor-hint");
        assert_eq!(generic_hint.family, Some("generic"));
        assert_eq!(
            profile_with_resolved_device_mode(
                &serde_json::json!({ "deviceMode": "auto", "host": "switch" }),
                SshDeviceModeResolution {
                    mode: ResolvedSshDeviceMode::NetworkDevice,
                    source: "banner",
                    family: Some("cisco"),
                }
            )["deviceMode"],
            "network-device"
        );
    }

    #[test]
    fn ssh_terminal_type_defaults_and_rejects_unknown_values_safely() {
        assert_eq!(ssh_terminal_type(&serde_json::json!({})), "xterm-256color");
        let auto_network_profile = profile_with_resolved_device_mode(
            &serde_json::json!({
                "type": "ssh",
                "deviceMode": "auto"
            }),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::NetworkDevice,
                source: "banner",
                family: Some("huawei"),
            },
        );
        assert_eq!(ssh_terminal_type(&auto_network_profile), "vt100");
        let auto_server_profile = profile_with_resolved_device_mode(
            &serde_json::json!({
                "type": "ssh",
                "deviceMode": "auto"
            }),
            SshDeviceModeResolution {
                mode: ResolvedSshDeviceMode::Server,
                source: "auto-fallback",
                family: None,
            },
        );
        assert_eq!(ssh_terminal_type(&auto_server_profile), "xterm-256color");
        assert_eq!(
            ssh_terminal_type(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device"
            })),
            "vt100"
        );
        assert_eq!(
            ssh_terminal_type(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device",
                "terminalType": "ansi"
            })),
            "ansi"
        );
        assert_eq!(
            ssh_terminal_type(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device",
                "terminalType": "unsupported"
            })),
            "vt100"
        );
    }

    #[test]
    fn initial_remote_listing_cannot_overwrite_a_followed_shell_directory() {
        assert!(!initial_remote_listing_matches_current_session(
            "/",
            "/",
            Some("/home/stoffel"),
            true
        ));
        assert!(initial_remote_listing_matches_current_session(
            "/home/stoffel",
            "/home/stoffel",
            Some("/home/stoffel"),
            true
        ));
        assert!(initial_remote_listing_matches_current_session(
            "/", "/", None, true
        ));
        assert!(!initial_remote_listing_matches_current_session(
            "/",
            "/home/stoffel",
            Some("/home/stoffel"),
            true
        ));
    }

    #[test]
    fn stale_initial_remote_listing_is_kept_for_an_unmapped_shell_cwd() {
        assert!(initial_remote_listing_can_be_fallback(
            false, "/", "/", true
        ));
        assert!(!initial_remote_listing_can_be_fallback(
            false, "/", "/", false
        ));
        assert!(!initial_remote_listing_can_be_fallback(
            false,
            "/",
            "/home/stoffel",
            true
        ));
        assert!(!initial_remote_listing_can_be_fallback(
            true, "/", "/", true
        ));
    }

    #[test]
    fn shell_cwd_mapping_does_not_assume_volume1() {
        assert_eq!(
            shell_cwd_sftp_path_candidates("/volume2/photo/albums"),
            vec![
                "/volume2/photo/albums".to_string(),
                "/photo/albums".to_string(),
            ]
        );
        assert_eq!(
            shell_cwd_sftp_path_candidates("/volume7/homes/alice/projects"),
            vec![
                "/volume7/homes/alice/projects".to_string(),
                "/homes/alice/projects".to_string(),
                "/alice/projects".to_string(),
                "/projects".to_string(),
            ]
        );
        assert_eq!(
            shell_cwd_sftp_path_candidates("/volume10foo/photo"),
            vec!["/volume10foo/photo".to_string()]
        );
    }

    #[test]
    fn shell_cwd_mapping_supports_synology_service_paths_and_root() {
        assert_eq!(
            shell_cwd_sftp_path_candidates("/var/services/homes/alice"),
            vec![
                "/var/services/homes/alice".to_string(),
                "/homes/alice".to_string(),
                "/alice".to_string(),
                "/".to_string(),
            ]
        );
        assert_eq!(
            shell_cwd_sftp_path_candidates("/var/services"),
            vec!["/var/services".to_string(), "/".to_string()]
        );
    }

    #[test]
    fn only_no_such_file_errors_trigger_sftp_namespace_fallback() {
        assert!(is_sftp_path_not_found_message("No such file: No such file"));
        assert!(!is_sftp_path_not_found_message("Permission denied"));
        assert!(!is_sftp_path_not_found_message("Timeout"));
    }

    #[test]
    fn ordinary_remote_exec_reports_bounded_input_hints_without_collecting_input() {
        assert_eq!(
            detect_remote_exec_input_kind("partial output\nPassword for ops: "),
            Some("secret")
        );
        assert_eq!(
            detect_remote_exec_input_kind("Proceed with installation? [y/N]"),
            Some("text")
        );
        assert_eq!(detect_remote_exec_input_kind("service started"), None);
    }

    #[test]
    fn empty_password_mode_is_explicit_and_backwards_compatible() {
        let unset_password = serde_json::json!({
            "authType": "password",
            "username": "ops"
        });
        assert_eq!(
            missing_password_credential(&unset_password),
            Some("missing-password")
        );
        assert_eq!(password_for_authentication(&unset_password), None);

        let empty_password = serde_json::json!({
            "authType": "password",
            "username": "ops",
            "password": "stale-password",
            "useEmptyPassword": true
        });
        assert_eq!(missing_password_credential(&empty_password), None);
        assert_eq!(password_for_authentication(&empty_password), Some(""));
    }

    #[test]
    fn resource_monitoring_uses_only_supported_intervals() {
        assert_eq!(
            resource_monitoring_interval_seconds(&serde_json::json!({})),
            1
        );
        assert_eq!(
            resource_monitoring_interval_seconds(&serde_json::json!({
                "resourceMonitoringIntervalSeconds": 30
            })),
            30
        );
        assert_eq!(
            resource_monitoring_interval_seconds(&serde_json::json!({
                "resourceMonitoringIntervalSeconds": 1
            })),
            1
        );
    }

    #[test]
    fn shell_cwd_setup_reuses_linux_hook_for_darwin() {
        // Regression for M1: macOS remotes must keep CWD + sudo tracking.
        // `darwin` reuses the Linux hook; `windows` / unknown fail closed.
        assert!(shell_cwd_setup_for_platform("linux").is_some());
        assert!(shell_cwd_setup_for_platform("darwin").is_some());
        assert_eq!(
            shell_cwd_setup_for_platform("darwin"),
            shell_cwd_setup_for_platform("linux")
        );
        assert!(shell_cwd_setup_for_platform("busybox").is_some());
        assert_ne!(
            shell_cwd_setup_for_platform("busybox"),
            shell_cwd_setup_for_platform("linux"),
        );
        assert!(shell_cwd_setup_for_platform("windows").is_none());
        assert!(shell_cwd_setup_for_platform("unknown").is_none());
    }

    #[test]
    fn ssh_initial_home_path_accepts_legacy_root_and_relative_home() {
        assert!(is_implicit_ssh_home_path(""));
        assert!(is_implicit_ssh_home_path("/"));
        assert!(is_implicit_ssh_home_path(" . "));
        assert!(!is_implicit_ssh_home_path("/srv/app"));
        assert!(!is_implicit_ssh_home_path("/volume2/photo"));
    }

    #[tokio::test]
    async fn dropped_tunnel_worker_rejects_queued_command() {
        let (tunnel_tx, tunnel_rx) = mpsc::unbounded_channel::<TunnelCommand>();
        drop(tunnel_rx);
        let (respond_to, response_rx) = oneshot::channel();

        enqueue_tunnel_command(&tunnel_tx, TunnelCommand::List { respond_to });

        assert_eq!(
            response_rx
                .await
                .expect("dropped tunnel worker must answer the caller")
                .expect_err("dropped tunnel worker must not report success"),
            "SSH tunnel worker stopped"
        );
    }

    #[test]
    fn trim_string_front_never_panics_on_multibyte_boundaries() {
        // 回归：热路径上的滚动 buffer 都含中文/U+FFFD（3 字节字符），
        // `s[len - keep..]` 直接切片落在字符内部会 panic 并无声杀死
        // worker/pump（终端冻结、Ctrl+C 失效）。裁剪后必须始终是合法 UTF-8。
        for fill in ["中文输出", "\u{FFFD}\u{FFFD}", "a中文b", "✓ 成功"] {
            for extra in 0..8 {
                let mut value = "x".repeat(extra) + &fill.repeat(1024);
                let original_len = value.len();
                trim_string_front(&mut value, 512);
                assert!(value.len() <= 512 || original_len <= 512);
                assert!(value.len() >= 512 - 3 || original_len <= 512);
            }
        }
        // keep 大于长度时不动；空字符串安全。
        let mut small = "abc中文".to_string();
        trim_string_front(&mut small, 1024);
        assert_eq!(small, "abc中文");
        let mut empty = String::new();
        trim_string_front(&mut empty, 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn rolling_buffers_survive_cjk_flood_without_panic() {
        // 回归：模拟高吞吐中文脚本输出冲刷 track_cwd_and_user 与
        // track_root_access_prompt_from_terminal 的滚动窗口。修复前窗口裁剪
        // 落在多字节字符内部直接 panic，SSH worker 任务随之死亡。
        let flood = "[ ✓ success ] 检查点 重建分区表 running\r\n".repeat(400);
        let mut cwd_buffer = String::new();
        let mut prompt_buffer = String::new();
        let mut awaiting = None;
        let mut pending = String::new();
        let mut sudo_password = None;
        let mut last_authenticated = None;
        let mut pending_command = None;
        for chunk in flood.as_bytes().chunks(97) {
            let text = String::from_utf8_lossy(chunk);
            let _ = track_cwd_and_user(&text, &mut cwd_buffer);
            let _ = track_root_access_prompt_from_terminal(
                &text,
                &mut prompt_buffer,
                &mut awaiting,
                &mut pending,
                &mut sudo_password,
                &mut last_authenticated,
                &mut pending_command,
            );
        }
        assert!(cwd_buffer.len() < 16384);
        assert!(prompt_buffer.len() < 4096);
    }

    #[tokio::test]
    async fn ssh_stage_timeout_is_reported_without_waiting_for_the_client_default() {
        let error = wait_for_ssh_stage(
            "SSH password authentication",
            Duration::from_millis(1),
            std::future::pending::<Result<(), String>>(),
        )
        .await
        .unwrap_err();

        assert_eq!(error, "SSH password authentication timed out after 1 ms");
    }

    #[tokio::test]
    async fn ssh_interaction_wait_stops_immediately_when_session_is_cancelled() {
        let cancellation = CancellationToken::new();
        let flow = SshInteractionFlow::new();
        let context = SshInteractionContext::from_profile(
            &flow,
            "tab-1",
            &serde_json::json!({"id": "profile-1", "name": "Test"}),
            "example.test",
            22,
            SshAuthenticationTarget::Direct,
            Some("main".to_string()),
            cancellation.clone(),
        );
        let (_sender, receiver) = oneshot::channel();
        cancellation.cancel();

        let result =
            wait_for_ssh_interaction(&context, receiver, Duration::from_secs(60)).await;

        assert!(matches!(result, SshInteractionWaitResult::Cancelled));
    }

    #[tokio::test]
    async fn host_key_confirmation_pauses_the_network_handshake_budget() {
        let host_verification_waiting = Arc::new(AtomicBool::new(true));
        let result = wait_for_ssh_handshake_with_timeouts(
            "SSH protocol handshake",
            host_verification_waiting,
            Duration::from_millis(50),
            Duration::from_millis(250),
            async {
                tokio::time::sleep(Duration::from_millis(120)).await;
                Ok::<_, String>(())
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn host_key_confirmation_keeps_its_own_bounded_timeout() {
        let host_verification_waiting = Arc::new(AtomicBool::new(true));
        let wait_flag = host_verification_waiting.clone();
        let error = wait_for_ssh_handshake_with_timeouts(
            "SSH protocol handshake",
            host_verification_waiting,
            Duration::from_millis(50),
            Duration::from_millis(50),
            async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                wait_flag.store(false, Ordering::Release);
                Ok::<_, String>(())
            },
        )
        .await
        .unwrap_err();

        assert_eq!(error, "SSH host-key verification timed out after 50 ms");
    }

    #[test]
    fn password_auth_requests_missing_credentials_without_falling_back_to_keys() {
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "username": "ops"
            })),
            Some("missing-password")
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "password": "secret"
            })),
            Some("missing-username")
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "password",
                "username": "ops",
                "password": "secret"
            })),
            None
        );
        assert_eq!(
            missing_password_credential(&serde_json::json!({
                "authType": "system",
                "username": "ops"
            })),
            None
        );
    }

    #[test]
    fn empty_trusted_host_fingerprint_is_not_treated_as_known() {
        assert_eq!(
            trusted_host_fingerprint(&serde_json::json!({
                "trustedHostFingerprint": ""
            })),
            None
        );
        assert_eq!(
            trusted_host_fingerprint(&serde_json::json!({
                "trustedHostFingerprint": "  \t"
            })),
            None
        );
        assert_eq!(
            trusted_host_fingerprint(&serde_json::json!({
                "trustedHostFingerprint": "SHA256:known-host-key"
            })),
            Some("SHA256:known-host-key".to_string())
        );
    }

    #[cfg(unix)]
    struct OpenSshFixture {
        root: std::path::PathBuf,
        remote_dir: std::path::PathBuf,
        client_key: std::path::PathBuf,
        port: u16,
        process: std::process::Child,
    }

    #[cfg(unix)]
    impl Drop for OpenSshFixture {
        fn drop(&mut self) {
            let _ = self.process.kill();
            let _ = self.process.wait();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(unix)]
    fn current_test_username() -> String {
        std::env::var("USER")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                String::from_utf8(
                    std::process::Command::new("id")
                        .arg("-un")
                        .output()
                        .expect("could not determine the current test user")
                        .stdout,
                )
                .expect("current test user was not UTF-8")
                .trim()
                .to_string()
            })
    }

    #[cfg(unix)]
    fn start_openssh_fixture() -> OpenSshFixture {
        const SSHD: &str = "/usr/sbin/sshd";
        const SSH_KEYGEN: &str = "/usr/bin/ssh-keygen";
        assert!(
            std::path::Path::new(SSHD).exists() && std::path::Path::new(SSH_KEYGEN).exists(),
            "real OpenSSH verification requires {SSHD} and {SSH_KEYGEN}"
        );

        let root =
            std::env::temp_dir().join(format!("fileterm-tauri-sshd-{}", uuid::Uuid::new_v4()));
        let remote_dir = root.join("remote");
        std::fs::create_dir_all(&remote_dir).unwrap();
        let host_key = root.join("host-key");
        let client_key = root.join("client-key");
        let authorized_keys = root.join("authorized_keys");
        for key in [&host_key, &client_key] {
            let result = std::process::Command::new(SSH_KEYGEN)
                .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                .arg(key)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        std::fs::copy(client_key.with_extension("pub"), &authorized_keys).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&authorized_keys, std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }

        let port_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = port_listener.local_addr().unwrap().port();
        drop(port_listener);
        let config = root.join("sshd_config");
        std::fs::write(
            &config,
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nChallengeResponseAuthentication no\nPubkeyAuthentication yes\nAllowTcpForwarding yes\nUsePAM no\nUseDNS no\nLogLevel ERROR\nSubsystem sftp internal-sftp\n",
                host_key.display(),
                root.join("sshd.pid").display(),
                authorized_keys.display(),
            ),
        )
        .unwrap();
        let process = std::process::Command::new(SSHD)
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        OpenSshFixture {
            root,
            remote_dir,
            client_key,
            port,
            process,
        }
    }

    #[cfg(unix)]
    async fn wait_for_openssh(port: u16) {
        for _ in 0..40 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("local OpenSSH fixture did not begin listening");
    }

    #[cfg(unix)]
    async fn read_http_headers(socket: &mut tokio::net::TcpStream) -> String {
        let mut headers = Vec::new();
        let mut byte = [0_u8; 1];
        while !headers.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = socket.read(&mut byte).await.unwrap();
            assert_eq!(
                count, 1,
                "proxy client closed before completing CONNECT headers"
            );
            headers.push(byte[0]);
        }
        String::from_utf8(headers).unwrap()
    }

    #[cfg(unix)]
    async fn read_socks5_connect_request(socket: &mut tokio::net::TcpStream) -> (String, u16) {
        let mut greeting = [0_u8; 2];
        socket.read_exact(&mut greeting).await.unwrap();
        assert_eq!(greeting[0], 5);
        let mut methods = vec![0_u8; greeting[1] as usize];
        socket.read_exact(&mut methods).await.unwrap();
        assert!(methods.contains(&0));
        socket.write_all(&[5, 0]).await.unwrap();

        let mut request = [0_u8; 4];
        socket.read_exact(&mut request).await.unwrap();
        assert_eq!(&request[..3], &[5, 1, 0]);
        let host = match request[3] {
            1 => {
                let mut address = [0_u8; 4];
                socket.read_exact(&mut address).await.unwrap();
                std::net::Ipv4Addr::from(address).to_string()
            }
            3 => {
                let mut length = [0_u8; 1];
                socket.read_exact(&mut length).await.unwrap();
                let mut hostname = vec![0_u8; length[0] as usize];
                socket.read_exact(&mut hostname).await.unwrap();
                String::from_utf8(hostname).unwrap()
            }
            other => panic!("unexpected SOCKS5 address type: {other}"),
        };
        let mut port = [0_u8; 2];
        socket.read_exact(&mut port).await.unwrap();
        (host, u16::from_be_bytes(port))
    }

    #[cfg(unix)]
    async fn authenticate_openssh_fixture(
        fixture: &OpenSshFixture,
        profile: &serde_json::Value,
    ) -> client::Handle<AcceptTestServerKey> {
        let stream = super::connect_ssh_transport(profile, "127.0.0.1", fixture.port)
            .await
            .unwrap();
        let mut handle = client::connect_stream(
            Arc::new(client::Config::default()),
            stream,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let key = russh::keys::decode_secret_key(
            &std::fs::read_to_string(&fixture.client_key).unwrap(),
            None,
        )
        .unwrap();
        let authenticated = handle
            .authenticate_publickey(
                current_test_username(),
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None),
            )
            .await
            .unwrap();
        assert!(authenticated.success());
        handle
    }

    struct AcceptTestServerKey;

    impl client::Handler for AcceptTestServerKey {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    #[derive(Clone)]
    struct NetworkDeviceProtocolState {
        pty: Arc<Mutex<Option<(String, u32, u32)>>>,
        resize: Arc<Mutex<Option<(u32, u32)>>>,
        input: Arc<Mutex<Vec<Vec<u8>>>>,
        exec_requests: Arc<AtomicUsize>,
        subsystem_requests: Arc<AtomicUsize>,
    }

    impl NetworkDeviceProtocolState {
        fn new() -> Self {
            Self {
                pty: Arc::new(Mutex::new(None)),
                resize: Arc::new(Mutex::new(None)),
                input: Arc::new(Mutex::new(Vec::new())),
                exec_requests: Arc::new(AtomicUsize::new(0)),
                subsystem_requests: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    struct NetworkDeviceProtocolServer {
        state: NetworkDeviceProtocolState,
    }

    impl server::Handler for NetworkDeviceProtocolServer {
        type Error = russh::Error;

        async fn auth_publickey(
            &mut self,
            _user: &str,
            _key: &russh::keys::ssh_key::PublicKey,
        ) -> Result<server::Auth, Self::Error> {
            Ok(server::Auth::Accept)
        }

        async fn auth_password(
            &mut self,
            _user: &str,
            _password: &str,
        ) -> Result<server::Auth, Self::Error> {
            Ok(server::Auth::Accept)
        }

        async fn channel_open_session(
            &mut self,
            _channel: russh::Channel<server::Msg>,
            reply: server::ChannelOpenHandle,
            _session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            reply.accept().await;
            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        async fn pty_request(
            &mut self,
            channel: russh::ChannelId,
            term: &str,
            col_width: u32,
            row_height: u32,
            _pix_width: u32,
            _pix_height: u32,
            _modes: &[(russh::Pty, u32)],
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            *self.state.pty.lock().unwrap() = Some((term.to_string(), col_width, row_height));
            session.channel_success(channel)?;
            Ok(())
        }

        async fn shell_request(
            &mut self,
            channel: russh::ChannelId,
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            session.channel_success(channel)?;
            session.data(channel, b"router# ".to_vec())?;
            Ok(())
        }

        async fn data(
            &mut self,
            channel: russh::ChannelId,
            data: &[u8],
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            self.state.input.lock().unwrap().push(data.to_vec());
            if data == b"show version\r" {
                session.data(channel, b"\r\nmock-router\r\nrouter# ".to_vec())?;
            } else {
                session.data(channel, b"router# ".to_vec())?;
            }
            Ok(())
        }

        async fn exec_request(
            &mut self,
            channel: russh::ChannelId,
            _data: &[u8],
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            self.state.exec_requests.fetch_add(1, Ordering::Relaxed);
            session.channel_failure(channel)?;
            Ok(())
        }

        async fn subsystem_request(
            &mut self,
            channel: russh::ChannelId,
            _name: &str,
            session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            self.state
                .subsystem_requests
                .fetch_add(1, Ordering::Relaxed);
            session.channel_failure(channel)?;
            Ok(())
        }

        #[allow(clippy::too_many_arguments)]
        async fn window_change_request(
            &mut self,
            _channel: russh::ChannelId,
            col_width: u32,
            row_height: u32,
            _pix_width: u32,
            _pix_height: u32,
            _session: &mut server::Session,
        ) -> Result<(), Self::Error> {
            *self.state.resize.lock().unwrap() = Some((col_width, row_height));
            Ok(())
        }
    }

    struct CaptureRemoteSshId {
        remote_sshid: Arc<Mutex<Vec<u8>>>,
    }

    impl client::Handler for CaptureRemoteSshId {
        type Error = russh::Error;

        async fn check_server_key(
            &mut self,
            _server_public_key: &russh::keys::PublicKeyOrCertificate,
        ) -> Result<bool, Self::Error> {
            Ok(true)
        }

        async fn kex_done(
            &mut self,
            _shared_secret: Option<&[u8]>,
            _names: &russh::Names,
            session: &mut russh::client::Session,
        ) -> Result<(), Self::Error> {
            *self.remote_sshid.lock().unwrap() = session.remote_sshid().to_vec();
            Ok(())
        }
    }

    async fn wait_for_channel_text(
        channel: &mut russh::Channel<client::Msg>,
        needle: &str,
    ) -> Vec<u8> {
        let mut output = Vec::new();
        loop {
            let message = timeout(Duration::from_secs(2), channel.wait())
                .await
                .unwrap()
                .unwrap();
            match message {
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    output.extend_from_slice(data.as_ref());
                    if String::from_utf8_lossy(&output).contains(needle) {
                        return output;
                    }
                }
                ChannelMsg::Close => panic!("network device fixture closed the terminal channel"),
                _ => {}
            }
        }
    }

    #[tokio::test]
    async fn network_device_protocol_mock_keeps_raw_pty_independent_of_optional_channels() {
        let state = NetworkDeviceProtocolState::new();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut server_config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        server_config.server_id = russh::SshId::Standard(Cow::Borrowed("SSH-2.0-Comware-7.1"));
        // Force the legacy GEX algorithm so this fixture exercises the same
        // handshake branch used by older Comware peers.
        server_config.preferred.kex = Cow::Owned(vec![russh::kex::DH_GEX_SHA1]);
        server_config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
        );
        let server_state = state.clone();
        let server_task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(server_config),
                socket,
                NetworkDeviceProtocolServer {
                    state: server_state,
                },
            )
            .await
            .unwrap();
            let _ = running.await;
        });

        let remote_sshid = Arc::new(Mutex::new(Vec::new()));
        let client_config = client::Config {
            preferred: build_legacy_preferred(),
            comware_legacy_gex: true,
            ..Default::default()
        };
        let mut handle = client::connect(
            Arc::new(client_config),
            address,
            CaptureRemoteSshId {
                remote_sshid: remote_sshid.clone(),
            },
        )
        .await
        .unwrap();
        let key =
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap();
        let authenticated = handle
            .authenticate_publickey(
                "fixture",
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None),
            )
            .await
            .unwrap();
        assert!(authenticated.success());

        let identification = normalize_ssh_identification(&remote_sshid.lock().unwrap());
        assert_eq!(identification, "SSH-2.0-Comware-7.1");
        let profile = serde_json::json!({
            "type": "ssh",
            "deviceMode": "auto",
            "terminalType": "vt100",
            "enableExecChannel": true,
            "enableResourceMonitoring": true,
            "sftpEnabled": true
        });
        let resolution = resolve_ssh_device_mode(&profile, identification.as_bytes());
        assert_eq!(resolution.mode, ResolvedSshDeviceMode::NetworkDevice);
        assert_eq!(resolution.source, "banner");
        assert_eq!(resolution.family, Some("h3c-comware"));
        let effective_profile = profile_with_resolved_device_mode(&profile, resolution);
        assert_eq!(effective_profile["deviceMode"], "network-device");
        assert!(!effective_exec_channel_enabled(&effective_profile));
        assert!(!effective_resource_monitoring_enabled(&effective_profile));
        assert!(!effective_sftp_enabled(&effective_profile));

        let mut shell = handle.channel_open_session().await.unwrap();
        shell
            .request_pty(true, "vt100", 80, 24, 0, 0, &[])
            .await
            .unwrap();
        shell.request_shell(true).await.unwrap();
        wait_for_channel_text(&mut shell, "router# ").await;
        assert_eq!(
            state.pty.lock().unwrap().clone(),
            Some(("vt100".to_string(), 80, 24))
        );

        shell.data_bytes(&b"show version\r"[..]).await.unwrap();
        let output = wait_for_channel_text(&mut shell, "mock-router").await;
        assert!(String::from_utf8_lossy(&output).contains("router# "));
        assert_eq!(
            state.input.lock().unwrap().as_slice(),
            [b"show version\r".as_slice()]
        );

        shell.window_change(132, 40, 0, 0).await.unwrap();
        for _ in 0..50 {
            if *state.resize.lock().unwrap() == Some((132, 40)) {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*state.resize.lock().unwrap(), Some((132, 40)));

        let exec_channel = handle.channel_open_session().await.unwrap();
        exec_channel.exec(true, "uname -a").await.unwrap();
        let subsystem_channel = handle.channel_open_session().await.unwrap();
        subsystem_channel
            .request_subsystem(true, "sftp")
            .await
            .unwrap();
        for _ in 0..50 {
            if state.exec_requests.load(Ordering::Relaxed) == 1
                && state.subsystem_requests.load(Ordering::Relaxed) == 1
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(state.exec_requests.load(Ordering::Relaxed), 1);
        assert_eq!(state.subsystem_requests.load(Ordering::Relaxed), 1);

        shell.data_bytes(&b"\r"[..]).await.unwrap();
        wait_for_channel_text(&mut shell, "router# ").await;

        drop(subsystem_channel);
        drop(exec_channel);
        drop(shell);
        drop(handle);
        timeout(Duration::from_secs(2), server_task)
            .await
            .expect("network device protocol fixture did not stop")
            .unwrap();
    }

    struct KeyboardInteractiveMfaServer {
        responses: Arc<Mutex<Vec<String>>>,
    }

    impl server::Handler for KeyboardInteractiveMfaServer {
        type Error = russh::Error;

        async fn auth_keyboard_interactive<'a>(
            &'a mut self,
            _user: &str,
            _submethods: &str,
            response: Option<server::Response<'a>>,
        ) -> Result<server::Auth, Self::Error> {
            if let Some(response) = response {
                let received = response
                    .map(|answer| String::from_utf8_lossy(&answer).into_owned())
                    .collect::<Vec<_>>();
                *self.responses.lock().unwrap() = received.clone();
                return Ok(if received == ["saved-password", "246810"] {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                });
            }
            Ok(server::Auth::Partial {
                name: Cow::Borrowed("FileTerm MFA fixture"),
                instructions: Cow::Borrowed("Enter password and second factor"),
                prompts: Cow::Owned(vec![
                    (Cow::Borrowed("Password: "), false),
                    (Cow::Borrowed("OTP code: "), false),
                ]),
            })
        }
    }

    struct PasswordThenKeyboardInteractiveMfaServer {
        password_attempts: Arc<AtomicUsize>,
        responses: Arc<Mutex<Vec<String>>>,
    }

    impl server::Handler for PasswordThenKeyboardInteractiveMfaServer {
        type Error = russh::Error;

        async fn auth_password(
            &mut self,
            _user: &str,
            password: &str,
        ) -> Result<server::Auth, Self::Error> {
            self.password_attempts.fetch_add(1, Ordering::Relaxed);
            Ok(if password == "saved-password" {
                server::Auth::Reject {
                    proceed_with_methods: Some(MethodSet::from(
                        &[MethodKind::KeyboardInteractive][..],
                    )),
                    partial_success: true,
                }
            } else {
                server::Auth::reject()
            })
        }

        async fn auth_keyboard_interactive<'a>(
            &'a mut self,
            _user: &str,
            _submethods: &str,
            response: Option<server::Response<'a>>,
        ) -> Result<server::Auth, Self::Error> {
            if let Some(response) = response {
                let received = response
                    .map(|answer| String::from_utf8_lossy(&answer).into_owned())
                    .collect::<Vec<_>>();
                *self.responses.lock().unwrap() = received.clone();
                return Ok(if received == ["246810"] {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                });
            }
            Ok(server::Auth::Partial {
                name: Cow::Borrowed("FileTerm second factor"),
                instructions: Cow::Borrowed("Enter the one-time code"),
                prompts: Cow::Owned(vec![(Cow::Borrowed("OTP code: "), false)]),
            })
        }
    }

    struct MultiRoundKeyboardInteractiveServer {
        challenge_round: AtomicUsize,
        responses: Arc<Mutex<Vec<String>>>,
    }

    impl server::Handler for MultiRoundKeyboardInteractiveServer {
        type Error = russh::Error;

        async fn auth_keyboard_interactive<'a>(
            &'a mut self,
            _user: &str,
            _submethods: &str,
            response: Option<server::Response<'a>>,
        ) -> Result<server::Auth, Self::Error> {
            if let Some(response) = response {
                let received = response
                    .map(|answer| String::from_utf8_lossy(&answer).into_owned())
                    .collect::<Vec<_>>();
                self.responses
                    .lock()
                    .unwrap()
                    .extend(received.iter().cloned());
                return Ok(if received == ["111111"] {
                    server::Auth::Reject {
                        proceed_with_methods: Some(MethodSet::from(
                            &[MethodKind::KeyboardInteractive][..],
                        )),
                        partial_success: true,
                    }
                } else if received == ["222222"] {
                    server::Auth::Accept
                } else {
                    server::Auth::reject()
                });
            }

            let round = self.challenge_round.fetch_add(1, Ordering::Relaxed) + 1;
            let prompt = match round {
                1 => "First OTP code: ",
                // Deliberately reuse a password-like label for a later factor
                // to ensure the saved primary secret is not leaked into it.
                2 => "Password: ",
                _ => return Ok(server::Auth::reject()),
            };
            Ok(server::Auth::Partial {
                name: Cow::Borrowed("FileTerm multi-round MFA fixture"),
                instructions: Cow::Borrowed("Enter both one-time codes"),
                prompts: Cow::Owned(vec![(Cow::Borrowed(prompt), false)]),
            })
        }
    }

    #[test]
    fn suppresses_fragmented_cwd_setup_echo_after_its_marker_settles() {
        let mut pending = Some(ShellSetupEchoSuppression::new(true));

        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                "Debian GNU/Linux\r\nuser@host:~$ test -z \"${FISH_VERSION-}\" && eval '__tdcwd(){ printf"
            ),
            ""
        );

        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " '\\033]7;file:///home/user\\007'; }; __tdcwd\r\n\u{1b}]7;file:///home/user\u{7}\u{1b}]7777;FileTermReady\u{7}user@host:~$ ",
            ),
            ""
        );

        pending.as_mut().unwrap().marker_seen_at = Some(Instant::now() - SHELL_SETUP_SETTLE_DELAY);
        let visible = suppress_shell_setup_echo(&mut pending, "root@host:~# ");

        assert_eq!(visible, "Debian GNU/Linux\r\nuser@host:~$ root@host:~# ");
        assert!(pending.is_none());
    }

    #[test]
    fn detects_common_posix_prompts_after_terminal_colours_are_removed() {
        assert!(looks_like_shell_prompt(
            "\u{1b}[01;32mStoffel@fnOSNAS-CN\u{1b}[0m:\u{1b}[01;34m/\u{1b}[0m$ "
        ));
        assert!(looks_like_shell_prompt("root@host:~# "));
        assert!(looks_like_shell_prompt("host% "));
        assert!(!looks_like_shell_prompt("Last login: today\r\n"));
    }

    #[test]
    fn buffers_literal_hash_until_shell_setup_is_complete() {
        assert!(should_buffer_terminal_input_during_shell_setup(
            true, false, "#"
        ));
        assert!(should_buffer_terminal_input_during_shell_setup(
            false,
            true,
            "echo ready\r"
        ));
        assert!(!should_buffer_terminal_input_during_shell_setup(
            true, false, "\u{3}"
        ));
        assert!(!should_buffer_terminal_input_during_shell_setup(
            false, false, "#"
        ));
    }

    #[test]
    fn shell_setup_uses_a_private_ready_marker_and_cleans_bash_history() {
        assert!(SHELL_CWD_SETUP.contains("7777;FileTermReady"));
        assert!(SHELL_CWD_SETUP.contains("history -d"));
        assert!(SHELL_CWD_SETUP.contains("__FILETERM_INTERNAL_SETUP_1"));
        assert!(BUSYBOX_SHELL_CWD_SETUP.contains("7777;FileTermReady"));
    }

    #[test]
    fn detects_fragmented_fn_os_prompt_and_cwd_marker() {
        let prompt = concat!(
            "Linux fnOSNAS-CN 6.18.18-trim\r\n",
            "Stoffel@fnOSNAS-CN:",
            "/$ "
        );
        assert!(looks_like_shell_prompt(prompt));

        let mut buffer = String::new();
        assert_eq!(
            track_cwd_and_user("\u{1b}]7;file:///e", &mut buffer),
            (None, None)
        );
        assert_eq!(
            track_cwd_and_user("tc\u{7}\u{1b}]1337;RemoteUser=Stoffel\u{7}", &mut buffer),
            (Some("/etc".to_string()), Some("Stoffel".to_string()))
        );
        assert_eq!(
            track_cwd_and_user("root@host:~# ", &mut buffer),
            (None, None)
        );
    }

    #[test]
    fn detects_root_prompt_after_terminal_colours_are_removed() {
        assert!(looks_like_root_prompt("\u{1b}[01;31mroot@host\u{1b}[0m:# "));
        assert!(!looks_like_root_prompt("user@host:$ "));
    }

    #[test]
    fn literal_hash_does_not_trigger_root_shell_setup_without_transition() {
        assert!(!should_reinject_root_shell_setup(
            true, false, false, false, false, "#"
        ));
        assert!(should_reinject_root_shell_setup(
            true,
            false,
            false,
            true,
            false,
            "root@host:~# "
        ));
        assert!(!should_reinject_root_shell_setup(
            true,
            false,
            false,
            true,
            true,
            "root@host:~# "
        ));
    }

    #[test]
    fn suppress_releases_new_prompt_after_marker_on_slow_device() {
        // 慢设备（群晖）：ready marker 后新 prompt 在 settle delay 之后才到达。
        // 第一个 prompt 已被 split_prompt_tail_for_setup_wait 暂存（不 forward），
        // 所以 suppress 释放时只返回新 prompt（最后一个换行符之后的部分），
        // 吞掉 setup echo 和 ready marker。用户最终看到一个完整 prompt。
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        // 喂入 setup echo + ready marker，suppress 仍在等待新 prompt
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " __tdcwd(){ printf '\\033]7;file:///home/u\\007';};__tdcwd\r\n\u{1b}]7;file:///home/u\u{7}\u{1b}]7777;FileTermReady\u{7}"
            ),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_some());
        // 新 prompt 到达（无论 settle delay 是否到期）：只释放新 prompt
        let visible = suppress_shell_setup_echo(&mut pending, "user@host:~$ ");
        assert_eq!(visible, "user@host:~$ ");
        assert!(pending.is_none());
    }

    #[test]
    fn finish_suppression_releases_newline_when_prompt_never_arrives() {
        // ready marker 已看到但新 prompt 迟迟未到（settle/timeout 到期）：
        // 补换行让晚到的新 prompt 从新行开始，避免粘在旧 prompt 后面。
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        // 喂入 setup echo + ready marker，但新 prompt 一直没来
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " __tdcwd(){ printf '\\033]7;file:///home/u\\007';};__tdcwd\r\n\u{1b}]7;file:///home/u\u{7}\u{1b}]7777;FileTermReady\u{7}"
            ),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_some());
        // 超时释放时 buffer 末尾不是 prompt，补换行
        let visible = finish_shell_setup_suppression(&mut pending);
        assert_eq!(visible, "\r\n");
        assert!(pending.is_none());
    }

    #[test]
    fn finish_suppression_no_newline_when_marker_never_seen() {
        // setup 执行失败（没检测到 OSC marker）时不补换行，避免多余的空行
        let mut pending = Some(ShellSetupEchoSuppression::new(false));
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, " __tdcwd(){ broken syntax"),
            ""
        );
        assert!(pending.as_ref().unwrap().marker_seen_at.is_none());
        // 超时释放时不补换行
        let visible = finish_shell_setup_suppression(&mut pending);
        assert_eq!(visible, "");
        assert!(pending.is_none());
    }

    #[test]
    fn root_setup_suppression_restores_original_prompt_when_injection_fails() {
        let mut pending = Some(ShellSetupEchoSuppression::with_fallback(
            "root@host:~# ".to_string(),
        ));
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, "setup command rejected"),
            ""
        );
        assert_eq!(
            finish_shell_setup_suppression(&mut pending),
            "root@host:~# "
        );
    }

    #[test]
    fn root_setup_suppression_discards_original_prompt_after_new_prompt() {
        let mut pending = Some(ShellSetupEchoSuppression::with_fallback(
            "root@host:~# ".to_string(),
        ));
        assert_eq!(
            suppress_shell_setup_echo(
                &mut pending,
                " __tdcwd(){ printf '\\033]7;file:///root\\007';};__tdcwd\r\n\u{1b}]7;file:///root\u{7}\u{1b}]7777;FileTermReady\u{7}"
            ),
            ""
        );
        assert_eq!(
            suppress_shell_setup_echo(&mut pending, "root@host:~# "),
            "root@host:~# "
        );
    }

    #[test]
    fn split_prompt_tail_separates_banner_from_prompt() {
        // banner + prompt 在同一 chunk：banner forward，prompt 暂存
        let (banner, tail) =
            split_prompt_tail_for_setup_wait("Welcome to Synology\r\nStoffel@SynologyNAS-MY:~$ ");
        assert_eq!(banner, "Welcome to Synology\r\n");
        assert_eq!(tail, "Stoffel@SynologyNAS-MY:~$ ");
    }

    #[test]
    fn split_prompt_tail_keeps_colored_prompt_escape_in_tail() {
        // 彩色 prompt 的 escape 序列划入 tail（不 forward），banner 部分保留原始 escape
        let (banner, tail) = split_prompt_tail_for_setup_wait(
            "\u{1b}[01;32mStoffel@SynologyNAS-MY\u{1b}[0m:\u{1b}[01;34m~\u{1b}[0m$ ",
        );
        assert_eq!(banner, "");
        assert_eq!(
            tail,
            "\u{1b}[01;32mStoffel@SynologyNAS-MY\u{1b}[0m:\u{1b}[01;34m~\u{1b}[0m$ "
        );
    }

    #[test]
    fn split_prompt_tail_returns_whole_chunk_when_no_prompt() {
        // 纯 banner（无 prompt 结尾符）：整个 chunk forward
        let (banner, tail) = split_prompt_tail_for_setup_wait(
            "Using terminal commands to modify system configs\r\n",
        );
        assert_eq!(
            banner,
            "Using terminal commands to modify system configs\r\n"
        );
        assert_eq!(tail, "");
    }

    #[test]
    fn split_prompt_tail_stops_at_newline_when_scanning_backwards() {
        // prompt 结尾符不在最后一行（最后一行是 banner 续行）：整个 chunk forward
        let (banner, tail) = split_prompt_tail_for_setup_wait("some $ var\r\nbanner continuation");
        assert_eq!(banner, "some $ var\r\nbanner continuation");
        assert_eq!(tail, "");
    }

    #[test]
    fn shell_identity_controls_file_access_independently_of_cached_sudo_auth() {
        assert_eq!(
            resolve_shell_file_access("stoffel", "root"),
            ("root", Some("root".to_string()))
        );
        assert_eq!(
            resolve_shell_file_access("stoffel", "postgres"),
            ("root", Some("postgres".to_string()))
        );
        assert_eq!(
            resolve_shell_file_access("stoffel", "stoffel"),
            ("user", None)
        );
    }

    #[test]
    fn parses_file_manager_root_access_methods_with_sudo_compatibility() {
        assert_eq!(
            parse_root_file_access_method(None),
            Ok(RootFileAccessMethod::Sudo)
        );
        assert_eq!(
            parse_root_file_access_method(Some("sudo")),
            Ok(RootFileAccessMethod::Sudo)
        );
        assert_eq!(
            parse_root_file_access_method(Some("su")),
            Ok(RootFileAccessMethod::Su)
        );
        assert!(parse_root_file_access_method(Some("doas")).is_err());
    }

    #[test]
    fn selects_separate_saved_passwords_for_sudo_and_su() {
        let sudo_password = Some("sudo-secret".to_string());
        let su_password = Some("su-secret".to_string());

        assert_eq!(
            super::root_password_for_method(
                RootFileAccessMethod::Sudo,
                &sudo_password,
                &su_password,
            )
            .as_deref(),
            Some("sudo-secret")
        );
        assert_eq!(
            super::root_password_for_method(
                RootFileAccessMethod::Su,
                &sudo_password,
                &su_password,
            )
            .as_deref(),
            Some("su-secret")
        );
    }

    #[test]
    fn su_method_is_synced_when_file_pane_is_already_in_root_mode() {
        let authenticated = super::PendingRootAccessAuth {
            method: RootFileAccessMethod::Su,
            target_user: "root".to_string(),
            interactive_shell: true,
        };
        let method = super::root_access_method_for_shell_user("root", Some(&authenticated), None);

        assert_eq!(method, RootFileAccessMethod::Su);
        assert_ne!(RootFileAccessMethod::Sudo, method);
    }

    #[test]
    fn latest_privilege_command_wins_over_an_older_authenticated_method() {
        let old_sudo = super::PendingRootAccessAuth {
            method: RootFileAccessMethod::Sudo,
            target_user: "root".to_string(),
            interactive_shell: true,
        };
        let latest_su = super::PendingRootAccessAuth {
            method: RootFileAccessMethod::Su,
            target_user: "root".to_string(),
            interactive_shell: true,
        };

        assert_eq!(
            super::root_access_method_for_shell_user("root", Some(&old_sudo), Some(&latest_su)),
            RootFileAccessMethod::Su
        );

        let noninteractive_sudo = super::PendingRootAccessAuth {
            method: RootFileAccessMethod::Sudo,
            target_user: "root".to_string(),
            interactive_shell: false,
        };
        assert_eq!(
            super::root_access_method_for_shell_user(
                "root",
                Some(&latest_su),
                Some(&noninteractive_sudo),
            ),
            RootFileAccessMethod::Su
        );
    }

    #[test]
    fn terminal_sudo_password_cache_is_cleared_after_auth_failure() {
        let mut prompt_buffer = String::new();
        let mut awaiting = None;
        let mut pending = String::new();
        let mut recent = String::new();
        let mut cached = None;
        let mut last_authenticated = None;
        let mut pending_command = None;

        assert!(!capture_root_access_password_input(
            "sudo -i\r",
            &mut awaiting,
            &mut pending,
            &mut recent,
            &mut cached,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert!(!track_root_access_prompt_from_terminal(
            "[sudo] user 的密码：",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending,
            &mut cached,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert_eq!(
            awaiting.as_ref().map(|auth| auth.method),
            Some(RootFileAccessMethod::Sudo)
        );
        assert!(capture_root_access_password_input(
            "wrong\r",
            &mut awaiting,
            &mut pending,
            &mut recent,
            &mut cached,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert_eq!(cached.as_deref(), Some("wrong"));
        assert!(track_root_access_prompt_from_terminal(
            "Sorry, try again.\r\n",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending,
            &mut cached,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert!(cached.is_none());
        assert!(awaiting.is_none());
    }

    #[test]
    fn recognizes_localized_root_authentication_failures() {
        assert!(root_access_auth_failed("su: 身份验证失败"));
        assert!(root_access_auth_failed("sudo: 密码不正确"));
        assert!(!root_access_auth_failed("root@debian:~# "));
    }

    #[test]
    fn recognizes_su_and_sudo_transitions_from_terminal_input() {
        assert_eq!(
            privilege_command_from_terminal_input("su -\r"),
            Some(super::PendingRootAccessAuth {
                method: RootFileAccessMethod::Su,
                target_user: "root".to_string(),
                interactive_shell: true,
            })
        );
        assert_eq!(
            privilege_command_from_terminal_input("sudo -u postgres -i\r"),
            Some(super::PendingRootAccessAuth {
                method: RootFileAccessMethod::Sudo,
                target_user: "postgres".to_string(),
                interactive_shell: true,
            })
        );
        assert_eq!(
            privilege_command_from_terminal_input("sudo -u postgres cat /etc/hosts\r")
                .map(|auth| auth.interactive_shell),
            Some(false)
        );
        assert_eq!(
            privilege_command_from_terminal_input("sudo -l\r").map(|auth| auth.interactive_shell),
            Some(false)
        );
        assert_eq!(
            privilege_command_from_terminal_input("echo password\r"),
            None
        );
    }

    #[test]
    fn captures_su_password_and_reuses_su_for_file_commands() {
        let mut prompt_buffer = String::new();
        let mut awaiting = None;
        let mut pending_password = String::new();
        let mut recent_input = String::new();
        let mut cached_password = None;
        let mut last_authenticated = None;
        let mut pending_command = None;

        assert!(!capture_root_access_password_input(
            "su -\r",
            &mut awaiting,
            &mut pending_password,
            &mut recent_input,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert!(!track_root_access_prompt_from_terminal(
            "Password: ",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending_password,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert!(capture_root_access_password_input(
            "root-password\r",
            &mut awaiting,
            &mut pending_password,
            &mut recent_input,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert_eq!(
            last_authenticated.as_ref().map(|auth| auth.method),
            Some(RootFileAccessMethod::Su)
        );

        let (command, password) = root_file_command(
            RootFileAccessMethod::Su,
            &Some("root".to_string()),
            &cached_password,
            "touch /etc/fileterm-test",
        );
        assert!(command.starts_with("su -s /bin/sh -c "));
        assert!(!command.contains("sudo"));
        assert_eq!(password.as_deref(), Some("root-password"));
    }

    #[test]
    fn preserves_su_method_when_password_input_arrives_before_prompt_output() {
        let mut awaiting = None;
        let mut pending_password = String::new();
        let mut recent_input = String::new();
        let mut cached_password = None;
        let mut last_authenticated = None;
        let mut pending_command = None;

        assert!(!capture_root_access_password_input(
            "su -\r",
            &mut awaiting,
            &mut pending_password,
            &mut recent_input,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        // The SSH shell may echo the password prompt after the frontend has
        // already forwarded the password line. Ordinary input must not erase
        // the command that established the root shell.
        assert!(!capture_root_access_password_input(
            "root-password\r",
            &mut awaiting,
            &mut pending_password,
            &mut recent_input,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert_eq!(
            pending_command.as_ref().map(|auth| auth.method),
            Some(RootFileAccessMethod::Su)
        );
        assert!(last_authenticated.is_none());

        let mut prompt_buffer = String::new();
        assert!(track_root_access_prompt_from_terminal(
            "密码：",
            &mut prompt_buffer,
            &mut awaiting,
            &mut pending_password,
            &mut cached_password,
            &mut last_authenticated,
            &mut pending_command,
        ));
        assert_eq!(
            last_authenticated.as_ref().map(|auth| auth.method),
            Some(RootFileAccessMethod::Su)
        );
        assert_eq!(cached_password.as_deref(), Some("root-password"));
    }

    #[test]
    fn root_upload_command_preserves_resume_offset_and_target_parent() {
        assert_eq!(
            root_upload_shell_command("/etc/fileterm/config.toml", 0),
            "set -e\nmkdir -p '/etc/fileterm'\ncat > '/etc/fileterm/config.toml'"
        );
        assert_eq!(
            root_upload_shell_command("/etc/fileterm/config.toml", 12),
            "set -e\nmkdir -p '/etc/fileterm'\ncat >> '/etc/fileterm/config.toml'"
        );
    }

    #[test]
    fn root_stat_treats_a_missing_partial_as_an_empty_checkpoint() {
        assert_eq!(
            root_stat_shell_command("/opt/applications/墙纸.JPG.fileterm-part"),
            "if [ -e '/opt/applications/墙纸.JPG.fileterm-part' ] && [ ! -d '/opt/applications/墙纸.JPG.fileterm-part' ]; then stat -c '%s|%Y' -- '/opt/applications/墙纸.JPG.fileterm-part'; fi"
        );
    }

    #[test]
    fn root_upload_staging_is_transferred_through_login_sftp() {
        assert!(is_root_upload_staging_path(
            "/var/tmp/fileterm-root-upload-a57f-example.part"
        ));
        assert!(is_root_upload_staging_path(
            "/tmp/fileterm-root-upload-a57f-example.part"
        ));
        assert!(!is_root_upload_staging_path(
            "/opt/applications/墙纸.JPG.fileterm-part"
        ));
    }

    #[test]
    fn su_pty_streaming_commands_frame_output_and_use_base64() {
        let command = su_exec_command("base64 -d > '/etc/fileterm/config.toml'");
        assert!(command.contains(SU_EXEC_OUTPUT_MARKER));
        assert_eq!(
            strip_su_exec_output(&format!(
                "Password: \r\n{SU_EXEC_OUTPUT_MARKER}\r\n12|34\r\n"
            ))
            .as_deref(),
            Ok("12|34\n")
        );
        assert_eq!(
            root_upload_base64_shell_command("/etc/fileterm/config.toml", 0),
            "set -e\nmkdir -p '/etc/fileterm'\nbase64 -d > '/etc/fileterm/config.toml'"
        );
        assert_eq!(
            root_upload_base64_shell_command("/etc/fileterm/config.toml", 12),
            "set -e\nmkdir -p '/etc/fileterm'\nbase64 -d >> '/etc/fileterm/config.toml'"
        );
    }

    #[test]
    fn accepts_complete_root_download_without_ssh_exit_status() {
        assert_eq!(validate_root_download_completion(None, 14, 14), Ok(()));
        assert_eq!(validate_root_download_completion(Some(0), 14, 14), Ok(()));
    }

    #[test]
    fn rejects_incomplete_or_failed_root_download() {
        assert_eq!(
            validate_root_download_completion(None, 13, 14),
            Err("root 下载未完成（13/14 bytes）".to_string())
        );
        assert_eq!(
            validate_root_download_completion(Some(1), 14, 14),
            Err("root 下载命令失败（exit=1）".to_string())
        );
    }

    #[test]
    fn coalesces_high_frequency_terminal_input_without_losing_order() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        sender.send("clear\r".to_string()).unwrap();
        for _ in 0..2_000 {
            sender.send("\r".to_string()).unwrap();
        }

        let first = receiver.try_recv().unwrap();
        let merged = coalesce_terminal_input(first, &mut receiver);
        assert!(merged.starts_with("clear\r"));
        assert_eq!(merged.matches('\r').count(), 2_001);
        assert!(receiver.is_empty());
    }

    #[test]
    fn detects_ctrl_c_without_matching_other_control_bytes() {
        assert!(contains_interrupt_byte("build\r\u{3}"));
        assert!(!contains_interrupt_byte("build\r"));
        assert!(!contains_interrupt_byte("\u{1b}[2J"));
    }

    #[test]
    fn creates_parent_rows_only_below_remote_root() {
        assert_eq!(parent_remote_path("/"), None);
        assert_eq!(parent_remote_path("/home"), Some("/".to_string()));
        assert_eq!(
            parent_remote_path("/home/stoffel/下载/"),
            Some("/home/stoffel".to_string())
        );
        assert!(parent_remote_item("/").is_none());
        assert_eq!(parent_remote_item("/root").unwrap()["path"], "/");
        assert_eq!(parent_remote_item("/root").unwrap()["name"], "..");
    }

    #[test]
    fn default_ssh_key_candidates_match_electron_precedence() {
        let home = Path::new("/home/fileterm");
        assert_eq!(
            default_ssh_key_paths(home),
            vec![
                home.join(".ssh/id_ed25519"),
                home.join(".ssh/id_ecdsa"),
                home.join(".ssh/id_rsa"),
                home.join(".ssh/id_dsa"),
            ]
        );
    }

    #[test]
    fn builds_authenticated_http_connect_request_with_ipv6_authority() {
        let request = String::from_utf8(
            build_http_connect_request("2001:db8::1", 22, "alice", "secret").unwrap(),
        )
        .unwrap();

        assert!(request.starts_with("CONNECT [2001:db8::1]:22 HTTP/1.1\r\n"));
        assert!(request.contains("Host: [2001:db8::1]:22\r\n"));
        assert!(request.contains("Proxy-Authorization: Basic YWxpY2U6c2VjcmV0\r\n"));
    }

    #[test]
    fn rejects_http_connect_header_injection() {
        assert!(build_http_connect_request("host\r\nInjected: x", 22, "", "").is_err());
    }

    #[test]
    fn reports_sftp_timeout_without_mislabeling_the_ssh_shell() {
        let message = format_sftp_unavailable_reason("SFTP init failed: Timeout");

        assert!(message.contains("SFTP 子系统"));
        assert!(message.contains("SSH 终端已连接"));
        assert!(message.contains("sftp subsystem"));
    }

    #[test]
    fn only_reuses_saved_password_for_password_prompts() {
        assert!(is_password_prompt("Password: "));
        assert!(looks_like_mfa_prompt("Verification code: "));
        assert!(!is_password_prompt("Verification code: "));
        assert!(!is_password_prompt("OTP token: "));
    }

    #[test]
    fn keyboard_interactive_restart_requires_partial_success_and_another_kbi_factor() {
        let keyboard_interactive = MethodSet::from(&[MethodKind::KeyboardInteractive][..]);
        let password_only = MethodSet::from(&[MethodKind::Password][..]);

        assert!(should_restart_keyboard_interactive(
            true,
            &keyboard_interactive,
            0
        ));
        assert!(!should_restart_keyboard_interactive(
            false,
            &keyboard_interactive,
            0
        ));
        assert!(!should_restart_keyboard_interactive(
            true,
            &password_only,
            0
        ));
        assert!(!should_restart_keyboard_interactive(
            true,
            &keyboard_interactive,
            super::MAX_KEYBOARD_INTERACTIVE_RESTARTS
        ));
    }

    #[test]
    fn keyboard_interactive_fallback_uses_remaining_kbi_method() {
        let keyboard_interactive = MethodSet::from(&[MethodKind::KeyboardInteractive][..]);
        let partial = AuthResult::Failure {
            remaining_methods: keyboard_interactive.clone(),
            partial_success: true,
        };
        let alternate_method = AuthResult::Failure {
            remaining_methods: keyboard_interactive,
            partial_success: false,
        };

        assert_eq!(
            authentication_result_from_auth_result(&partial),
            AuthenticationResult::KeyboardInteractiveAvailable {
                mode: KeyboardInteractiveMode::AdditionalFactor,
            }
        );
        // A KBI-only server can expose password login as a normal alternative
        // with partial_success=false. That still enters KBI; it is a method
        // fallback rather than an additional MFA factor.
        assert_eq!(
            authentication_result_from_auth_result(&alternate_method),
            AuthenticationResult::KeyboardInteractiveAvailable {
                mode: KeyboardInteractiveMode::PasswordFallback,
            }
        );
    }

    #[test]
    fn ssh_interaction_flow_orders_jump_and_target_hops_together() {
        let flow = SshInteractionFlow::new();
        let jump_profile = serde_json::json!({
            "id": "jump-profile",
            "name": "Bastion",
        });
        let target_profile = serde_json::json!({
            "id": "target-profile",
            "name": "Production",
        });
        let jump = SshInteractionContext::from_profile(
            &flow,
            "tab-1",
            &jump_profile,
            "bastion.example",
            22,
            SshAuthenticationTarget::JumpHost,
            Some("main".to_string()),
            CancellationToken::new(),
        );
        let target = SshInteractionContext::from_profile(
            &flow,
            "tab-1",
            &target_profile,
            "server.example",
            2222,
            SshAuthenticationTarget::Target,
            Some("main".to_string()),
            CancellationToken::new(),
        );

        assert_eq!(jump.flow.flow_id, target.flow.flow_id);
        assert_eq!(jump.hop_index, 0);
        assert_eq!(target.hop_index, 1);
        assert_eq!(jump.authentication_target.as_str(), "jump-host");
        assert_eq!(target.authentication_target.as_str(), "target");
        assert_eq!(jump.next_sequence(), 1);
        assert_eq!(target.next_sequence(), 2);
    }

    #[tokio::test]
    async fn real_ssh_mfa_server_keeps_saved_password_out_of_otp_answer() {
        let responses = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut server_config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        server_config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
        );
        let server_responses = responses.clone();
        let server_task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(server_config),
                socket,
                KeyboardInteractiveMfaServer {
                    responses: server_responses,
                },
            )
            .await
            .unwrap();
            // Dropping the test client ends the SSH stream with EOF; that is
            // the expected lifecycle outcome after successful authentication.
            let _ = running.await;
        });

        let mut handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let requests = Arc::new(Mutex::new(Vec::<KeyboardInteractiveRequest>::new()));
        let requested_prompts = requests.clone();
        let authenticated = try_keyboard_interactive_with_responder(
            &mut handle,
            "alice",
            Some("saved-password"),
            KeyboardInteractiveMode::PasswordFallback,
            move |request| {
                let requested_prompts = requested_prompts.clone();
                async move {
                    requested_prompts.lock().unwrap().push(request);
                    Some(vec!["246810".to_string()])
                }
            },
        )
        .await
        .unwrap();

        assert!(authenticated);
        {
            let requests = requests.lock().unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].round, 1);
            assert_eq!(requests[0].prompts.len(), 1);
            assert_eq!(requests[0].prompts[0].prompt, "OTP code: ");
        }
        assert_eq!(
            responses.lock().unwrap().as_slice(),
            ["saved-password", "246810"]
        );

        drop(handle);
        timeout(Duration::from_secs(2), server_task)
            .await
            .expect("MFA fixture did not release its SSH socket")
            .unwrap();
    }

    #[tokio::test]
    async fn password_authentication_continues_keyboard_interactive_on_the_same_handle() {
        let password_attempts = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut server_config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        server_config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
        );
        let server_password_attempts = password_attempts.clone();
        let server_responses = responses.clone();
        let server_task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(server_config),
                socket,
                PasswordThenKeyboardInteractiveMfaServer {
                    password_attempts: server_password_attempts,
                    responses: server_responses,
                },
            )
            .await
            .unwrap();
            let _ = running.await;
        });

        let mut handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let password_result = handle
            .authenticate_password("alice", "saved-password")
            .await
            .unwrap();
        assert!(matches!(
            password_result,
            AuthResult::Failure {
                remaining_methods,
                partial_success: false,
            } if remaining_methods.contains(&MethodKind::KeyboardInteractive)
        ));

        // The vendored test server currently normalizes the password-handler
        // partial-success bit away. Exercise the RFC-correct client mapping
        // separately above, then continue the real KBI exchange on this same
        // handle to verify that no reconnect or second password attempt occurs.

        let requests = Arc::new(Mutex::new(Vec::<KeyboardInteractiveRequest>::new()));
        let requested_prompts = requests.clone();
        let authenticated = try_keyboard_interactive_with_responder(
            &mut handle,
            "alice",
            Some("saved-password"),
            KeyboardInteractiveMode::PasswordFallback,
            move |request| {
                let requested_prompts = requested_prompts.clone();
                async move {
                    requested_prompts.lock().unwrap().push(request);
                    Some(vec!["246810".to_string()])
                }
            },
        )
        .await
        .unwrap();

        assert!(authenticated);
        assert_eq!(password_attempts.load(Ordering::Relaxed), 1);
        assert_eq!(responses.lock().unwrap().as_slice(), ["246810"]);
        let request = requests
            .lock()
            .unwrap()
            .first()
            .cloned()
            .expect("keyboard-interactive fixture should request one OTP prompt");
        assert_eq!(request.round, 1);
        assert_eq!(request.prompts[0].prompt, "OTP code: ");

        drop(handle);
        timeout(Duration::from_secs(2), server_task)
            .await
            .expect("password-to-MFA fixture did not release its SSH socket")
            .unwrap();
    }

    #[tokio::test]
    async fn keyboard_interactive_restarts_after_partial_success_on_same_handle() {
        let responses = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut server_config = server::Config {
            inactivity_timeout: None,
            auth_rejection_time: Duration::from_millis(1),
            ..Default::default()
        };
        server_config.keys.push(
            PrivateKey::random(&mut rand::rng(), russh::keys::ssh_key::Algorithm::Ed25519).unwrap(),
        );
        let server_responses = responses.clone();
        let server_task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let running = server::run_stream(
                Arc::new(server_config),
                socket,
                MultiRoundKeyboardInteractiveServer {
                    challenge_round: AtomicUsize::new(0),
                    responses: server_responses,
                },
            )
            .await
            .unwrap();
            let _ = running.await;
        });

        let mut handle = client::connect(
            Arc::new(client::Config::default()),
            address,
            AcceptTestServerKey,
        )
        .await
        .unwrap();
        let requested_rounds = Arc::new(Mutex::new(Vec::new()));
        let requested_rounds_for_responder = requested_rounds.clone();
        let authenticated = try_keyboard_interactive_with_responder(
            &mut handle,
            "alice",
            None,
            KeyboardInteractiveMode::AdditionalFactor,
            move |request| {
                let requested_rounds = requested_rounds_for_responder.clone();
                async move {
                    requested_rounds.lock().unwrap().push(request.round);
                    Some(vec![match request.round {
                        1 => "111111".to_string(),
                        2 => "222222".to_string(),
                        _ => return None,
                    }])
                }
            },
        )
        .await
        .unwrap();

        assert!(authenticated);
        assert_eq!(requested_rounds.lock().unwrap().as_slice(), [1, 2]);
        assert_eq!(responses.lock().unwrap().as_slice(), ["111111", "222222"]);

        drop(handle);
        timeout(Duration::from_secs(2), server_task)
            .await
            .expect("multi-round MFA fixture did not release its SSH socket")
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_accepts_tauri_auth_exec_sftp_and_platform_probe() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;

        let profile = serde_json::json!({ "proxy": { "type": "none" } });
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;

        let command =
            crate::sessions::system_metrics::exec_command(&handle, "printf 'tauri-openssh-exec'")
                .await
                .unwrap();
        assert_eq!(command, "tauri-openssh-exec");

        let platform = crate::sessions::system_metrics::probe_remote_platform(&handle).await;
        #[cfg(target_os = "linux")]
        assert_eq!(platform, "linux");
        #[cfg(target_os = "macos")]
        assert_eq!(
            platform, "darwin",
            "macOS remotes must be detected as `darwin` so CWD tracking stays active"
        );

        let channel = handle.channel_open_session().await.unwrap();
        channel.request_subsystem(true, "sftp").await.unwrap();
        let sftp = russh_sftp::client::SftpSession::new(channel.into_stream())
            .await
            .unwrap();
        let remote_file = fixture.remote_dir.join("tauri-sftp.txt");
        let remote_file = remote_file.to_string_lossy().into_owned();
        sftp.create(&remote_file).await.unwrap();
        sftp.write(&remote_file, b"tauri-openssh-sftp")
            .await
            .unwrap();
        assert_eq!(
            sftp.read(&remote_file).await.unwrap(),
            b"tauri-openssh-sftp"
        );
        sftp.close().await.unwrap();

        let target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target_listener.local_addr().unwrap();
        let target = tokio::spawn(async move {
            let (mut socket, _) = target_listener.accept().await.unwrap();
            let mut request = [0_u8; 4];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"ping");
            socket.write_all(b"pong").await.unwrap();
        });
        let local_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_address = local_listener.local_addr().unwrap();
        let mut local_client = tokio::net::TcpStream::connect(local_address).await.unwrap();
        let (local_socket, _) = local_listener.accept().await.unwrap();
        let tunnel_handle = Arc::new(handle);
        let tunnel_rule = SshTunnelRule {
            id: "real-openssh-local".to_string(),
            name: "real-openssh-local".to_string(),
            kind: "local".to_string(),
            bind_host: "127.0.0.1".to_string(),
            bind_port: local_address.port(),
            target_host: Some("127.0.0.1".to_string()),
            target_port: Some(target_address.port()),
            auto_start: false,
        };
        let bridge = tokio::spawn({
            let tunnel_handle = tunnel_handle.clone();
            async move { forward_local_connection(local_socket, tunnel_handle, &tunnel_rule).await }
        });
        local_client.write_all(b"ping").await.unwrap();
        let mut response = [0_u8; 4];
        local_client.read_exact(&mut response).await.unwrap();
        assert_eq!(&response, b"pong");
        drop(local_client);
        timeout(Duration::from_secs(2), target)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(2), bridge)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        let dynamic_target_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dynamic_target_address = dynamic_target_listener.local_addr().unwrap();
        let dynamic_target = tokio::spawn(async move {
            let (mut socket, _) = dynamic_target_listener.accept().await.unwrap();
            let mut request = [0_u8; 5];
            socket.read_exact(&mut request).await.unwrap();
            assert_eq!(&request, b"socks");
            socket.write_all(b"proxy").await.unwrap();
        });
        let dynamic_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dynamic_address = dynamic_listener.local_addr().unwrap();
        let mut dynamic_client = tokio::net::TcpStream::connect(dynamic_address)
            .await
            .unwrap();
        let (dynamic_socket, _) = dynamic_listener.accept().await.unwrap();
        let dynamic_bridge = tokio::spawn({
            let tunnel_handle = tunnel_handle.clone();
            async move { forward_socks5_connection(dynamic_socket, tunnel_handle).await }
        });
        dynamic_client.write_all(&[5, 1, 0]).await.unwrap();
        let mut selected = [0_u8; 2];
        dynamic_client.read_exact(&mut selected).await.unwrap();
        assert_eq!(&selected, &[5, 0]);
        dynamic_client
            .write_all(&[
                5,
                1,
                0,
                1,
                127,
                0,
                0,
                1,
                (dynamic_target_address.port() >> 8) as u8,
                dynamic_target_address.port() as u8,
            ])
            .await
            .unwrap();
        let mut connected = [0_u8; 10];
        dynamic_client.read_exact(&mut connected).await.unwrap();
        assert_eq!(&connected[..2], &[5, 0]);
        dynamic_client.write_all(b"socks").await.unwrap();
        let mut dynamic_response = [0_u8; 5];
        dynamic_client
            .read_exact(&mut dynamic_response)
            .await
            .unwrap();
        assert_eq!(&dynamic_response, b"proxy");
        drop(dynamic_client);
        timeout(Duration::from_secs(2), dynamic_target)
            .await
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(2), dynamic_bridge)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        let remote_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let remote_port = remote_listener.local_addr().unwrap().port();
        drop(remote_listener);
        let returned_port = tunnel_handle
            .tcpip_forward("127.0.0.1", u32::from(remote_port))
            .await
            .unwrap();
        assert_eq!(
            returned_port, 0,
            "OpenSSH fixed-port success has no allocated-port payload"
        );
        let effective_port = effective_remote_forward_port(remote_port, returned_port).unwrap();
        assert_eq!(effective_port, u32::from(remote_port));
        let remote_client = timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(("127.0.0.1", remote_port)),
        )
        .await
        .expect("remote forward did not begin listening")
        .unwrap();
        drop(remote_client);
        tunnel_handle
            .cancel_tcpip_forward("127.0.0.1", effective_port)
            .await
            .unwrap();
        let rebound = TcpListener::bind(("127.0.0.1", remote_port))
            .await
            .expect("cancel remote forward did not release the requested port");
        drop(rebound);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_exec_timeout_preserves_partial_remote_output() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;

        let profile = serde_json::json!({ "proxy": { "type": "none" } });
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;
        let result = crate::sessions::system_metrics::exec_command_with_status_timeout_detailed(
            &handle,
            "printf 'partial-diagnostic'; sleep 1",
            Duration::from_millis(100),
        )
        .await
        .expect("bounded exec should return the collected partial output");

        assert!(result.timed_out);
        assert_eq!(result.output, "partial-diagnostic");
        assert_eq!(result.exit_code, None);
        assert!(!result.output_truncated);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_authenticates_through_tauri_http_proxy_transport() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let target_port = fixture.port;
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let request = read_http_headers(&mut client).await;
            assert!(request.starts_with(&format!("CONNECT 127.0.0.1:{target_port} HTTP/1.1\r\n")));
            assert!(request.contains("Proxy-Authorization: Basic cHJveHktdXNlcjpwcm94eS1wYXNz\r\n"));
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(("127.0.0.1", target_port))
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });
        let profile = serde_json::json!({
            "proxy": {
                "type": "http",
                "host": "127.0.0.1",
                "port": proxy_address.port(),
                "username": "proxy-user",
                "password": "proxy-pass"
            }
        });
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;
        let output = crate::sessions::system_metrics::exec_command(
            &handle,
            "printf 'tauri-openssh-http-proxy'",
        )
        .await
        .unwrap();
        assert_eq!(output, "tauri-openssh-http-proxy");
        drop(handle);
        timeout(Duration::from_secs(2), proxy)
            .await
            .expect("HTTP proxy transport did not release")
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn real_openssh_sshd_authenticates_through_tauri_socks5_proxy_transport() {
        let fixture = start_openssh_fixture();
        wait_for_openssh(fixture.port).await;
        let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_address = proxy_listener.local_addr().unwrap();
        let target_port = fixture.port;
        let proxy = tokio::spawn(async move {
            let (mut client, _) = proxy_listener.accept().await.unwrap();
            let (host, port) = read_socks5_connect_request(&mut client).await;
            assert_eq!(host, "127.0.0.1");
            assert_eq!(port, target_port);
            client
                .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
                .await
                .unwrap();
            let mut target = tokio::net::TcpStream::connect(("127.0.0.1", target_port))
                .await
                .unwrap();
            tokio::io::copy_bidirectional(&mut client, &mut target)
                .await
                .unwrap();
        });
        let profile = serde_json::json!({
            "proxy": {
                "type": "socks5",
                "host": "127.0.0.1",
                "port": proxy_address.port()
            }
        });
        let handle = authenticate_openssh_fixture(&fixture, &profile).await;
        let output = crate::sessions::system_metrics::exec_command(
            &handle,
            "printf 'tauri-openssh-socks5-proxy'",
        )
        .await
        .unwrap();
        assert_eq!(output, "tauri-openssh-socks5-proxy");
        drop(handle);
        timeout(Duration::from_secs(2), proxy)
            .await
            .expect("SOCKS5 proxy transport did not release")
            .unwrap();
    }

    #[test]
    fn validates_tunnel_rules_and_normalizes_cross_platform_bind_addresses() {
        let valid = SshTunnelRule {
            id: "local-db".to_string(),
            name: "database".to_string(),
            kind: "local".to_string(),
            bind_host: "127.0.0.1".to_string(),
            bind_port: 15432,
            target_host: Some("db.internal".to_string()),
            target_port: Some(5432),
            auto_start: false,
        };
        assert!(validate_tunnel_rule(&valid).is_ok());
        assert_eq!(tunnel_bind_address("*", 1080).unwrap(), "0.0.0.0:1080");
        assert_eq!(tunnel_bind_address("::1", 1080).unwrap(), "[::1]:1080");

        let invalid = SshTunnelRule {
            target_port: None,
            ..valid
        };
        assert!(validate_tunnel_rule(&invalid).is_err());
    }

    #[test]
    fn remote_forward_matches_exact_and_wildcard_bind_hosts() {
        assert!(remote_bind_host_matches("127.0.0.1", "127.0.0.1"));
        assert!(!remote_bind_host_matches("127.0.0.1", "10.0.0.4"));
        assert!(remote_bind_host_matches("0.0.0.0", "10.0.0.4"));
        assert!(remote_bind_host_matches("::", "2001:db8::4"));
    }

    #[test]
    fn remote_forward_keeps_fixed_port_when_server_reply_has_no_port() {
        assert_eq!(effective_remote_forward_port(15432, 0).unwrap(), 15432);
        assert_eq!(effective_remote_forward_port(0, 49152).unwrap(), 49152);
        assert!(effective_remote_forward_port(0, 0).is_err());
        assert!(effective_remote_forward_port(0, 65536).is_err());
    }

    #[test]
    fn legacy_preferred_appends_sha1_algorithms_after_sha2() {
        use russh::{kex, mac};

        let preferred = build_legacy_preferred();

        // SHA-2 类 MAC 应在 SHA-1 之前（保持 SHA-2 优先）
        let sha256_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA256)
            .expect("SHA-256 MAC must remain in legacy list");
        let sha1_etm_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA1_ETM)
            .expect("SHA-1 ETM MAC must be appended for legacy servers");
        let sha1_pos = preferred
            .mac
            .iter()
            .position(|m| *m == mac::HMAC_SHA1)
            .expect("SHA-1 MAC must be appended for legacy servers");
        assert!(sha256_pos < sha1_etm_pos);
        assert!(sha1_etm_pos < sha1_pos);

        // SHA-2 类 KEX（DH_G14_SHA256）应在 SHA-1 类（DH_G14_SHA1）之前
        let sha256_kex_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G14_SHA256)
            .expect("SHA-256 KEX must remain in legacy list");
        let sha1_kex_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G14_SHA1)
            .expect("SHA-1 KEX must be appended for legacy servers");
        let g1_pos = preferred
            .kex
            .iter()
            .position(|k| *k == kex::DH_G1_SHA1)
            .expect("DH-G1-SHA1 must be appended for very old servers");
        assert!(sha256_kex_pos < sha1_kex_pos);
        assert!(sha1_kex_pos < g1_pos);
    }
}
