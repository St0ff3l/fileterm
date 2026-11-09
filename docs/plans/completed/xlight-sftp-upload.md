# Xlight SFTP upload diagnostics

Refs #369. FTPRush can upload both ordinary names and `.fileterm-part` names, while FileTerm reports an unqualified permission error at zero progress.

- Add operation, remote path, open flags and write offset to upload errors, including checkpoint STAT and final replacement.
- Accommodate older Xlight STAT permission behavior using successful directory listings. Use only exact, non-symlink entries with a size for file metadata. Preserve the original denial when a listing cannot confirm the file.
- Create parent directories only after a missing-path response, preserving permission and transport failures.
- Verify the fallback and error reporting with an in-process SFTP server, then run repository quality gates.

The official Xlight changelog for 3.8.5.1 records that STAT/LSTAT previously required READ permission even when LIST was allowed: <https://www.xlightftpd.com/whatsnew.htm>.

Real Xlight validation remains necessary to confirm the failing operation in the reported environment.

## Completion

Implemented operation/path context for STAT, OPEN, resume READ, WRITE, flush/close, and final replacement. WRITE errors include the submitted offset and chunk size; OPEN errors include flags and resume offset. Existing transfer failure logging and task details retain this context.

The listing fallback is restricted to permission-denied STAT responses. It accepts only a confirmed regular file with a reported size. Filtered/missing entries, symlinks and failed listings preserve the denial. Directory creation is attempted only after a missing-path response.

Validation passed with typecheck, lint, Prettier, 249 renderer tests, 969 Rust unit tests (one pre-existing ignored test), 10 CLI tests, 24 contract tests and all-target/all-feature Clippy. Two added in-process SFTP regression tests cover listing fallback safety and upload OPEN/WRITE diagnostics.

The actual Xlight instance was not available. This change addresses the documented permission-model difference, but does not establish that it caused #369. A denied STAT of an unlisted checkpoint remains an error to avoid treating a hidden existing file as absent.
