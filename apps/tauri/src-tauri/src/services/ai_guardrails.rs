//! Guardrails for the future fully-automatic Copilot execution path.
//!
//! This module is deliberately independent from the provider adapters. It
//! answers only whether a locally validated command is eligible for automatic
//! execution and how a session counter should be updated. No function here
//! starts a remote process or receives a secret.

use std::time::Duration;

use super::ai::{AiAutoModeThresholds, AiCommandRisk};

pub const AUTO_MODE_BLOCKED_COMMAND: &str = "AI_AUTO_MODE_BLOCKED_COMMAND";
pub const AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED: &str =
    "AI_AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED";
pub const AUTO_MODE_SESSION_LIMIT_REACHED: &str = "AI_AUTO_MODE_SESSION_LIMIT_REACHED";
pub const AUTO_MODE_RISK_LIMIT_REACHED: &str = "AI_AUTO_MODE_RISK_LIMIT_REACHED";
pub const AUTO_MODE_DURATION_LIMIT_REACHED: &str = "AI_AUTO_MODE_DURATION_LIMIT_REACHED";
pub const AUTO_MODE_TARGET_CHANGED: &str = "AI_AUTO_MODE_TARGET_CHANGED";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AutoModeCounters {
    pub tool_calls: u32,
    pub destructive_calls: u32,
    pub privileged_calls: u32,
    pub total_exec_duration_secs: u64,
}

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

pub const DEFAULT_MAX_TOOL_CALLS_PER_SESSION: u32 = 20;
pub const DEFAULT_MAX_DESTRUCTIVE_CALLS_PER_SESSION: u32 = 5;
pub const DEFAULT_MAX_PRIVILEGED_CALLS_PER_SESSION: u32 = 3;
pub const DEFAULT_MAX_TOTAL_EXEC_DURATION_SECS: u64 = 600;

pub fn default_thresholds() -> AiAutoModeThresholds {
    AiAutoModeThresholds {
        max_tool_calls_per_session: DEFAULT_MAX_TOOL_CALLS_PER_SESSION,
        max_destructive_calls_per_session: DEFAULT_MAX_DESTRUCTIVE_CALLS_PER_SESSION,
        max_privileged_calls_per_session: DEFAULT_MAX_PRIVILEGED_CALLS_PER_SESSION,
        max_total_exec_duration_secs: DEFAULT_MAX_TOTAL_EXEC_DURATION_SECS,
    }
}

/// Lowering a safety floor is never accepted from the renderer or a settings
/// file. Advanced settings may increase these limits later, but the defaults
/// remain the minimum safe values.
pub fn validate_thresholds(thresholds: &AiAutoModeThresholds) -> Result<(), String> {
    if thresholds.max_tool_calls_per_session < DEFAULT_MAX_TOOL_CALLS_PER_SESSION
        || thresholds.max_destructive_calls_per_session < DEFAULT_MAX_DESTRUCTIVE_CALLS_PER_SESSION
        || thresholds.max_privileged_calls_per_session < DEFAULT_MAX_PRIVILEGED_CALLS_PER_SESSION
        || thresholds.max_total_exec_duration_secs < DEFAULT_MAX_TOTAL_EXEC_DURATION_SECS
    {
        return Err("自动模式护栏阈值不能低于默认安全下限".to_string());
    }
    Ok(())
}

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
    counters: &AutoModeCounters,
    thresholds: &AiAutoModeThresholds,
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

    if let Some(pattern) = blocked_pattern(command) {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_BLOCKED_COMMAND,
            reason: format!("命令命中不可绕过的危险模式：{pattern}"),
        });
    }
    if counters.tool_calls >= thresholds.max_tool_calls_per_session {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_SESSION_LIMIT_REACHED,
            reason: "本次 Copilot 会话已达到自动执行次数上限".to_string(),
        });
    }
    if matches!(risk, AiCommandRisk::Destructive)
        && counters.destructive_calls >= thresholds.max_destructive_calls_per_session
    {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_RISK_LIMIT_REACHED,
            reason: "本次 Copilot 会话已达到破坏性操作上限".to_string(),
        });
    }
    if matches!(risk, AiCommandRisk::Privileged)
        && counters.privileged_calls >= thresholds.max_privileged_calls_per_session
    {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_RISK_LIMIT_REACHED,
            reason: "本次 Copilot 会话已达到提权操作上限".to_string(),
        });
    }
    if counters.total_exec_duration_secs >= thresholds.max_total_exec_duration_secs {
        return Err(AutoModeGuardrailError {
            code: AUTO_MODE_DURATION_LIMIT_REACHED,
            reason: "本次 Copilot 会话已达到自动执行时长上限".to_string(),
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
    Ok(())
}

/// Reserve the count and risk budget before an automatic command starts.
/// Keeping this reservation under the caller's mode-state lock prevents two
/// concurrent Copilot requests from both observing the same remaining budget.
pub fn reserve_execution(counters: &mut AutoModeCounters, risk: AiCommandRisk) {
    counters.tool_calls = counters.tool_calls.saturating_add(1);
    if matches!(risk, AiCommandRisk::Destructive) {
        counters.destructive_calls = counters.destructive_calls.saturating_add(1);
    }
    if matches!(risk, AiCommandRisk::Privileged) {
        counters.privileged_calls = counters.privileged_calls.saturating_add(1);
    }
}

pub fn record_execution_duration(counters: &mut AutoModeCounters, duration: Duration) {
    let seconds = duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0));
    counters.total_exec_duration_secs = counters.total_exec_duration_secs.saturating_add(seconds);
}

pub fn record_execution(counters: &mut AutoModeCounters, risk: AiCommandRisk, duration: Duration) {
    reserve_execution(counters, risk);
    record_execution_duration(counters, duration);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_root_deletion_but_allows_scoped_tmp_deletion() {
        let thresholds = default_thresholds();
        let counters = AutoModeCounters::default();
        assert_eq!(
            authorize_command(
                "rm -rf /",
                AiCommandRisk::Destructive,
                &counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_BLOCKED_COMMAND
        );
        assert!(authorize_command(
            "rm -rf /tmp/fileterm",
            AiCommandRisk::Destructive,
            &counters,
            &thresholds,
            None,
            None,
        )
        .is_ok());
        assert_eq!(
            authorize_command(
                "sudo rm -rf /",
                AiCommandRisk::Privileged,
                &counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_BLOCKED_COMMAND
        );
        assert_eq!(
            authorize_command(
                "sudo rm -rf ~",
                AiCommandRisk::Destructive,
                &counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_BLOCKED_COMMAND
        );
    }

    #[test]
    fn requires_whitelist_for_unknown_irreversible_commands() {
        let thresholds = default_thresholds();
        let counters = AutoModeCounters::default();
        assert_eq!(
            authorize_command(
                "truncate --size 0 /var/log/app.log",
                AiCommandRisk::Destructive,
                &counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_IRREVERSIBLE_NOT_WHITELISTED
        );
        assert!(authorize_command(
            "sudo systemctl restart ssh",
            AiCommandRisk::Privileged,
            &counters,
            &thresholds,
            None,
            None,
        )
        .is_ok());
    }

    #[test]
    fn enforces_counts_and_session_revision_before_execution() {
        let mut thresholds = default_thresholds();
        thresholds.max_tool_calls_per_session = 1;
        let counters = AutoModeCounters {
            tool_calls: 1,
            ..AutoModeCounters::default()
        };
        assert_eq!(
            authorize_command(
                "pwd",
                AiCommandRisk::ReadOnly,
                &counters,
                &thresholds,
                Some("1"),
                Some("2"),
            )
            .unwrap_err()
            .code,
            AUTO_MODE_TARGET_CHANGED
        );
        assert_eq!(
            authorize_command(
                "pwd",
                AiCommandRisk::ReadOnly,
                &counters,
                &thresholds,
                Some("1"),
                Some("1"),
            )
            .unwrap_err()
            .code,
            AUTO_MODE_SESSION_LIMIT_REACHED
        );
    }

    #[test]
    fn reserves_count_and_risk_budget_before_execution_finishes() {
        let mut thresholds = default_thresholds();
        thresholds.max_tool_calls_per_session = 1;
        let mut counters = AutoModeCounters::default();
        assert!(authorize_command(
            "pwd",
            AiCommandRisk::ReadOnly,
            &counters,
            &thresholds,
            None,
            None,
        )
        .is_ok());
        reserve_execution(&mut counters, AiCommandRisk::ReadOnly);
        assert_eq!(counters.tool_calls, 1);
        assert_eq!(
            authorize_command(
                "id",
                AiCommandRisk::ReadOnly,
                &counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_SESSION_LIMIT_REACHED
        );

        thresholds.max_tool_calls_per_session = default_thresholds().max_tool_calls_per_session;
        thresholds.max_destructive_calls_per_session = 1;
        let mut destructive_counters = AutoModeCounters::default();
        reserve_execution(&mut destructive_counters, AiCommandRisk::Destructive);
        assert_eq!(destructive_counters.destructive_calls, 1);
        assert_eq!(
            authorize_command(
                "rm /tmp/fileterm",
                AiCommandRisk::Destructive,
                &destructive_counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_RISK_LIMIT_REACHED
        );

        thresholds.max_privileged_calls_per_session = 1;
        let mut privileged_counters = AutoModeCounters::default();
        reserve_execution(&mut privileged_counters, AiCommandRisk::Privileged);
        assert_eq!(privileged_counters.privileged_calls, 1);
        assert_eq!(
            authorize_command(
                "sudo systemctl restart ssh",
                AiCommandRisk::Privileged,
                &privileged_counters,
                &thresholds,
                None,
                None,
            )
            .unwrap_err()
            .code,
            AUTO_MODE_RISK_LIMIT_REACHED
        );
    }

    #[test]
    fn record_execution_updates_risk_and_duration_counters() {
        let mut counters = AutoModeCounters::default();
        record_execution(
            &mut counters,
            AiCommandRisk::Privileged,
            Duration::from_millis(1_001),
        );
        assert_eq!(counters.tool_calls, 1);
        assert_eq!(counters.privileged_calls, 1);
        assert_eq!(counters.total_exec_duration_secs, 2);
    }

    #[test]
    fn threshold_validation_preserves_the_default_safety_floor() {
        let defaults = default_thresholds();
        assert!(validate_thresholds(&defaults).is_ok());

        let mut lowered = defaults.clone();
        lowered.max_tool_calls_per_session -= 1;
        assert!(validate_thresholds(&lowered).is_err());

        let mut raised = defaults;
        raised.max_tool_calls_per_session += 10;
        raised.max_total_exec_duration_secs += 60;
        assert!(validate_thresholds(&raised).is_ok());
    }
}
