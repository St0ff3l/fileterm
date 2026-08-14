//! Guardrails for the fully-automatic Copilot execution path.
//!
//! This module answers only whether a locally validated command is eligible
//! for automatic execution. No function here starts a remote process or
//! receives a secret.

use super::ai::AiCommandRisk;

pub const AUTO_MODE_BLOCKED_COMMAND: &str = "AI_AUTO_MODE_BLOCKED_COMMAND";
pub const AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED: &str =
    "AI_AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED";
pub const AUTO_MODE_TARGET_CHANGED: &str = "AI_AUTO_MODE_TARGET_CHANGED";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoModeGuardrailError {
    pub code: &'static str,
    pub reason: String,
}

const DANGEROUS_COMMAND_PATTERNS: &[&str] = &[
    "mkfs",
    "wipefs",
    "dd if=/dev/zero of=/dev/",
    "dd if=/dev/random of=/dev/",
    "chmod -R 777 /",
    "chown -R /",
    ":(){ :|:& };:",
    "kill -9 -1",
    "nc -l -p ",
    "bash -i >& /dev/tcp/",
    "sh -i >& /dev/tcp/",
    "apt remove --purge ",
    "apt-get remove --purge ",
    "yum remove -y ",
    "dnf remove -y ",
    "rmmod ",
    "modprobe -r ",
    "shutdown",
    "reboot",
    "init 0",
    "init 6",
    "halt",
    "poweroff",
];

const IRREVERSIBLE_WHITELIST: &[&str] = &[
    "rm ",
    "mv ",
    "cp ",
    "chmod ",
    "chown ",
    "apt install",
    "apt-get install",
    "yum install",
    "dnf install",
    "pip install",
    "npm install",
    "systemctl restart ",
    "systemctl stop ",
    "systemctl start ",
    "sudo systemctl restart ",
    "sudo systemctl stop ",
    "sudo systemctl start ",
    "doas systemctl restart ",
    "doas systemctl stop ",
    "doas systemctl start ",
    "sudo apt install ",
    "sudo apt-get install ",
    "sudo yum install ",
    "sudo dnf install ",
    "doas apt install ",
    "doas apt-get install ",
    "doas yum install ",
    "doas dnf install ",
    ">",
    ">>",
];

fn command_starts_with(command: &str, prefix: &str) -> bool {
    command == prefix.trim_end() || command.starts_with(prefix)
}

fn is_root_rm(command: &str) -> bool {
    command == "rm -rf /"
        || command.starts_with("rm -rf /*")
        || command.starts_with("rm -rf / ")
        || command == "rm -fr /"
        || command.starts_with("rm -fr /*")
        || command.starts_with("rm -fr / ")
}

fn is_home_rm(command: &str) -> bool {
    command == "rm -rf ~"
        || command.starts_with("rm -rf ~/")
        || command.starts_with("rm -rf ~ ")
        || command == "rm -fr ~"
        || command.starts_with("rm -fr ~/")
        || command.starts_with("rm -fr ~ ")
}

fn command_after_privilege_wrapper(command: &str) -> &str {
    let command = command.trim_start();
    let Some((wrapper, mut remainder)) = command.split_once(char::is_whitespace) else {
        return command;
    };
    if wrapper != "sudo" && wrapper != "doas" {
        return command;
    }
    remainder = remainder.trim_start();
    while let Some((option, rest)) = remainder.split_once(char::is_whitespace) {
        if !option.starts_with('-') {
            break;
        }
        remainder = rest.trim_start();
        if option == "--" {
            break;
        }
    }
    remainder
}

fn blocked_pattern(command: &str) -> Option<&'static str> {
    if is_root_rm(command) {
        return Some("rm -rf /");
    }
    if is_home_rm(command) {
        return Some("rm -rf ~");
    }
    let command = command_after_privilege_wrapper(command);
    if is_root_rm(command) {
        return Some("rm -rf /");
    }
    if is_home_rm(command) {
        return Some("rm -rf ~");
    }
    DANGEROUS_COMMAND_PATTERNS
        .iter()
        .copied()
        .find(|pattern| command_starts_with(command, pattern))
}

fn irreversible_is_whitelisted(command: &str) -> bool {
    IRREVERSIBLE_WHITELIST
        .iter()
        .any(|prefix| command_starts_with(command, prefix))
}

pub fn authorize_command(
    command: &str,
    risk: AiCommandRisk,
    dangerous_command_restrictions_enabled: bool,
    expected_session_revision: Option<&str>,
    current_session_revision: Option<&str>,
) -> Result<(), AutoModeGuardrailError> {
    if let (Some(expected), Some(current)) = (expected_session_revision, current_session_revision) {
        if expected != current {
            return Err(AutoModeGuardrailError {
                code: AUTO_MODE_TARGET_CHANGED,
                reason: "终端会话已变化，自动执行已停止".to_string(),
            });
        }
    }

    let command = command.trim();
    if command.is_empty() {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_BLOCKED_COMMAND,
            reason: "空命令不能自动执行".to_string(),
        });
    }

    if dangerous_command_restrictions_enabled {
        if let Some(pattern) = blocked_pattern(command) {
            return Err(AutoModeGuardrailError {
                code: AUTO_MODE_BLOCKED_COMMAND,
                reason: format!("命令命中危险模式：{pattern}"),
            });
        }
        if matches!(risk, AiCommandRisk::Destructive | AiCommandRisk::Privileged)
            && !irreversible_is_whitelisted(command)
        {
            return Err(AutoModeGuardrailError {
                code: AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED,
                reason: "不可逆或提权命令不在自动执行白名单中".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        assert!(
            authorize_command("pwd", AiCommandRisk::ReadOnly, false, Some("1"), Some("1")).is_ok()
        );
    }

    #[test]
    fn disabling_restrictions_allows_commands_that_would_otherwise_be_blocked() {
        assert!(
            authorize_command("rm -rf /", AiCommandRisk::Destructive, false, None, None).is_ok()
        );
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
}
