---
name: fileterm-cli
description: Use FileTerm's one-shot CLI, persistent CLI JSONL bridge, or MCP bridge from an external AI Agent while the FileTerm desktop app is running. Read this Skill whenever an AI Agent or automation needs to discover connections, execute remote actions, transfer files, or use the FileTerm bridge safely.
---

# FileTerm CLI Skill

This document explains how end users integrate with FileTerm. It applies to version 2.2.7 and later. For the complete set of command arguments, use the `--help` output from the installed version.

## Choose an integration mode

| Use case                                  | How to start              | Process behavior                                                |
| ----------------------------------------- | ------------------------- | --------------------------------------------------------------- |
| Default integration for an external AI Agent | `fileterm cli --jsonl`   | Starts once, keeps stdin/stdout open, and handles many requests |
| MCP-capable client                        | `fileterm mcp`            | Starts once and keeps an MCP stdio session                      |
| Manual use or shell scripts               | `fileterm cli <command>` | Starts a short-lived CLI process for each call                  |

There is no separate `fileterm agent` command. AI Agents use `fileterm cli --jsonl`; do not restart the one-shot CLI for every action.

The FileTerm desktop app must already be running. The CLI and MCP are local bridge clients that connect to the running FileTerm app; they are not standalone SSH clients, do not export connection credentials, and do not automatically change external-client configuration. CLI arguments are processed before the Tauri GUI initializes, so a CLI call does not open another FileTerm window. A one-shot CLI call still creates its own short-lived operating-system process; only JSONL mode reuses the same CLI process.

Connections opened through the CLI or MCP remain owned by the FileTerm app's SSH/SFTP worker. Background mode does not place connections in the top tab bar; instead, they appear on the GUI's **Background Sessions** page. The session ID in that list is the `tabId` to use in later CLI or MCP requests. Selecting **Open Session** or calling `fileterm_activate_session` reuses the original worker and returns it to a visible tab without reconnecting.

The Background Sessions list labels each session's source as `CLI` or `MCP`. After a session is opened in the workspace, the same source label remains next to its session ID at the bottom. Sessions created in the regular GUI do not display an external source.

Prompts for external actions that require user confirmation also show the request source, `CLI` or `MCP`. This source is only for identification and auditing; the CLI and MCP share the same permission policy.

You can move a visible CLI or MCP session back to the background by selecting the hide control beside its session ID at the bottom. This only changes where the session is displayed; it does not disconnect. You can open it again from **Background Sessions** later.

## AI Agent: persistent JSONL

Start it once:

```text
fileterm cli --jsonl
```

Then send one JSON object per line to stdin and read one result per line from stdout:

```json
{"id":"request-1","action":"list_connections","params":{}}
{"id":"request-2","action":"get_session_context","params":{"profile_id":"PROFILE_ID"}}
```

Example successful result:

```json
{ "id": "request-1", "ok": true, "result": { "connections": [] } }
```

While waiting for the user to confirm an action or enter a password in the main FileTerm window, you may first receive a progress line with the same request ID, followed by the result line. To cancel a request that is still waiting:

```json
{ "id": "cancel-1", "action": "cancel_request", "params": { "request_id": "request-1" } }
```

Cancellation only stops the CLI JSONL wait and any later output; it does not roll back remote actions that FileTerm has already accepted or started. A request ID must be a non-empty string or number, and it cannot be reused by another active request. Each input line is limited to 2 MiB, with up to eight concurrent requests. The bridge process exits when stdin closes.

## Manual CLI

Show help:

```text
fileterm cli --help
```

Common one-shot calls:

```text
fileterm cli connections
fileterm cli open --profile-id PROFILE_ID --wait-for-ready true
fileterm cli sessions --profile-id PROFILE_ID
fileterm cli directory --tab-id TAB_ID --path /
fileterm cli read --tab-id TAB_ID --path /etc/hostname
fileterm cli exec --tab-id TAB_ID --command "uname -a"
```

By default, CLI `open` creates a background session and returns a reusable session ID in `sessionId` (while retaining `tabId` for compatibility with existing requests). To view the terminal in the top tab bar, open it from the GUI's **Background Sessions** page or call `fileterm_activate_session` through MCP.

The one-shot CLI is intended for manual debugging and shell scripts. Every call creates a new CLI process and exits when the call finishes; it does not reuse a CLI process and should not be used by an AI Agent for per-action calls.

## Permissions and confirmation

MCP, the one-shot CLI, and CLI JSONL share the connection scope, operation permissions, and security checks configured in FileTerm:

- **Read-only**: Allows only queries for connections, sessions, directories, files, and transfer status. It does not run commands or make changes.
- **Basic safe operations**: Queries and ordinary commands that FileTerm identifies as read-only run automatically. Changes, dangerous or privileged commands, session changes, file operations, transfers, tunnels, and unknown operations require confirmation in the main FileTerm window.
- **Full access**: Skips per-operation confirmation, including for `sudo` and `su`. Connection allowlists, protocol capabilities, security checks, and any required password entry still apply.

The `requiresApproval` field in a CLI JSONL request cannot disable desktop approval. Passwords are never included in command text, logs, or results. When a one-shot CLI call needs a password, prefer a stdin option such as `--sudo-password-stdin` or `--su-password-stdin`.

## Remote-command boundaries

- Commands on ordinary SSH servers use a separate non-interactive exec channel and do not write to the visible terminal.
- Network-device commands send a single command through the visible raw terminal. Results may include the command echo and prompt. Background exec, `cwd`, `sudo`, and `su` are not available.
- MFA, installer confirmation, REPLs, and other operations that need ongoing interactive input must be completed in the visible SSH terminal. Open the corresponding background session or call `fileterm_activate_session` before continuing. Do not automatically retry when FileTerm returns `REMOTE_INTERACTIVE_INPUT_REQUIRED`.
- `sudo` and `su` may wait for a secure password entry in the main FileTerm window. The original request resumes after the user provides it.

## Client configuration

External clients should register `fileterm mcp` or `fileterm cli --jsonl` and reuse the started stdio process. FileTerm only provides a copyable registration command in Settings; it does not start external clients or rewrite their configuration files automatically.
