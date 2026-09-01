// SSH shell state facade. The fragments retain the existing private session
// namespace while keeping CWD, root access, setup suppression, and encoding separate.

include!("shell/cwd.rs");
include!("shell/root_access.rs");
include!("shell/shell_setup.rs");
include!("shell/encoding.rs");
