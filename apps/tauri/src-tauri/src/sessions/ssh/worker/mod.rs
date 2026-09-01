// SSH worker implementation fragments.
//
// The fragments are included into the existing `sessions::ssh::session`
// module so the split preserves the worker's private state and call graph.

include!("context.rs");
include!("loop.rs");
include!("event_loop.rs");
include!("terminal_output.rs");
include!("tunnel_startup.rs");
include!("command_event.rs");
include!("metrics.rs");
include!("session_snapshot.rs");
include!("sftp_startup.rs");
include!("remote_exec.rs");
include!("without_sftp.rs");
include!("dispatch_terminal.rs");
include!("dispatch_transfer.rs");
include!("dispatch_files.rs");
include!("dispatch.rs");
