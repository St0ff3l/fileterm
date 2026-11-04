// SSH file operations facade. Implementation stays in responsibility fragments
// so the session assembly can preserve its private API without widening visibility.

include!("sftp_files.rs");
include!("transfer_io.rs");
include!("root_transfer.rs");
include!("shell_exec.rs");
include!("files/root_listing.rs");

#[cfg(test)]
#[path = "tests/commit.rs"]
mod commit_tests;

#[cfg(test)]
#[path = "tests/transfer_io.rs"]
mod transfer_io_tests;
