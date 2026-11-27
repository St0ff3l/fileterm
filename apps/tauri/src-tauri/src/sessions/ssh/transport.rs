// SSH transport facade. Jump, proxy, credential/host verification, and session
// opening remain private implementation fragments in this module directory.

include!("transport/host.rs");
include!("transport/route.rs");
include!("transport/jump.rs");
include!("transport/proxy.rs");
include!("transport/route_policy.rs");
include!("transport/route_health.rs");
include!("transport/route_management.rs");
include!("transport/credentials.rs");
include!("transport/ssh_config_route.rs");
include!("transport/algorithms.rs");
include!("transport/session.rs");
