//! FTP session implementation.
//!
//! FTP keeps its own protocol facade and responsibility-oriented fragments;
//! it must not share implementation state with the SSH/SFTP session.

include!("types.rs");
include!("worker.rs");
include!("transport.rs");
include!("capabilities.rs");
include!("listing.rs");
include!("file_operations.rs");
include!("tests/mod.rs");

#[cfg(test)]
#[path = "tests/commit.rs"]
mod commit_tests;

#[cfg(test)]
#[path = "tests/editor_save.rs"]
mod editor_save_tests;

#[cfg(test)]
#[path = "tests/xlight.rs"]
mod xlight_tests;
