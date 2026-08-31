use crate::services::connection_operations::ConnectionOperationRegistry;
use crate::services::transfers::TransferTask;
use crate::sessions::WorkerCmd;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tauri::ipc::Channel;
use tokio::sync::{oneshot, watch, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct TransferRunHandle {
    pub generation: u64,
    pub cancel: CancellationToken,
    pub settled: watch::Receiver<bool>,
}

/// Coordinates output from one local PTY runtime with shutdown/reconnect.
///
/// The reader runs on a native thread while reconnect and close are async
/// commands. Keeping an async lock around the final output publication lets a
/// shutdown wait briefly for an in-flight chunk, then deactivate the old
/// runtime before a replacement shell is installed for the same tab.
pub struct LocalTerminalRuntimeGate {
    pub(crate) active: AtomicBool,
    pub(crate) emit_lock: Mutex<()>,
}

include!("model.rs");
include!("profiles.rs");
include!("state.rs");
include!("tests.rs");
