//! Process-level contracts for the non-GUI FileTerm entry points.
//!
//! These tests deliberately point the bridge at a missing local runtime file,
//! so they exercise argument parsing, stdio, exit codes, and secret handling
//! without opening a desktop window or contacting a remote machine.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn fileterm_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_fileterm")
        .map(PathBuf::from)
        .expect("Cargo must provide the FileTerm binary for process tests")
}

fn run_fileterm(arguments: &[&str], input: Option<&[u8]>) -> Output {
    let missing_runtime = std::env::temp_dir().join(format!(
        "fileterm-cli-contract-{}-missing-runtime.json",
        std::process::id()
    ));
    let mut command = Command::new(fileterm_binary());
    command
        .args(arguments)
        .env("FILETERM_MCP_RUNTIME_FILE", missing_runtime)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let mut child = command.spawn().expect("FileTerm process should start");
    if let Some(input) = input {
        let mut stdin = child.stdin.take().expect("stdin should be piped");
        stdin
            .write_all(input)
            .expect("test input should reach FileTerm");
    }
    child
        .wait_with_output()
        .expect("FileTerm process should exit cleanly")
}

#[test]
fn cli_help_is_headless_and_keeps_stdout_clean() {
    let output = run_fileterm(&["cli", "--help"], None);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("FileTerm CLI"));
    assert!(output.stderr.is_empty());
}

#[test]
fn agent_help_documents_one_process_and_cancellation() {
    let output = run_fileterm(&["agent", "--help"], None);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("persistent FileTerm Agent bridge"));
    assert!(stdout.contains("cancel_request"));
    assert!(output.stderr.is_empty());
}

#[test]
fn agent_returns_a_final_jsonl_result_without_starting_the_gui() {
    let output = run_fileterm(
        &["agent"],
        Some(
            br#"{"id":"request-1","action":"list_connections","params":{}}
"#,
        ),
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("Agent should emit one final JSON response");
    assert_eq!(response["id"], "request-1");
    assert_eq!(response["ok"], false);
    assert!(response["error"]
        .as_str()
        .expect("Agent error should be text")
        .contains("desktop app is not running"));
}

#[test]
fn cli_rejects_mixed_password_sources_without_leaking_argv_secret() {
    let output = run_fileterm(
        &[
            "exec",
            "--tab-id",
            "tab-1",
            "--command",
            "sudo id",
            "--sudo-password",
            "argv-secret",
            "--sudo-password-stdin",
        ],
        None,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("either --sudo-password or --sudo-password-stdin"));
    assert!(!stderr.contains("argv-secret"));
}

#[test]
fn cli_consumes_stdin_password_without_leaking_it_to_bridge_errors() {
    let output = run_fileterm(
        &[
            "exec",
            "--tab-id",
            "tab-1",
            "--command",
            "sudo id",
            "--sudo-password-stdin",
        ],
        Some(b"stdin secret with spaces\n"),
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("stdin secret with spaces"));
    assert!(stderr.contains("desktop app is not running"));
}
