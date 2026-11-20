# Server overview command batches

- Select connected SSH servers in the overview list. Use one exec-capable session per connection profile.
- Confirm the saved command template, parameters, and targets in one dialog. Reuse the Rust template interpolation and isolated SSH exec channel.
- Collect per-host output, execution state and exit code. Preserve the target identity across the request and skip stale sessions.
- Keep recent runs and per-host results in a bounded local history. Keep the rendered commands, arguments, remote output and raw errors out of persistent history.
- Verify target selection, argument binding, partial failures, history and the current overview filters.
