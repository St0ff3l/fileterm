//! Durable transfer service facade.
//!
//! The implementation is kept in responsibility-oriented fragments while
//! remaining one private Rust module. This preserves the existing service
//! API and its shared workspace state semantics during the physical split.

include!("model.rs");
include!("runtime.rs");
include!("remote.rs");
include!("error_classify.rs");
include!("path_validation.rs");
include!("creation.rs");
include!("copy_creation.rs");
include!("copy_execution.rs");
include!("copy_queue.rs");
include!("copy_stream.rs");
include!("conflict_policy.rs");
include!("run_lifecycle.rs");
include!("directory_execution.rs");
include!("directory_entry.rs");
include!("cleanup.rs");
include!("tests/mod.rs");

mod power;

#[cfg(test)]
#[path = "tests/journal.rs"]
mod journal_tests;

#[cfg(test)]
#[path = "tests/cancellation.rs"]
mod cancellation_tests;

#[cfg(test)]
#[path = "tests/local_download.rs"]
mod local_download_tests;
