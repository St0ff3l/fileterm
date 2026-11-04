use super::*;
use std::process::Command;

fn launchers() -> Vec<Command> {
    ["xdg-open", "gio", "gnome-open"]
        .into_iter()
        .map(Command::new)
        .collect()
}

#[test]
fn failed_installed_launcher_falls_back_and_stops_after_success() {
    let mut attempts = Vec::new();
    let result = try_launchers(launchers(), |command| {
        let name = command.get_program().to_string_lossy().into_owned();
        attempts.push(name.clone());
        if name == "xdg-open" {
            Err("exit status: 4".into())
        } else {
            Ok(())
        }
    });
    assert!(result.is_ok());
    assert_eq!(attempts, ["xdg-open", "gio"]);
}

#[test]
fn missing_launchers_return_actionable_error() {
    let result = try_launchers(launchers(), |_| Err("tool not found".into()));
    let error = result.unwrap_err().to_string();
    assert!(error.contains("xdg-utils"));
    assert!(error.contains("xdg-open: tool not found"));
    assert!(error.contains("gio: tool not found"));
}

#[test]
fn real_nonzero_exit_falls_back_to_successful_process() {
    fn exit_command(code: &str) -> Command {
        #[cfg(windows)]
        let mut command = Command::new("cmd");
        #[cfg(windows)]
        command.args(["/C", "exit", code]);
        #[cfg(not(windows))]
        let mut command = Command::new("sh");
        #[cfg(not(windows))]
        command.args(["-c", &format!("exit {code}")]);
        command
    }
    let mut attempts = 0;
    let result = try_launchers(vec![exit_command("4"), exit_command("0")], |command| {
        attempts += 1;
        run_launcher(command)
    });
    assert!(result.is_ok());
    assert_eq!(attempts, 2);
}
