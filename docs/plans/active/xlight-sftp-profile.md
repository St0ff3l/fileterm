# Xlight SFTP profile

Refs #369. Add a standalone SFTP connection for servers that expose SFTP over SSH2 without an interactive shell, and support legacy GBK filenames.

- Add a persisted SFTP profile type that shares SSH authentication and network routing but exposes a file-only workspace and no shell monitoring.
- Route SFTP profiles through SSH transport, open the SFTP subsystem directly, and dispatch file commands without requesting a PTY or shell.
- Add UTF-8/GBK selection for SSH-backed file sessions so received names decode correctly and requested paths use the selected encoding; keep opaque handles and protocol status strings UTF-8.
- Remove the connection form footer override that erased the gap above its divider.
- Static/type/format checks pass. The common-component CSS contract still reports its existing legacy color aliases in `feedback-bubble.css`.
- Real Xlight SFTP end-to-end validation remains pending on the user's Windows server.

## Upload without DELETE permission (Refs #369)

- A fresh single-file upload previously sent REMOVE for its checkpoint before checking whether it existed. Xlight can reject this request on permission grounds even when there is nothing to delete.
- Cleanup now uses LSTAT first and skips REMOVE only for a confirmed missing path. An existing checkpoint or an inconclusive permission error still fails safely; it is not silently treated as missing.
- Cleanup failures include the SFTP operation, path and server error. Upload permission feedback preserves these details and no longer asserts that every permission failure means missing directory write access. Backup cleanup failures after a successful commit are logged as warnings.
- Regression coverage includes a server that denies every REMOVE request: a new checkpoint is created, uploaded and renamed to its final name without issuing REMOVE. Existing checkpoint denial and metadata denial remain actionable errors.
- Creating a new target still requires the server to permit upload and rename. Cleaning an existing checkpoint requires delete permission; overwriting an existing destination can require additional server permissions. This change does not bypass those checks.
- Validation: typecheck, lint, formatting, CSS contract, renderer tests (254 existing + 1 focused banner regression), Rust/contract suite (974 unit tests + 34 integration tests, 1 ignored), and strict Clippy pass. One unrelated terminal-output timing test failed on the first full run, passed in isolation, and passed in the subsequent full run. Real Windows Xlight validation remains pending.

## Automatic legacy SFTP filename compatibility

- Default UTF-8 SFTP sessions fall back to GBK only after a NAME response fails lossless UTF-8 decoding and succeeds with lossless GBK decoding. The detected encoding is shared by decoding and outgoing path requests, and inherited by dedicated transfer channels.
- NAME responses that cannot round-trip in either encoding fail immediately with a filename-encoding error. Opaque handles and file contents are not transcoded.
- Manual GBK remains fixed. Automatic detection assumes a consistent server encoding; byte sequences valid in both encodings remain ambiguous and prefer UTF-8 before GBK has been detected.
- Wire fixtures cover listing/opening Chinese directories and child paths with default UTF-8 against UTF-8/GBK servers, explicit GBK, and invalid-byte failures. Real Xlight verification remains pending.
- Final local validation: typecheck, lint, Prettier, 255 renderer tests, 976 Rust unit tests and 34 integration/contract tests pass (1 ignored); strict Clippy passes. The prior DELETE-permission regressions pass in the same full suite.

## Restricted SFTP permissions

- WinSCP's documented temporary-file resume flow preserves the target until commit; its direct-write mode has different failure behavior. FileTerm continues to avoid silently switching to destructive in-place overwrite. Reference: <https://winscp.net/eng/docs/resume>.
- The per-profile `sftpNoDelete` option prevents committing over an existing destination; the original target and resumable checkpoint are retained with an actionable error. New targets remain supported. The default remains off for existing profiles.
- Existing checkpoints still need Delete permission for fresh-task cleanup. If backup deletion is denied with the option off, the operation explicitly reports that the target was updated and identifies the retained backup; it must not be retried as an uncommitted upload. The current transfer status is failed despite the committed target, and its message and banner clarify that distinction.
- SFTP permission denials include the operation, path and original server error, plus guidance for read, write/resume, directory creation, listing, metadata, deletion and rename. Download metadata reuses the existing LIST fallback when STAT is denied and a regular file can be confirmed.
- A directory entry appearing concurrently between the destination LSTAT and the SFTP rename relies on the server respecting SFTP v3's non-overwriting rename behavior. Servers that replace an existing destination on ordinary RENAME cannot provide a client-side guarantee for that race without a server-side exclusive commit primitive.
