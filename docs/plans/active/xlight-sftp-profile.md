# Xlight SFTP profile

Refs #369. Add a standalone SFTP connection for servers that expose SFTP over SSH2 without an interactive shell, and support legacy GBK filenames.

- Add a persisted SFTP profile type that shares SSH authentication and network routing but exposes a file-only workspace and no shell monitoring.
- Route SFTP profiles through SSH transport, open the SFTP subsystem directly, and dispatch file commands without requesting a PTY or shell.
- Add UTF-8/GBK selection for SSH-backed file sessions so received names decode correctly and requested paths use the selected encoding; keep opaque handles and protocol status strings UTF-8.
- Remove the connection form footer override that erased the gap above its divider.
- Static/type/format checks pass. The common-component CSS contract still reports its existing legacy color aliases in `feedback-bubble.css`.
- Real Xlight SFTP end-to-end validation remains pending on the user's Windows server.
