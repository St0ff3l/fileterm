// SSH worker implementation fragments.
//
// The fragments are included into the existing `sessions::ssh::session`
// module so the split preserves the worker's private state and call graph.

include!("loop.rs");
include!("remote_exec.rs");
include!("without_sftp.rs");
include!("dispatch.rs");
