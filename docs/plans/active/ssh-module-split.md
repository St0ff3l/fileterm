# SSH session module split

Status: in progress

## Context

The historical `apps/tauri/src-tauri/src/sessions/ssh.rs` had grown beyond
12,000 lines and mixed transport setup, authentication, interactive MFA, shell
state, privilege escalation, SFTP I/O, transfer handling, tunnels, and tests.
The implementation now lives under `sessions/ssh/`; this plan tracks the
remaining ownership and context work.

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
confusing beside `services/transfers/mod.rs`.

## Current physical layout

The first physical split keeps the existing `session.rs` assembly and private
visibility intact. The former large files are now directory-local facades:

```text
sessions/ssh/
  files.rs                  # facade
  sftp_files.rs             # SFTP listing and basic file operations
  transfer_io.rs            # normal SFTP transfer I/O
  root_transfer.rs          # privileged staging and stream transfers
  shell_exec.rs             # privileged shell-backed file operations
  shell.rs                  # facade
  shell/{cwd,root_access,shell_setup,encoding}.rs
  authentication.rs         # facade
  authentication/{common,primary,keyboard_interactive}.rs
  constants.rs              # session/operation limits and fallback text
  transport.rs              # facade
  transport/{host,jump,proxy,credentials,session}.rs
```

These are still `include!` fragments rather than independent Rust child
modules. That is deliberate for this stage: existing private helpers keep the
same visibility and the B-3148 same-Handle MFA flow remains unchanged.

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
- [x] Establish the nested worker directory facade with physical fragments for
      the loop, remote-exec, no-SFTP, and dispatch responsibilities; keep the
      deeper `SshSessionContext` and dispatch ownership work for Stage 3.
- [x] Split the former `files.rs`, `shell.rs`, `authentication.rs`, and
      `transport.rs` bodies into directory-local responsibility fragments;
      keep the original files as compatibility facades and keep every new
      production fragment below 800 lines.
- [x] Update the source-layout contract test so it checks the actual SSH
      worker fragments instead of hard-coding the old filename.
- [ ] Promote the self-contained leaf fragments to independent Rust modules:
      capabilities, device, metrics, codec, and tunnel.

### Stage 2 — authentication and shell state boundaries

- [x] Keep password/key/agent/keyboard-interactive authentication together,
      including the B-3148 same-handle MFA continuation, the distinction between
      `partial_success` MFA continuation and normal KBI fallback, and bounded
      multi-round restart.
- [x] Move the session-wide timeout and SFTP fallback constants out of the
      authentication body into the directory-local `constants.rs` fragment.
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

## Progress notes

- 2026-08-31：完成四个 SSH 大文件的首轮物理职责拆分。`files.rs`、`shell.rs`、
  `authentication.rs` 和 `transport.rs` 均降为 facade；实现分别进入 SFTP/传输/
  root exec、CWD/root/setup/encoding、主认证/KBI、以及 host/jump/proxy/credential/
  session 片段。新增业务片段均低于 800 行，独立 Rust module、`SshSessionContext`
  和 worker dispatch 收口仍留在后续阶段。
