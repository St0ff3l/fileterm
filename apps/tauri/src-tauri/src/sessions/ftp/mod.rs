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
include!("tests.rs");
