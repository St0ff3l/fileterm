//! SSH session implementation.
//!
//! Only the APIs consumed by other `sessions` and `commands` modules are
//! exported here. The protocol implementation stays private to this directory.

mod session;

pub(crate) use session::{
    effective_remote_file_type, is_sftp_path_not_found_message, shell_cwd_sftp_path_candidates,
};
pub use session::{format_unix_ts, list_dir, start_ssh_worker, test_connection};
