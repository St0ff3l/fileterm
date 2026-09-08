//! Durable transfer service facade.
//!
//! The implementation is kept in responsibility-oriented fragments while
//! remaining one private Rust module. This preserves the existing service
//! API and its shared workspace state semantics during the physical split.

include!("model.rs");
include!("runtime.rs");
include!("remote.rs");
include!("error_classify.rs");
include!("creation.rs");
include!("run_lifecycle.rs");
include!("directory_execution.rs");
include!("directory_entry.rs");
include!("cleanup.rs");
include!("tests.rs");
