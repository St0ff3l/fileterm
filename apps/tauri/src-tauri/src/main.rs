#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.get(1).is_some_and(|argument| argument == "mcp") {
        if let Err(error) = fileterm_lib::run_mcp_stdio(&arguments[2..]) {
            eprintln!("FileTerm MCP failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    if matches!(
        arguments.get(1).map(String::as_str),
        Some(
            "cli"
                | "connections"
                | "sessions"
                | "directory"
                | "ls"
                | "read"
                | "cat"
                | "commands"
                | "command-templates"
                | "transfers"
                | "tunnels"
                | "open"
                | "activate"
                | "reconnect"
                | "disconnect"
                | "close"
                | "exec"
                | "execute"
                | "command-template"
                | "write"
                | "mkdir"
                | "touch"
                | "copy"
                | "move"
                | "rename"
                | "delete"
                | "chmod"
                | "access"
                | "upload"
                | "download"
                | "download-directory"
                | "pause-transfer"
                | "resume-transfer"
                | "discard-transfer"
                | "cancel-transfer"
                | "clear-transfers"
                | "create-tunnel"
                | "start-tunnel"
                | "stop-tunnel"
                | "delete-tunnel"
                | "call"
                | "help"
                | "--help"
                | "-h"
                | "--version"
                | "-V"
        )
    ) {
        if let Err(error) = fileterm_lib::run_cli(&arguments[1..]) {
            eprintln!("FileTerm CLI failed: {error}");
            std::process::exit(1);
        }
        return;
    }
    fileterm_lib::run()
}
