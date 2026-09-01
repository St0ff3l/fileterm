// SSH transport facade. Jump, proxy, credential/host verification, and session
// opening remain private implementation fragments in this module directory.

include!("transport/host.rs");
include!("transport/jump.rs");
include!("transport/proxy.rs");
include!("transport/credentials.rs");
include!("transport/session.rs");
