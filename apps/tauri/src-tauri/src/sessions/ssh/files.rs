// SSH file operations facade. Implementation stays in responsibility fragments
// so the session assembly can preserve its private API without widening visibility.

include!("sftp_files.rs");
include!("transfer_io.rs");
include!("root_transfer.rs");
include!("shell_exec.rs");
