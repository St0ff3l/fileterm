#[cfg(test)]
mod tests {
    use super::{
        configured_device_mode_for_profile, initial_remote_path_for_profile,
        reconnect_mode_for_profile, ConnectionCapabilities, PaneNode, SplitDirection,
        TransferRunHandle, WorkspaceState, WorkspaceTabStatus,
    };
    use std::sync::{Arc, Mutex};
    use tauri::ipc::Channel;

    #[test]
    fn ssh_is_the_only_session_type_with_tunnel_capability() {
        assert!(ConnectionCapabilities::for_session_type("ssh").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("ftp").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("telnet").tunnels);
        assert!(!ConnectionCapabilities::for_session_type("serial").tunnels);
    }

    #[test]
    fn capabilities_serialize_with_the_core_camel_case_shape() {
        let value = serde_json::to_value(ConnectionCapabilities::for_session_type("ssh")).unwrap();

        assert_eq!(value["resourceMonitoring"], true);
        assert_eq!(value["shellIntegration"], true);
        assert_eq!(value["fileAccess"], true);
        assert_eq!(value["tunnels"], true);
    }

    #[test]
    fn network_device_profiles_expose_only_terminal_and_tunnels() {
        let profile = serde_json::json!({
            "type": "ssh",
            "deviceMode": "network-device"
        });

        assert!(ConnectionCapabilities::is_network_device_profile(&profile));
        assert_eq!(
            ConnectionCapabilities::for_profile(&profile),
            ConnectionCapabilities {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: true,
            }
        );
    }

    #[test]
    fn missing_or_auto_device_mode_keeps_legacy_server_capabilities() {
        for profile in [
            serde_json::json!({ "type": "ssh" }),
            serde_json::json!({ "type": "ssh", "deviceMode": "auto" }),
        ] {
            assert!(!ConnectionCapabilities::is_network_device_profile(&profile));
            assert_eq!(
                ConnectionCapabilities::for_profile(&profile),
                ConnectionCapabilities::for_session_type("ssh")
            );
        }
    }

    #[test]
    fn tab_status_serializes_to_the_core_union_values() {
        let statuses = [
            (WorkspaceTabStatus::Idle, "idle"),
            (WorkspaceTabStatus::Connecting, "connecting"),
            (WorkspaceTabStatus::Connected, "connected"),
            (WorkspaceTabStatus::Error, "error"),
            (WorkspaceTabStatus::Closed, "closed"),
        ];
        for (status, expected) in statuses {
            assert_eq!(serde_json::to_value(status).unwrap(), expected);
        }
    }

    #[test]
    fn local_terminal_capabilities_expose_only_the_terminal_surface() {
        assert_eq!(
            ConnectionCapabilities::for_session_type("local"),
            ConnectionCapabilities {
                terminal: true,
                files: false,
                resource_monitoring: false,
                shell_integration: false,
                file_access: false,
                tunnels: false,
            }
        );
    }

    #[test]
    fn reconnect_mode_is_present_for_network_profiles() {
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "reconnectMode": "enter"
            })),
            Some("enter".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({ "type": "ssh" })),
            Some("none".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(
                &serde_json::json!({ "type": "ftp", "reconnectMode": "auto" })
            ),
            Some("auto".to_string())
        );
        assert_eq!(
            reconnect_mode_for_profile(&serde_json::json!({
                "type": "serial",
                "reconnectMode": "auto"
            })),
            Some("auto".to_string())
        );
    }

    #[test]
    fn configured_device_mode_does_not_publish_auto_before_handshake() {
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "network-device"
            })),
            Some("network-device".to_string())
        );
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({
                "type": "ssh",
                "deviceMode": "auto"
            })),
            None
        );
        assert_eq!(
            configured_device_mode_for_profile(&serde_json::json!({ "type": "ftp" })),
            None
        );
    }

    #[test]
    fn initial_remote_path_respects_profile_and_protocol_defaults() {
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({
                "type": "ssh",
                "remotePath": "/srv/app"
            })),
            "/srv/app"
        );
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({ "type": "ssh" })),
            "."
        );
        assert_eq!(
            initial_remote_path_for_profile(&serde_json::json!({ "type": "ftp" })),
            "/"
        );
    }

    #[test]
    fn terminal_output_channel_preserves_stream_order_under_load() {
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_messages = Arc::clone(&received);
        let channel = Channel::new(move |body| {
            let payload: serde_json::Value = body.deserialize().unwrap();
            received_messages.lock().unwrap().push(payload);
            Ok(())
        });
        let state = WorkspaceState::default();
        state.register_terminal_output_channel(channel);

        for index in 0..2_000 {
            state.publish_terminal_output("tab-load", &format!("{index}\r\n"));
        }

        let messages = received.lock().unwrap();
        assert_eq!(messages.len(), 2_000);
        for (index, payload) in messages.iter().enumerate() {
            assert_eq!(payload["tabId"], "tab-load");
            assert_eq!(payload["chunk"], format!("{index}\r\n"));
        }
    }

    #[tokio::test]
    async fn ai_session_revision_ignores_output_and_changes_on_target_transition() {
        let state = WorkspaceState::default();

        state.publish_terminal_output("tab-target", "prompt\r\n");
        assert_eq!(state.ai_session_revision("tab-target").await, 0);

        assert_eq!(state.touch_ai_session_revision("tab-target").await, 1);
        state.publish_terminal_output("tab-target", "command output\r\n");
        assert_eq!(state.ai_session_revision("tab-target").await, 1);

        assert_eq!(state.touch_ai_session_revision("tab-target").await, 2);
    }

    #[test]
    fn split_weights_update_only_the_targeted_nested_split() {
        let mut pane_root = PaneNode::Split {
            direction: SplitDirection::Row,
            weights: vec![0.5, 0.5],
            children: vec![
                PaneNode::Leaf {
                    tab_id: "left".to_string(),
                },
                PaneNode::Split {
                    direction: SplitDirection::Column,
                    weights: vec![0.5, 0.5],
                    children: vec![
                        PaneNode::Leaf {
                            tab_id: "top-right".to_string(),
                        },
                        PaneNode::Leaf {
                            tab_id: "bottom-right".to_string(),
                        },
                    ],
                },
            ],
        };

        assert!(pane_root.set_split_weights_at_path(&[1], &[0.25, 0.75]));

        let PaneNode::Split {
            weights, children, ..
        } = pane_root
        else {
            panic!("root should remain a split");
        };
        assert_eq!(weights, vec![0.5, 0.5]);
        let PaneNode::Split {
            weights: nested_weights,
            ..
        } = &children[1]
        else {
            panic!("right pane should remain a split");
        };
        assert_eq!(nested_weights, &vec![0.25, 0.75]);
    }

    #[test]
    fn pane_nodes_serialize_with_the_core_camel_case_shape() {
        let value = serde_json::to_value(PaneNode::Leaf {
            tab_id: "pane-1".to_string(),
        })
        .unwrap();

        assert_eq!(value["kind"], "leaf");
        assert_eq!(value["tabId"], "pane-1");
        assert!(value.get("tab_id").is_none());
    }

    #[tokio::test]
    async fn transfer_run_handle_exposes_cancel_and_waits_for_settlement() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let (settled_tx, settled_rx) = tokio::sync::watch::channel(false);
        let handle = TransferRunHandle {
            generation: 7,
            cancel: cancel.clone(),
            settled: settled_rx,
        };

        handle.cancel.cancel();
        assert!(cancel.is_cancelled());
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            let _ = settled_tx.send(true);
        });

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            handle.wait_until_settled(),
        )
        .await
        .expect("run settlement should wake all waiters");
    }
}
