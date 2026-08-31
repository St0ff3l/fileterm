// SSH worker based on russh (pure-Rust async SSH implementation).
//
// This module is intentionally assembled from focused source fragments. They
// remain in one Rust module so transport, shell, SFTP, and worker code can keep
// their narrow private APIs instead of making implementation details visible to
// sibling modules merely for file organization.

include!("shared.rs");
include!("device_mode.rs");
include!("runtime.rs");
include!("tunnels.rs");
include!("shell.rs");
include!("transport.rs");
include!("authentication.rs");
include!("sftp.rs");
include!("worker.rs");
include!("files.rs");
include!("tests.rs");
