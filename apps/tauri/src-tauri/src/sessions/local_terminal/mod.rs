use crate::{
    services::{workspace::LocalTerminalRuntimeGate, WorkspaceState, WorkspaceTabStatus},
    sessions::{
        terminal::{emit_local_terminal_data, set_terminal_state, update_local_terminal_cwd},
        WorkerCmd,
    },
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, mpsc as std_mpsc, Arc},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const LOCAL_OUTPUT_CHANNEL_CAPACITY: usize = 128;
const LOCAL_OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const LOCAL_OUTPUT_BATCH_MAX_BYTES: usize = 32 * 1024;
const LOCAL_OUTPUT_BATCH_WINDOW: Duration = Duration::from_millis(16);

include!("scanner.rs");
include!("shell.rs");
include!("terminal_output.rs");
include!("process.rs");
include!("worker.rs");
include!("tests.rs");
