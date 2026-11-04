use super::*;

#[test]
fn blocks_dangerous_commands_but_allows_scoped_operations() {
    assert_eq!(
        authorize_command("rm -rf /", AiCommandRisk::Destructive, true, None, None)
            .unwrap_err()
            .code,
        AUTO_MODE_BLOCKED_COMMAND
    );
    assert_eq!(
        authorize_command("sudo rm -rf /", AiCommandRisk::Privileged, true, None, None)
            .unwrap_err()
            .code,
        AUTO_MODE_BLOCKED_COMMAND
    );
    assert!(authorize_command(
        "rm -rf /tmp/fileterm",
        AiCommandRisk::Destructive,
        true,
        None,
        None,
    )
    .is_ok());
    assert!(authorize_command(
        "sudo systemctl restart ssh",
        AiCommandRisk::Privileged,
        true,
        None,
        None,
    )
    .is_ok());
}

#[test]
fn requires_whitelist_for_unknown_irreversible_commands() {
    assert_eq!(
        authorize_command(
            "truncate --size 0 /var/log/app.log",
            AiCommandRisk::Destructive,
            true,
            None,
            None,
        )
        .unwrap_err()
        .code,
        AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED
    );
}

#[test]
fn target_revision_check_is_always_applied() {
    assert_eq!(
        authorize_command("pwd", AiCommandRisk::ReadOnly, false, Some("1"), Some("2"))
            .unwrap_err()
            .code,
        AUTO_MODE_TARGET_CHANGED
    );
    assert!(authorize_command("pwd", AiCommandRisk::ReadOnly, false, Some("1"), Some("1")).is_ok());
}

#[test]
fn disabling_restrictions_allows_commands_that_would_otherwise_be_blocked() {
    assert!(authorize_command("rm -rf /", AiCommandRisk::Destructive, false, None, None).is_ok());
    assert!(authorize_command(
        "truncate --size 0 /var/log/app.log",
        AiCommandRisk::Destructive,
        false,
        None,
        None,
    )
    .is_ok());
}

#[test]
fn rejects_empty_commands() {
    assert_eq!(
        authorize_command("  ", AiCommandRisk::ReadOnly, false, None, None)
            .unwrap_err()
            .code,
        AUTO_MODE_BLOCKED_COMMAND
    );
}
