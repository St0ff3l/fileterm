use super::create_tab_layout;

#[test]
fn network_device_ssh_profiles_start_with_terminal_only_layout() {
    assert_eq!(
        create_tab_layout(&serde_json::json!({
            "type": "ssh",
            "deviceMode": "network-device"
        })),
        "terminal-only"
    );
    assert_eq!(
        create_tab_layout(&serde_json::json!({ "type": "ssh" })),
        "terminal-file"
    );
}
