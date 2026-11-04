#[cfg(test)]
mod tests {
    use super::{
        check_cancellation, detect_privileged_auth_failure, parse_remote_exec_result,
        privileged_command_kind, resolve_privileged_password, validate_network_device_command,
        validate_privileged_password, validate_remote_exec_command, validate_remote_exec_cwd,
        validate_remote_exec_tab_id, validate_visible_terminal_command, wrap_sudo_command,
        ActionApprovalDecision, ActionApprovalSource, PrivilegedCommandKind, AI_REQUEST_CANCELLED,
        SUDO_AUTH_FAILURE, VISIBLE_TERMINAL_COMMAND_INVALID,
    };
    use crate::AppError;
    use serde_json::json;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn approval_rejections_remain_specific_to_the_initiating_surface() {
        assert_eq!(
            ActionApprovalDecision::Rejected.rejection_message(ActionApprovalSource::Mcp),
            "FileTerm external operation was rejected by the user"
        );
        assert_eq!(
            ActionApprovalDecision::Rejected.rejection_message(ActionApprovalSource::Cli),
            "FileTerm external operation was rejected by the user"
        );
        assert_eq!(
            ActionApprovalDecision::TimedOut.rejection_message(ActionApprovalSource::AiCopilot),
            "Copilot approval timed out; the command was not started"
        );
    }

    #[test]
    fn approval_sources_keep_cli_and_mcp_wire_labels() {
        assert_eq!(
            serde_json::to_value(ActionApprovalSource::Cli).unwrap(),
            "cli"
        );
        assert_eq!(
            serde_json::to_value(ActionApprovalSource::Mcp).unwrap(),
            "mcp"
        );
    }

    #[test]
    fn remote_exec_cancellation_is_checked_before_any_side_effect() {
        let cancellation = CancellationToken::new();
        assert!(check_cancellation(Some(&cancellation)).is_ok());

        cancellation.cancel();
        assert!(matches!(
            check_cancellation(Some(&cancellation)),
            Err(AppError::Command(message)) if message == AI_REQUEST_CANCELLED
        ));
        assert!(check_cancellation(None).is_ok());
    }

    #[test]
    fn remote_exec_parser_preserves_the_bounded_output_signal() {
        let result = parse_remote_exec_result(json!({
            "output": "partial output",
            "exitCode": 0,
            "timedOut": false,
            "outputTruncated": true,
        }))
        .expect("remote exec result should parse");

        assert_eq!(result.output, "partial output");
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert!(result.output_truncated);
        assert!(!result.raw_terminal);
        assert!(!result.input_required);
        assert_eq!(result.input_kind, None);
    }

    #[test]
    fn remote_exec_parser_exposes_only_a_bounded_input_hint() {
        let result = parse_remote_exec_result(json!({
            "output": "Password: ",
            "exitCode": null,
            "timedOut": true,
            "outputTruncated": false,
            "inputRequired": true,
            "inputKind": "secret",
        }))
        .expect("remote exec input hint should parse");

        assert!(result.input_required);
        assert_eq!(result.input_kind.as_deref(), Some("secret"));

        let invalid_kind = parse_remote_exec_result(json!({
            "output": "Continue? [y/N]",
            "exitCode": null,
            "timedOut": true,
            "outputTruncated": false,
            "inputRequired": true,
            "inputKind": "password",
        }))
        .expect("invalid input kind should not break result parsing");
        assert!(!invalid_kind.input_required);
        assert_eq!(invalid_kind.input_kind, None);
    }

    #[test]
    fn network_device_commands_are_single_line_and_marked_raw() {
        assert_eq!(
            validate_network_device_command("display version").unwrap(),
            "display version"
        );
        assert!(validate_network_device_command("display version\r").is_err());
        assert!(validate_network_device_command("display\nversion").is_err());
    }

    #[test]
    fn visible_terminal_commands_are_single_line() {
        assert_eq!(
            validate_visible_terminal_command("uname -a").unwrap(),
            "uname -a"
        );
        let error = validate_visible_terminal_command("printf 'first\nsecond'").unwrap_err();
        assert!(matches!(
            error,
            AppError::Command(message) if message == VISIBLE_TERMINAL_COMMAND_INVALID
        ));
    }

    #[test]
    fn remote_exec_validators_reject_empty_and_unsafe_routing_values() {
        assert!(validate_remote_exec_tab_id("\n").is_err());
        assert!(validate_remote_exec_command("  ").is_err());
        assert!(validate_remote_exec_cwd(Some("x".repeat(4_097))).is_err());
        assert_eq!(
            validate_remote_exec_cwd(Some(" /srv/app ".to_string())).unwrap(),
            Some("/srv/app".to_string())
        );
    }

    #[test]
    fn privileged_detection_only_accepts_a_leading_shell_token() {
        assert_eq!(
            privileged_command_kind("  sudo -S id"),
            Some(PrivilegedCommandKind::Sudo)
        );
        assert_eq!(
            privileged_command_kind("su -c 'id'"),
            Some(PrivilegedCommandKind::Su)
        );
        assert_eq!(privileged_command_kind("sudoers --check"), None);
        assert_eq!(privileged_command_kind("echo sudo id"), None);
    }

    #[test]
    fn sudo_wrapper_keeps_password_out_of_the_command_text() {
        let command = wrap_sudo_command("  sudo -u root id");
        assert_eq!(command, "sudo -S -p '' -u root id");
        assert!(!command.contains("secret"));
    }

    #[test]
    fn privileged_auth_failures_are_classified_without_returning_remote_output() {
        assert!(detect_privileged_auth_failure(
            "sudo: incorrect password",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "sudo: 3 incorrect password attempts",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "sudo: authentication failure",
            PrivilegedCommandKind::Sudo
        ));
        assert!(detect_privileged_auth_failure(
            "su: Authentication failure",
            PrivilegedCommandKind::Su
        ));
        assert!(!detect_privileged_auth_failure(
            "command completed",
            PrivilegedCommandKind::Sudo
        ));
        assert_eq!(SUDO_AUTH_FAILURE, "SUDO_AUTH_FAILURE");
    }

    #[test]
    fn privileged_password_validation_rejects_control_input() {
        assert!(validate_privileged_password("secret").is_ok());
        assert!(validate_privileged_password("").is_err());
        assert!(validate_privileged_password("secret\n").is_err());
    }

    #[test]
    fn privileged_password_priority_is_explicit_then_saved_without_login_fallback() {
        assert_eq!(
            resolve_privileged_password(
                PrivilegedCommandKind::Sudo,
                Some("explicit".to_string()),
                Some("saved".to_string()),
            )
            .unwrap(),
            "explicit"
        );
        assert_eq!(
            resolve_privileged_password(
                PrivilegedCommandKind::Sudo,
                None,
                Some("saved".to_string()),
            )
            .unwrap(),
            "saved"
        );
        let error = resolve_privileged_password(PrivilegedCommandKind::Su, None, None).unwrap_err();
        assert!(matches!(error, AppError::Command(message) if message == "SU_PASSWORD_NEEDED"));
    }
}
