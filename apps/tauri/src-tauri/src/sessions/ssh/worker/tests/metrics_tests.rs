use super::{
    append_metrics_stderr_tail, interactive_gateway_menu_detected, metrics_identity_field,
    metrics_stdout_preview, target_identity_is_valid, target_shell_prompt_detected,
    METRICS_STDERR_TAIL_BYTES,
};

#[test]
fn stderr_tail_is_bounded_and_keeps_latest_bytes() {
    let mut tail = Vec::new();
    append_metrics_stderr_tail(&mut tail, &[b'a'; METRICS_STDERR_TAIL_BYTES]);
    append_metrics_stderr_tail(&mut tail, b"tail");

    assert_eq!(tail.len(), METRICS_STDERR_TAIL_BYTES);
    assert_eq!(&tail[tail.len() - 4..], b"tail");
    assert!(tail[..tail.len() - 4].iter().all(|byte| *byte == b'a'));
}

#[test]
fn stderr_tail_discards_old_chunk_when_new_chunk_exceeds_cap() {
    let mut tail = Vec::new();
    let chunk = (0..METRICS_STDERR_TAIL_BYTES + 32)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    append_metrics_stderr_tail(&mut tail, &chunk);

    assert_eq!(tail.len(), METRICS_STDERR_TAIL_BYTES);
    assert_eq!(tail, chunk[chunk.len() - METRICS_STDERR_TAIL_BYTES..]);
}

#[test]
fn identity_fields_are_read_from_nested_identity_payload() {
    let value = serde_json::json!({
        "identity": {
            "hostname": "centos-target",
            "osName": "CentOS Linux 7 (Core)",
            "kernelName": "Linux"
        }
    });

    assert_eq!(metrics_identity_field(&value, "hostname"), "centos-target");
    assert_eq!(
        metrics_identity_field(&value, "osName"),
        "CentOS Linux 7 (Core)"
    );
    assert_eq!(metrics_identity_field(&value, "kernelName"), "Linux");
}

#[test]
fn identity_field_keeps_flattened_payload_compatibility() {
    let value = serde_json::json!({"hostname": "legacy-target"});

    assert_eq!(metrics_identity_field(&value, "hostname"), "legacy-target");
}

#[test]
fn target_identity_requires_a_real_linux_identity_for_posix_metrics() {
    let target = serde_json::json!({
        "identity": {
            "hostname": "centos-target",
            "osName": "CentOS Linux 7 (Core)",
            "kernelName": "Linux"
        }
    });
    let gateway = serde_json::json!({
        "identity": {
            "hostname": "koko",
            "osName": "JumpServer",
            "kernelName": "Linux"
        }
    });
    let ubuntu = serde_json::json!({
        "identity": {
            "hostname": "ubuntu-target",
            "osName": "Ubuntu 24.04.2 LTS",
            "kernelName": "Linux"
        }
    });
    let armbian = serde_json::json!({
        "identity": {
            "hostname": "arm-target",
            "osName": "Armbian 26.8.1 trixie",
            "kernelName": "Linux"
        }
    });

    assert!(target_identity_is_valid(&target, "linux"));
    assert!(target_identity_is_valid(&ubuntu, "linux"));
    assert!(target_identity_is_valid(&armbian, "linux"));
    assert!(!target_identity_is_valid(&gateway, "linux"));
}

#[test]
fn target_identity_accepts_linux_distro_ids_and_parent_families() {
    for (name, id, parents) in [
        ("Pop!_OS 24.04", "pop", "ubuntu debian"),
        ("KDE neon 24.04", "neon", "ubuntu"),
        ("elementary OS 8", "elementary", "ubuntu"),
        ("NixOS 25.11", "nixos", ""),
        ("Solus 4.8", "solus", ""),
        ("Deepin 25", "deepin", "debian"),
        ("Anolis OS 8", "anolis", "rhel fedora"),
        ("AlmaLinux 9", "almalinux", "rhel centos fedora"),
        ("Azure Linux 3", "azurelinux", ""),
        ("Custom derivative", "custom", "other\tdebian"),
    ] {
        let block = format!(
            "__OS__{name}\r\n__OS_ID__{id}\r\n__OS_ID_LIKE__{parents}\r\n__KERNEL_NAME__Linux\r\n__HOSTNAME__target\r\n"
        );
        let parsed = crate::sessions::system_metrics::parse_system_metrics(&block, "linux");
        assert!(target_identity_is_valid(&parsed, "linux"), "{name}");
        assert_eq!(parsed["identity"]["osId"], id);
        assert_eq!(parsed["identity"]["osIdLike"], parents);
    }
}

#[test]
fn target_identity_keeps_gateway_and_incompatible_platform_boundaries() {
    for (name, id, parents, kernel, platform) in [
        ("JumpServer", "ubuntu", "debian", "Linux", "linux"),
        ("JumpServer Linux", "ubuntu", "debian", "Linux", "linux"),
        ("Go", "ubuntu", "debian", "Linux", "linux"),
        ("Koko", "ubuntu", "debian", "Linux", "linux"),
        ("Unknown appliance", "unknown", "debian-ish", "Linux", "linux"),
        ("NixOS", "nixos", "", "-", "linux"),
        ("NixOS", "nixos", "", "Darwin", "linux"),
        ("Ubuntu", "ubuntu", "debian", "Linux", "unknown"),
        ("Ubuntu", "ubuntu", "debian", "Linux", "freebsd"),
    ] {
        let value = serde_json::json!({ "identity": {
            "hostname": "target", "osName": name, "osId": id,
            "osIdLike": parents, "kernelName": kernel
        }});
        assert!(!target_identity_is_valid(&value, platform), "{name}/{platform}/{kernel}");
    }
}

#[test]
fn target_identity_rejects_missing_identity_fields() {
    let value = serde_json::json!({
        "identity": {
            "hostname": "centos-target",
            "osName": "-",
            "kernelName": "Linux"
        }
    });

    assert!(!target_identity_is_valid(&value, "linux"));
}

#[test]
fn stdout_preview_suppresses_interactive_gateway_menu() {
    let menu = b"JumpServer open source fortress system\n1) Search\nOpt> ";

    assert!(interactive_gateway_menu_detected(menu, 0));
    assert!(!interactive_gateway_menu_detected(menu, 1));
    assert!(!interactive_gateway_menu_detected(b"Linux\n", 0));
    assert_eq!(
        metrics_stdout_preview(menu, 0),
        "<interactive-gateway-menu-suppressed>"
    );
    assert_eq!(metrics_stdout_preview(b"probe failed\n", 0), "probe failed\n");
    assert_eq!(
        metrics_stdout_preview(menu, 1),
        "<suppressed-after-first-sample>"
    );
}

#[test]
fn target_shell_prompt_is_detected_without_logging_remote_prompt() {
    let prompt = b"Welcome\r\n\x1b]0;root@target:~\x07[root@target ~]# ";
    let colored_prompt = b"\x1b[01;32mroot@target\x1b[0m:~$\x1b[0m";

    assert!(target_shell_prompt_detected(prompt, 0));
    assert!(target_shell_prompt_detected(colored_prompt, 0));
    assert!(!target_shell_prompt_detected(prompt, 1));
    assert_eq!(metrics_stdout_preview(prompt, 0), "<target-shell-prompt>");
}

#[test]
fn unrelated_output_is_not_treated_as_a_target_shell_prompt() {
    assert!(!target_shell_prompt_detected(b"No PTY requested.\n", 0));
    assert!(!target_shell_prompt_detected(b"value #\n", 0));
}
