# SSH session module split

Status: in progress

## Context

`apps/tauri/src-tauri/src/sessions/ssh.rs` had grown beyond 12,000 lines and
mixed transport setup, authentication, interactive MFA, shell state,
privilege escalation, SFTP I/O, transfer handling, tunnels, and tests. This
makes a change in one protocol concern disproportionately difficult to review
and raises the risk of unrelated regressions.

The B-3148 jump-host MFA fix is intentionally implemented before this
refactor: it keeps the same SSH handle when authentication progresses to
keyboard-interactive, so no host-key verification or partial-auth state is
lost. The split must preserve that behavior exactly.

## Public boundary

`sessions/ssh/mod.rs` is the only public facade. Current in-repository callers
require seven exports (four public and three crate-visible), rather than the
five initially proposed:

- `start_ssh_worker`
- `test_connection`
- `list_dir`
- `format_unix_ts`
- `shell_cwd_sftp_path_candidates`
- `is_sftp_path_not_found_message`
- `effective_remote_file_type`

No protocol implementation type should become newly public just to move it to
another file.

## Target layout

The final directory is organized by protocol responsibility:

```text
sessions/ssh/
  mod.rs
  constants.rs / types.rs
  capabilities.rs
  device.rs
  metrics.rs
  codec.rs
  transport.rs
  auth.rs
  handler.rs
  shell_setup.rs
  root_access.rs
  shell_exec.rs
  tunnel.rs
  sftp_files.rs
  transfer_io.rs
  worker/{loop,dispatch,terminal,output}.rs
  tests.rs
```

`transfer_io.rs` deliberately avoids the name `transfers.rs`, which would be
confusing beside `services/transfers.rs`.

## Delivery stages

### Stage 0 — test extraction

- [x] Move the complete `mod tests` block to `ssh/tests.rs`.
- [x] Keep it as a child test module using `use super::*`; do not widen
      production visibility for tests.
- [x] Run the complete Rust test suite after moving it.

### Stage 1 — source ownership and leaf concerns

- [x] Replace `sessions/ssh.rs` with the `sessions/ssh/` directory facade.
- [x] Separate the current source by responsibility (device mode, runtime,
      tunnel, shell, transport, authentication, SFTP, worker, and files) without
      changing executable content.
- [x] Update the source-layout contract test so it checks the actual SSH
      worker fragments instead of hard-coding the old filename.
- [ ] Promote the self-contained leaf fragments to independent Rust modules:
      capabilities, device, metrics, codec, and tunnel.

### Stage 2 — authentication and shell state boundaries

- [x] Keep password/key/agent/keyboard-interactive authentication together,
      including the B-3148 same-handle MFA continuation, the distinction between
      `partial_success` MFA continuation and normal KBI fallback, and bounded
      multi-round restart.
- [ ] Make `auth.rs` the sole owner of that chain once the independent Rust
      module boundaries are introduced.
- [ ] Extract shell setup and root-access state into explicit structs owned by
      the worker context; do not change prompts, CWD tracking, or sudo/su
      semantics.

### Stage 3 — worker dispatch

- [ ] Introduce `SshSessionContext` for the worker-owned handle, optional
      SFTP session, shell state, root-access state, and transfer state.
- [ ] Split initialization, main loop, output processing, and command
      dispatch; dispatch delegates terminal, tunnel, transfer, file, access, and
      exec groups.
- [ ] Treat the duplicated no-SFTP dispatch path as a separate, behaviorally
      covered follow-up. It should not be merged by changing every file-operation
      `None` semantic in the same refactor.

## Safety and verification

- Preserve private module boundaries; do not make internals `pub` merely to
  satisfy file placement.
- Preserve the Rust command/event → bridge → renderer boundary.
- Keep the current special Comware compatibility boundary in vendored russh
  unchanged.
- After each stage, run `cargo fmt --check`, focused SSH tests, the full
  Tauri suite, and Clippy with warnings denied. The final change also runs the
  renderer typecheck, lint, and formatting checks.
