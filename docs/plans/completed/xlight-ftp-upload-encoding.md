# Xlight FTP upload and filename encoding

Refs #369. A real FTP test on Xlight failed before sending bytes because a nonexistent checkpoint was deleted and Xlight returned a generic 550. The directory listing also exposed a GBK filename decoded as UTF-8.

- Add an explicit UTF-8/GBK FTP filename-encoding selector, persisted through core types, storage and edit forms. Apply it before login and negotiate OPTS UTF8 ON/OFF consistently on browsing and transfer connections.
- The server must retain GBK and rejects OPTS UTF8. Vendor the locked suppaftp 10.0.2 package with a narrow Tokio command/reply/listing codec patch, reusing encoding_rs already present in FileTerm. Do not transcode file payloads. Reject unrepresentable command paths.
- Confirm existing directories after Xlight MKD 521 failures using CWD and restore the original working directory.
- Verify ambiguous 550 missing-file replies against a successfully parsed parent directory listing for checkpoint cleanup, size lookup and replacement. Keep errors for listed entries, unreadable directories and malformed listings.
- Ignore MLSD cdir/pdir navigation records even when their names are absolute paths. This is needed for trustworthy directory checks and safe cleanup.
- Add protocol regression fixtures, run quality gates and repeat real FTP uploads/downloads and Unicode directory checks through FileTerm.

The SSH2/SFTP endpoint is not yet enabled in the user's Xlight setup, so these FTP tests do not validate #369's SFTP failure.

## Completion

Implemented the missing-path and existing-directory compatibility handling, MLSD navigation-record filtering, and the persisted FTP filename-encoding selector. The locked Tokio FTP dependency now encodes command text and decodes control/listing text using the selected codec. Default UTF-8 behavior and binary file payloads are preserved.

Validation passed with typecheck, lint, Prettier, renderer tests, 972 Rust unit tests (one pre-existing ignored test), 10 CLI tests, 24 contract tests and all-target/all-feature Clippy. The added protocol fixtures exercise UTF-8 and GBK round trips, Chinese paths, new-file/overwrite commits, ambiguous failures, denied listings, and rejection of unrepresentable command text.

Real Xlight FTP upload completed successfully after previously failing at 0 B. The user confirmed the GBK display was satisfactory. A real download/checksum round trip and SSH2/SFTP testing remain outstanding. The test directory `/fileterm-xlight-test-20261109-1034` and failed-transfer history were retained for follow-up.

CSS contract checks still report the pre-existing danger color aliases in feedback-bubble.css. The desktop automation inventory cannot bind the unpackaged development process precisely, so the user performed the GBK selection in the normal connection form.
