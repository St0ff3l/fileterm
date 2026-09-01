//! Process-level contracts for the non-GUI FileTerm entry points.
//!
//! These tests deliberately point the bridge at a missing local runtime file,
//! so they exercise argument parsing, stdio, exit codes, and secret handling
//! without opening a desktop window or contacting a remote machine.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::net::{TcpListener, TcpStream};
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
    run_fileterm_with_runtime(arguments, input, missing_runtime)
}

fn run_fileterm_with_runtime(
    arguments: &[&str],
    input: Option<&[u8]>,
    runtime_path: PathBuf,
) -> Output {
    let mut command = Command::new(fileterm_binary());
    command
        .args(arguments)
        .env("FILETERM_MCP_RUNTIME_FILE", runtime_path)
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FileTerm CLI"));
    assert!(stdout.contains("start-remote-command"));
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_jsonl_help_documents_one_process_and_cancellation() {
    let output = run_fileterm(&["cli", "--jsonl", "--help"], None);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("persistent FileTerm CLI JSONL bridge"));
    assert!(stdout.contains("cancel_request"));
    assert!(output.stderr.is_empty());
}

#[test]
fn cli_jsonl_reuses_one_process_for_multiple_jsonl_requests_without_starting_the_gui() {
    let output = run_fileterm(
        &["cli", "--jsonl"],
        Some(
            br#"{"id":"request-1","action":"list_connections","params":{}}
{"id":"request-2","action":"list_connections","params":{}}
"#,
        ),
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut responses = stdout
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("CLI JSONL response should be JSON")
        })
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    responses.sort_by_key(|response| response["id"].as_str().unwrap_or_default().to_string());
    assert_eq!(responses[0]["id"], "request-1");
    assert_eq!(responses[1]["id"], "request-2");
    for response in responses {
        assert_eq!(response["ok"], false);
        let error = response["error"]
            .as_str()
            .expect("CLI JSONL error should be text");
        assert!(error.contains("desktop app is not running") || error.contains("CANCELLED"));
    }
}

#[test]
fn cli_jsonl_reuses_one_authenticated_bridge_session_for_multiple_requests() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("test bridge should bind");
    let address = listener
        .local_addr()
        .expect("test bridge should expose an address");
    let runtime_path = std::env::temp_dir().join(format!(
        "fileterm-cli-contract-{}-bridge-runtime.json",
        std::process::id()
    ));
    std::fs::write(
        &runtime_path,
        serde_json::to_vec(&json!({
            "protocolVersion": 2,
            "address": address.to_string(),
            "token": "test-token"
        }))
        .expect("test runtime descriptor should encode"),
    )
    .expect("test runtime descriptor should be written");

    let server = std::thread::spawn(move || {
        let (stream, _) = listener
            .accept()
            .expect("test bridge should accept exactly one session");
        let reader_stream = stream.try_clone().expect("test bridge should clone reader");
        let mut reader = BufReader::new(reader_stream);
        let mut writer = BufWriter::new(stream);
        let hello = read_bridge_frame(&mut reader);
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["protocolVersion"], 2);
        assert_eq!(hello["token"], "test-token");
        write_bridge_frame(
            &mut writer,
            json!({
                "type": "helloAck",
                "protocolVersion": 2,
                "sessionId": "integration-session",
                "error": null
            }),
        );

        let mut request_ids = Vec::new();
        for _ in 0..2 {
            let request = read_bridge_frame(&mut reader);
            assert_eq!(request["type"], "request");
            assert_eq!(request["request"]["action"], "list_connections");
            request_ids.push(
                request["requestId"]
                    .as_str()
                    .expect("bridge request should carry an id")
                    .to_string(),
            );
        }
        for request_id in request_ids.into_iter().rev() {
            write_bridge_frame(
                &mut writer,
                json!({
                    "type": "response",
                    "requestId": request_id,
                    "response": {
                        "ok": true,
                        "result": { "connections": [] }
                    }
                }),
            );
        }
    });

    let mut command = Command::new(fileterm_binary());
    command
        .args(["cli", "--jsonl"])
        .env("FILETERM_MCP_RUNTIME_FILE", runtime_path.clone())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("FileTerm process should start");
    let mut child_stdin = child.stdin.take().expect("stdin should be piped");
    child_stdin
        .write_all(
            br#"{"id":"request-1","action":"list_connections","params":{}}
{"id":"request-2","action":"list_connections","params":{}}
"#,
        )
        .expect("test input should reach FileTerm");
    let child_stdout = child.stdout.take().expect("stdout should be piped");
    let mut child_stdout = BufReader::new(child_stdout);
    let mut response_lines = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        child_stdout
            .read_line(&mut line)
            .expect("CLI should emit a response");
        response_lines.push(line);
    }
    drop(child_stdout);
    drop(child_stdin);
    let status = child.wait().expect("FileTerm process should exit cleanly");
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .expect("stderr should be piped")
        .read_to_string(&mut stderr)
        .expect("stderr should be readable");
    let _ = std::fs::remove_file(runtime_path);

    assert!(status.success());
    assert!(stderr.is_empty());
    let mut responses = response_lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("CLI response should be JSON"))
        .collect::<Vec<_>>();
    responses.sort_by_key(|response| response["id"].as_str().unwrap_or_default().to_string());
    assert_eq!(responses.len(), 2);
    assert_eq!(responses[0]["id"], "request-1");
    assert_eq!(responses[1]["id"], "request-2");
    assert!(responses.iter().all(|response| {
        response["ok"] == true && response["result"] == json!({ "connections": [] })
    }));
    server.join().expect("test bridge should join");
}

fn read_bridge_frame(reader: &mut BufReader<TcpStream>) -> Value {
    let mut line = String::new();
    let count = reader
        .read_line(&mut line)
        .expect("test bridge should read a frame");
    assert!(
        count > 0,
        "test bridge should not reach EOF before the frame"
    );
    serde_json::from_str(&line).expect("test bridge frame should be JSON")
}

fn write_bridge_frame(writer: &mut BufWriter<TcpStream>, frame: Value) {
    serde_json::to_writer(&mut *writer, &frame).expect("test bridge frame should encode");
    writer
        .write_all(b"\n")
        .expect("test bridge should write a newline");
    writer.flush().expect("test bridge should flush");
}

#[test]
fn removed_agent_command_is_rejected_without_starting_the_gui() {
    let output = run_fileterm(&["agent", "--help"], None);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown FileTerm CLI command: agent"));
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
