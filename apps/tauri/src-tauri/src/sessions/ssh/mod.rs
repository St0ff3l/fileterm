//! SSH session implementation.
//!
//! Only the APIs consumed by other `sessions` and `commands` modules are
//! exported here. The protocol implementation stays private to this directory.

mod session;

pub(crate) const SFTP_PATH_NOT_FOUND: &str = "SFTP path not found (no such file)";

pub(crate) use session::{
    effective_remote_file_type, has_proxy_assignment_jump_conflict,
    is_sftp_path_not_found_message, persist_new_profile_with_ssh_route, probe_ssh_route_policy,
    save_ssh_route_policy, shell_cwd_sftp_path_candidates, ssh_route_policy_snapshot,
    synology_volume_prefix, validate_ssh_route_profile, SshRoutePolicy, SshRoutePolicySnapshot,
};
pub use session::{
    format_unix_ts, list_dir, start_sftp_worker, start_ssh_worker, test_connection,
    test_sftp_connection,
};
