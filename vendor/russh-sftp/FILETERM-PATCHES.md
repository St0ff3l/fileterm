# FileTerm SFTP filename encoding patches

- Path-bearing requests and NAME responses use the configured filename encoding. File payloads, opaque handles, status text and extension identifiers retain their original protocol encoding.
- UTF-8 sessions try GBK when a NAME response cannot round-trip through UTF-8. The whole response must decode and re-encode without replacing or changing any string bytes before GBK is remembered for subsequent requests.
- The effective session encoding is exposed so FileTerm's separate transfer channel inherits detected GBK. Explicit GBK remains fixed.
- Invalid NAME strings return a request-scoped BadMessage error instead of replacement characters or a request timeout.
- Detection assumes consistent server filename encoding. Bytes valid in both encodings cannot be reliably distinguished; UTF-8 takes precedence until GBK is confirmed. Manual GBK remains available for ambiguous servers.

Wire fixtures live in `apps/tauri/src-tauri/src/sessions/ssh/tests/filename_encoding.rs`. They verify decoded directory/file names, outgoing path bytes, unchanged Unicode handles, UTF-8/GBK compatibility, and invalid-byte failures.
