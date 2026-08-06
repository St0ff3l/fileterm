use crate::{
    services::{workspace::LocalTerminalRuntimeGate, WorkspaceState, WorkspaceTabStatus},
    sessions::{
        terminal::{emit_local_terminal_data, set_terminal_state, update_local_terminal_cwd},
        WorkerCmd,
    },
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    env,
    io::{Read, Write},
    path::PathBuf,
    sync::{atomic::Ordering, mpsc as std_mpsc, Arc},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);
const LOCAL_OUTPUT_CHANNEL_CAPACITY: usize = 128;
const LOCAL_OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const LOCAL_OUTPUT_BATCH_MAX_BYTES: usize = 32 * 1024;
const LOCAL_OUTPUT_BATCH_WINDOW: Duration = Duration::from_millis(16);

#[derive(Default)]
struct Utf8StreamDecoder {
    pending: Vec<u8>,
}

impl Utf8StreamDecoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        if bytes.is_empty() && self.pending.is_empty() {
            return String::new();
        }

        let mut combined = Vec::with_capacity(self.pending.len() + bytes.len());
        combined.extend_from_slice(&self.pending);
        combined.extend_from_slice(bytes);
        self.pending.clear();

        match std::str::from_utf8(&combined) {
            Ok(value) => value.to_string(),
            Err(error) if error.error_len().is_none() => {
                let valid_up_to = error.valid_up_to();
                self.pending.extend_from_slice(&combined[valid_up_to..]);
                String::from_utf8_lossy(&combined[..valid_up_to]).into_owned()
            }
            Err(_) => String::from_utf8_lossy(&combined).into_owned(),
        }
    }

    fn finish(&mut self) -> String {
        let pending = std::mem::take(&mut self.pending);
        String::from_utf8_lossy(&pending).into_owned()
    }
}

#[derive(Clone, Debug)]
pub struct LocalTerminalLaunch {
    pub shell: String,
    pub cwd: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalLaunchOptions {
    pub shell: Option<String>,
    pub cwd: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
}

enum LocalPtyCommand {
    Input(String),
    Resize {
        cols: u32,
        rows: u32,
        width: u32,
        height: u32,
    },
    Shutdown,
}

struct LocalOutputChunk {
    data: String,
    dropped_bytes_before: usize,
    /// 丢帧期间被丢的数据里是否可能包含 alternate screen 切换序列。
    /// renderer 可据此提示用户终端状态可能不一致（参考 Netcatty 的
    /// droppedOutputMayAffectTerminalState 语义）。
    dropped_alt_screen_change: bool,
}

#[derive(Default)]
struct LocalOutputDropState {
    bytes: usize,
    logged: bool,
    saw_alt_screen_change: bool,
    alt_screen_scanner: AltScreenTransitionScanner,
}

#[derive(Default)]
struct AltScreenTransitionScanner {
    state: AltScreenScanState,
}

#[derive(Default)]
enum AltScreenScanState {
    #[default]
    Ground,
    Escape,
    Csi {
        params: Vec<u8>,
        has_intermediate: bool,
        overflowed: bool,
    },
}

impl AltScreenTransitionScanner {
    /// Scan a PTY chunk while retaining an incomplete CSI sequence for the next chunk.
    fn observe(&mut self, data: &str) -> bool {
        let mut found_transition = false;
        for byte in data.bytes() {
            let (state, transition) = match std::mem::take(&mut self.state) {
                AltScreenScanState::Ground => {
                    if byte == 0x1b {
                        (AltScreenScanState::Escape, false)
                    } else {
                        (AltScreenScanState::Ground, false)
                    }
                }
                AltScreenScanState::Escape => match byte {
                    b'[' => (
                        AltScreenScanState::Csi {
                            params: Vec::new(),
                            has_intermediate: false,
                            overflowed: false,
                        },
                        false,
                    ),
                    0x1b => (AltScreenScanState::Escape, false),
                    _ => (AltScreenScanState::Ground, false),
                },
                AltScreenScanState::Csi {
                    mut params,
                    mut has_intermediate,
                    mut overflowed,
                } => {
                    if (0x30..=0x3f).contains(&byte) {
                        if params.len() < 128 {
                            params.push(byte);
                        } else {
                            overflowed = true;
                        }
                        (
                            AltScreenScanState::Csi {
                                params,
                                has_intermediate,
                                overflowed,
                            },
                            false,
                        )
                    } else if (0x20..=0x2f).contains(&byte) {
                        has_intermediate = true;
                        (
                            AltScreenScanState::Csi {
                                params,
                                has_intermediate,
                                overflowed,
                            },
                            false,
                        )
                    } else if (0x40..=0x7e).contains(&byte) {
                        let transition = !overflowed
                            && !has_intermediate
                            && (byte == b'h' || byte == b'l')
                            && alt_screen_params_match(&params);
                        (AltScreenScanState::Ground, transition)
                    } else if byte == 0x1b {
                        (AltScreenScanState::Escape, false)
                    } else {
                        (AltScreenScanState::Ground, false)
                    }
                }
            };
            self.state = state;
            found_transition |= transition;
        }
        found_transition
    }

    fn has_pending_sequence(&self) -> bool {
        !matches!(&self.state, AltScreenScanState::Ground)
    }
}

fn alt_screen_params_match(params: &[u8]) -> bool {
    let params = if params.first() == Some(&b'?') {
        &params[1..]
    } else {
        params
    };
    params.split(|byte| *byte == b';').any(|token| {
        let token = std::str::from_utf8(token).unwrap_or("");
        matches!(token, "47" | "1047" | "1049")
    })
}

/// 检测数据里是否包含 DECSET/DECRST 风格的 alternate screen 切换序列。
/// 这个无状态入口保留给单 chunk 场景和单元测试；真实 PTY reader 使用
/// `AltScreenTransitionScanner`，以便处理 ANSI 序列跨 read 边界的情况。
#[cfg(test)]
fn scan_alt_screen_transition(data: &str) -> bool {
    let mut scanner = AltScreenTransitionScanner::default();
    scanner.observe(data)
}

const LOCAL_OSC7_BUFFER_LIMIT: usize = 16 * 1024;
const LOCAL_OSC7_MARKER: &str = "\x1b]7;";

#[derive(Default)]
struct LocalOsc7CwdTracker {
    buffer: String,
}

impl LocalOsc7CwdTracker {
    fn observe(&mut self, chunk: &str) -> Option<String> {
        self.buffer.push_str(chunk);
        let mut latest_cwd = None;

        loop {
            let Some(start) = self.buffer.find(LOCAL_OSC7_MARKER) else {
                retain_osc7_prefix(&mut self.buffer);
                break;
            };
            if start > 0 {
                self.buffer.drain(..start);
            }

            let payload = &self.buffer[LOCAL_OSC7_MARKER.len()..];
            let bel_end = payload.find('\u{7}').map(|index| (index + 1, 1));
            let st_end = payload.find("\x1b\\").map(|index| (index + 2, 2));
            let Some((end, terminator_len)) = [bel_end, st_end]
                .into_iter()
                .flatten()
                .min_by_key(|(end, _)| *end)
            else {
                break;
            };

            if let Some(cwd) = decode_osc7_cwd(&payload[..end - terminator_len]) {
                latest_cwd = Some(cwd);
            }
            self.buffer.drain(..LOCAL_OSC7_MARKER.len() + end);
        }

        if self.buffer.len() > LOCAL_OSC7_BUFFER_LIMIT {
            if let Some(start) = self.buffer.rfind(LOCAL_OSC7_MARKER) {
                self.buffer.drain(..start);
            } else {
                self.buffer.clear();
            }
        }
        latest_cwd
    }
}

fn retain_osc7_prefix(buffer: &mut String) {
    let keep = ["\x1b", "\x1b]", "\x1b]7"]
        .into_iter()
        .filter(|prefix| buffer.ends_with(prefix))
        .map(str::len)
        .max()
        .unwrap_or(0);
    if keep == 0 {
        buffer.clear();
    } else {
        let start = buffer.len() - keep;
        buffer.drain(..start);
    }
}

fn decode_osc7_cwd(payload: &str) -> Option<String> {
    let uri = payload.strip_prefix("file://")?;
    let path_start = uri.find('/')?;
    let path = percent_decode(&uri[path_start..]);
    if path.is_empty() || path.contains('\0') {
        return None;
    }

    #[cfg(target_os = "windows")]
    {
        let mut path = path;
        if path.starts_with('/') && path.as_bytes().get(2) == Some(&b':') {
            path.remove(0);
        }
        return Some(path);
    }

    #[cfg(not(target_os = "windows"))]
    Some(path)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub fn default_launch() -> LocalTerminalLaunch {
    LocalTerminalLaunch {
        shell: default_shell(),
        cwd: default_working_directory(),
        args: Vec::new(),
        env: BTreeMap::new(),
    }
}

pub fn resolve_launch(
    options: Option<LocalTerminalLaunchOptions>,
) -> Result<LocalTerminalLaunch, String> {
    let defaults = default_launch();
    let options = options.unwrap_or_default();
    let launch = LocalTerminalLaunch {
        shell: options.shell.unwrap_or(defaults.shell),
        cwd: options.cwd.unwrap_or(defaults.cwd),
        args: options.args.unwrap_or_default(),
        env: options.env.unwrap_or_default(),
    };
    validate_launch(&launch)?;
    Ok(launch)
}

#[allow(clippy::too_many_arguments)]
pub fn start_local_terminal_worker(
    tab_id: String,
    runtime_id: String,
    worker_rx: mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: mpsc::UnboundedReceiver<String>,
    app: AppHandle,
    cancellation: CancellationToken,
    launch: LocalTerminalLaunch,
    runtime_gate: Arc<LocalTerminalRuntimeGate>,
) -> Result<(), String> {
    validate_launch(&launch)?;
    let cwd = PathBuf::from(&launch.cwd);
    if !cwd.is_dir() {
        return Err(format!(
            "Local terminal working directory does not exist: {}",
            launch.cwd
        ));
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Unable to allocate local PTY: {error}"))?;
    let portable_pty::PtyPair { master, slave } = pair;

    let mut command = CommandBuilder::new(&launch.shell);
    command.cwd(cwd);
    for (name, value) in &launch.env {
        command.env(name, value);
    }
    configure_shell_command(&mut command, &launch.shell, &launch.args, &launch.env);

    let mut child = slave
        .spawn_command(command)
        .map_err(|error| format!("Unable to start local shell {}: {error}", launch.shell))?;
    let process_tree = LocalProcessTree::attach(child.as_ref());
    let reader = master
        .try_clone_reader()
        .map_err(|error| format!("Unable to read local PTY output: {error}"))?;
    let writer = match master.take_writer() {
        Ok(writer) => writer,
        Err(error) => {
            let _ = child.kill();
            return Err(format!("Unable to write to local PTY: {error}"));
        }
    };

    // Keep PTY reads independent from renderer IPC and transcript locking.
    // The bounded queue prevents a command such as `yes` or a verbose CLI
    // from growing an unbounded native-thread backlog, while the control
    // channel remains available for Ctrl+C and resize commands.
    let (output_tx, mut output_rx) =
        mpsc::channel::<LocalOutputChunk>(LOCAL_OUTPUT_CHANNEL_CAPACITY);
    let (output_done_tx, output_done_rx) = tokio::sync::oneshot::channel();
    let pump_app = app.clone();
    let pump_tab_id = tab_id.clone();
    let pump_runtime_id = runtime_id.clone();
    let pump_gate = runtime_gate.clone();
    tauri::async_runtime::spawn(async move {
        let mut cwd_tracker = LocalOsc7CwdTracker::default();
        let mut pending_chunk = None;
        while let Some(first_chunk) = match pending_chunk.take() {
            Some(chunk) => Some(chunk),
            None => output_rx.recv().await,
        } {
            let mut batch = String::new();
            append_local_output_chunk(&mut batch, &first_chunk);
            let deadline = tokio::time::sleep(LOCAL_OUTPUT_BATCH_WINDOW);
            tokio::pin!(deadline);

            while batch.len() < LOCAL_OUTPUT_BATCH_MAX_BYTES {
                tokio::select! {
                    _ = &mut deadline => break,
                    next_chunk = output_rx.recv() => match next_chunk {
                        Some(chunk) => {
                            let previous_len = batch.len();
                            append_local_output_chunk(&mut batch, &chunk);
                            if batch.len() > LOCAL_OUTPUT_BATCH_MAX_BYTES {
                                batch.truncate(previous_len);
                                pending_chunk = Some(chunk);
                                break;
                            }
                        }
                        None => break,
                    },
                }
            }

            if !emit_local_terminal_data(
                &pump_app,
                &pump_tab_id,
                &pump_runtime_id,
                &pump_gate,
                &batch,
            )
            .await
            {
                break;
            }
            if let Some(cwd) = cwd_tracker.observe(&batch) {
                let _ = update_local_terminal_cwd(
                    &pump_app,
                    &pump_tab_id,
                    &pump_runtime_id,
                    &pump_gate,
                    cwd,
                )
                .await;
            }
        }
        let _ = output_done_tx.send(());
    });

    let (control_tx, control_rx) = std_mpsc::channel::<LocalPtyCommand>();
    let relay_tx = control_tx.clone();
    tauri::async_runtime::spawn(async move {
        forward_terminal_commands(worker_rx, terminal_input_rx, cancellation, relay_tx).await;
    });

    let reader_app = app.clone();
    let reader_tab_id = tab_id.clone();
    let reader_gate = runtime_gate.clone();
    let reader_output_tx = output_tx.clone();
    thread::Builder::new()
        .name("fileterm-local-pty-reader".to_string())
        .spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 8 * 1024];
            let mut decoder = Utf8StreamDecoder::default();
            let mut output_drop_state = LocalOutputDropState::default();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        let tail = decoder.finish();
                        if !tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                tail,
                                &mut output_drop_state,
                            );
                        }
                        flush_local_output_drop_notice(&reader_output_tx, &mut output_drop_state);
                        break;
                    }
                    Ok(size) => {
                        let chunk = decoder.decode(&buffer[..size]);
                        if !chunk.is_empty()
                            && !queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                chunk,
                                &mut output_drop_state,
                            )
                        {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        let tail = decoder.finish();
                        if !tail.is_empty() {
                            let _ = queue_local_terminal_output(
                                &reader_app,
                                &reader_tab_id,
                                &reader_gate,
                                &reader_output_tx,
                                tail,
                                &mut output_drop_state,
                            );
                        }
                        flush_local_output_drop_notice(&reader_output_tx, &mut output_drop_state);
                        break;
                    }
                }
            }
        })
        .map_err(|error| {
            process_tree.terminate(child.as_mut());
            format!("Unable to start local PTY reader: {error}")
        })?;

    thread::Builder::new()
        .name("fileterm-local-pty".to_string())
        .spawn(move || {
            let (summary, status) =
                run_pty_loop(control_rx, &mut child, master, writer, &process_tree);
            tauri::async_runtime::block_on(async move {
                let _ = tokio::time::timeout(LOCAL_OUTPUT_DRAIN_TIMEOUT, output_done_rx).await;
                if cleanup_local_terminal_runtime(&app, &tab_id, &runtime_id).await {
                    set_terminal_state(&app, &tab_id, summary, status).await;
                }
            });
        })
        .map_err(|error| format!("Unable to start local PTY worker: {error}"))?;

    Ok(())
}

fn queue_local_terminal_output(
    app: &AppHandle,
    tab_id: &str,
    gate: &Arc<LocalTerminalRuntimeGate>,
    output_tx: &mpsc::Sender<LocalOutputChunk>,
    chunk: String,
    output_drop_state: &mut LocalOutputDropState,
) -> bool {
    if !gate.active.load(Ordering::Acquire) {
        return false;
    }

    // Feed every reader chunk into the scanner, not only chunks that are already
    // being dropped. A CSI sequence can start in a successfully delivered chunk
    // and finish after the output queue becomes full.
    let alt_screen_transition = output_drop_state.alt_screen_scanner.observe(&chunk);
    let dropped_bytes_before = output_drop_state.bytes;
    let dropped_alt_screen_change = output_drop_state.saw_alt_screen_change
        || (dropped_bytes_before > 0 && alt_screen_transition);
    match output_tx.try_send(LocalOutputChunk {
        data: chunk.clone(),
        dropped_bytes_before,
        dropped_alt_screen_change,
    }) {
        Ok(()) => {
            output_drop_state.bytes = 0;
            output_drop_state.logged = false;
            output_drop_state.saw_alt_screen_change = false;
            true
        }
        Err(mpsc::error::TrySendError::Full(_)) => {
            output_drop_state.bytes = output_drop_state.bytes.saturating_add(chunk.len());
            if alt_screen_transition || output_drop_state.alt_screen_scanner.has_pending_sequence()
            {
                output_drop_state.saw_alt_screen_change = true;
            }
            if !output_drop_state.logged {
                output_drop_state.logged = true;
                crate::services::logging::session(
                    app,
                    "WARN",
                    "local-terminal",
                    tab_id,
                    "terminal output pump saturated; dropping local PTY output",
                );
            }
            true
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

fn flush_local_output_drop_notice(
    output_tx: &mpsc::Sender<LocalOutputChunk>,
    output_drop_state: &mut LocalOutputDropState,
) {
    if output_drop_state.bytes == 0 {
        return;
    }

    let dropped_bytes_before = output_drop_state.bytes;
    let dropped_alt_screen_change = output_drop_state.saw_alt_screen_change;
    if output_tx
        .try_send(LocalOutputChunk {
            data: String::new(),
            dropped_bytes_before,
            dropped_alt_screen_change,
        })
        .is_ok()
    {
        output_drop_state.bytes = 0;
        output_drop_state.logged = false;
        output_drop_state.saw_alt_screen_change = false;
    }
}

fn append_local_output_chunk(batch: &mut String, chunk: &LocalOutputChunk) {
    if chunk.dropped_bytes_before > 0 {
        batch.push_str(&format!(
            "\r\n[FileTerm: local terminal output dropped {} bytes while the renderer was busy]\r\n",
            chunk.dropped_bytes_before
        ));
        if chunk.dropped_alt_screen_change {
            batch.push_str("\r\n[FileTerm: dropped output may include alternate screen transitions; terminal state may be inconsistent — run `reset` or Ctrl+L to resync]\r\n");
        }
    }
    batch.push_str(&chunk.data);
}

#[derive(Default)]
struct LocalProcessTree {
    #[cfg(target_os = "windows")]
    job_handle: Option<usize>,
}

impl LocalProcessTree {
    fn attach(child: &dyn portable_pty::Child) -> Self {
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::{
                Foundation::{CloseHandle, HANDLE},
                System::JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                },
            };

            let Some(raw_process) = child.as_raw_handle() else {
                return Self::default();
            };
            let job_handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job_handle.is_null() {
                return Self::default();
            }

            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    job_handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION)
                        .cast::<std::ffi::c_void>(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) != 0
            };
            let assigned =
                unsafe { AssignProcessToJobObject(job_handle, raw_process as HANDLE) != 0 };
            if !configured || !assigned {
                unsafe {
                    CloseHandle(job_handle);
                }
                return Self::default();
            }

            return Self {
                job_handle: Some(job_handle as usize),
            };
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = child;
            Self::default()
        }
    }

    fn terminate(&self, child: &mut dyn portable_pty::Child) {
        #[cfg(target_os = "windows")]
        if let Some(job_handle) = self.job_handle {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;

            let terminated = unsafe { TerminateJobObject(job_handle as _, 1) != 0 };
            if terminated {
                return;
            }
        }

        #[cfg(unix)]
        if let Some(pid) = child.process_id().filter(|pid| *pid > 0) {
            if pid <= i32::MAX as u32 {
                let process_group = -(pid as libc::pid_t);
                let child_alive = child.try_wait().ok().flatten().is_none();
                if child_alive {
                    unsafe {
                        libc::kill(process_group, libc::SIGHUP);
                    }
                    for _ in 0..5 {
                        if child.try_wait().ok().flatten().is_some() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(25));
                    }
                }
                unsafe {
                    libc::kill(process_group, libc::SIGKILL);
                }
                return;
            }
        }

        let _ = child.kill();
    }
}

impl Drop for LocalProcessTree {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        if let Some(job_handle) = self.job_handle.take() {
            use windows_sys::Win32::Foundation::CloseHandle;

            unsafe {
                CloseHandle(job_handle as _);
            }
        }
    }
}

async fn forward_terminal_commands(
    mut worker_rx: mpsc::Receiver<WorkerCmd>,
    mut terminal_input_rx: mpsc::UnboundedReceiver<String>,
    cancellation: CancellationToken,
    control_tx: std_mpsc::Sender<LocalPtyCommand>,
) {
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                let _ = control_tx.send(LocalPtyCommand::Shutdown);
                break;
            }
            input = terminal_input_rx.recv() => match input {
                Some(data) => {
                    if control_tx.send(LocalPtyCommand::Input(data)).is_err() {
                        break;
                    }
                }
                None => {
                    let _ = control_tx.send(LocalPtyCommand::Shutdown);
                    break;
                }
            },
            command = worker_rx.recv() => match command {
                Some(WorkerCmd::ResizeTerminal { cols, rows, width, height }) => {
                    if control_tx.send(LocalPtyCommand::Resize { cols, rows, width, height }).is_err() {
                        break;
                    }
                }
                Some(WorkerCmd::Disconnect) | None => {
                    let _ = control_tx.send(LocalPtyCommand::Shutdown);
                    break;
                }
                Some(_) => {
                    // The local terminal has no remote filesystem, transfer, or tunnel surface.
                }
            }
        }
    }
}

fn run_pty_loop(
    control_rx: std_mpsc::Receiver<LocalPtyCommand>,
    child: &mut Box<dyn portable_pty::Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    mut writer: Box<dyn Write + Send>,
    process_tree: &LocalProcessTree,
) -> (String, WorkspaceTabStatus) {
    let mut last_size: Option<PtySize> = None;
    loop {
        match control_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
            Ok(LocalPtyCommand::Input(data)) => {
                if let Err(error) = writer
                    .write_all(data.as_bytes())
                    .and_then(|()| writer.flush())
                {
                    process_tree.terminate(child.as_mut());
                    return (
                        format!("Local shell input failed: {error}"),
                        WorkspaceTabStatus::Error,
                    );
                }
            }
            Ok(LocalPtyCommand::Resize {
                cols,
                rows,
                width,
                height,
            }) => {
                let size = PtySize {
                    cols: clamp_u16(cols, DEFAULT_COLS),
                    rows: clamp_u16(rows, DEFAULT_ROWS),
                    pixel_width: clamp_u16(width, 0),
                    pixel_height: clamp_u16(height, 0),
                };
                if last_size != Some(size) {
                    let _ = master.resize(size);
                    last_size = Some(size);
                }
            }
            Ok(LocalPtyCommand::Shutdown) | Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                process_tree.terminate(child.as_mut());
                return (
                    "Local shell stopped".to_string(),
                    WorkspaceTabStatus::Closed,
                );
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                process_tree.terminate(child.as_mut());
                return (
                    local_shell_exit_summary(&status),
                    WorkspaceTabStatus::Closed,
                );
            }
            Ok(None) => {}
            Err(error) => {
                process_tree.terminate(child.as_mut());
                return (
                    format!("Unable to observe local shell: {error}"),
                    WorkspaceTabStatus::Error,
                );
            }
        }
    }
}

fn local_shell_exit_summary(status: &portable_pty::ExitStatus) -> String {
    if status.success() {
        format!("Local shell exited with code {}", status.exit_code())
    } else {
        format!("Local shell exited: {status}")
    }
}

pub async fn deactivate_local_terminal_runtime(state: &WorkspaceState, tab_id: &str) {
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .remove(tab_id);
    let gate = state
        .local_terminal_runtime_gates
        .write()
        .await
        .remove(tab_id);
    if let Some(gate) = gate {
        gate.deactivate().await;
    }
}

async fn cleanup_local_terminal_runtime(app: &AppHandle, tab_id: &str, runtime_id: &str) -> bool {
    let state = app.state::<WorkspaceState>();
    let gate = {
        let mut runtime_ids = state.local_terminal_runtime_ids.write().await;
        if runtime_ids
            .get(tab_id)
            .is_none_or(|current_id| current_id != runtime_id)
        {
            return false;
        }
        runtime_ids.remove(tab_id);
        state
            .local_terminal_runtime_gates
            .write()
            .await
            .remove(tab_id)
    };
    if let Some(gate) = gate {
        gate.deactivate().await;
    }
    state.terminal_inputs.write().await.remove(tab_id);
    state.workers.write().await.remove(tab_id);
    state.worker_controls.write().await.remove(tab_id);
    true
}

fn clamp_u16(value: u32, fallback: u16) -> u16 {
    if value == 0 {
        return fallback;
    }
    value.min(u16::MAX as u32) as u16
}

#[cfg(target_os = "windows")]
fn default_shell() -> String {
    // 优先 PowerShell（Windows 默认），缺失时回退 cmd.exe。
    // Server Core / 精简镜像可能没有 powershell.exe。
    if shell_available_in_path("powershell.exe") {
        "powershell.exe".to_string()
    } else if shell_available_in_path("pwsh.exe") {
        "pwsh.exe".to_string()
    } else {
        "cmd.exe".to_string()
    }
}

#[cfg(target_os = "windows")]
fn shell_available_in_path(name: &str) -> bool {
    use std::path::{Path, PathBuf};

    // 正常情况：PATH 里能找到。
    if let Some(path_var) = env::var_os("PATH") {
        if env::split_paths(&path_var).any(|dir| dir.join(name).is_file()) {
            return true;
        }
    }

    // Fallback：PATH 异常（如被清理过的服务进程）时，直接查 System32。
    // Windows PowerShell 和 cmd.exe 在 System32；PowerShell 7 另查其标准安装目录。
    let system32 = env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32");
    if system32.join(name).is_file() {
        return true;
    }

    if name.eq_ignore_ascii_case("pwsh.exe") {
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            let Some(root) = env::var_os(variable) else {
                continue;
            };
            let base = Path::new(&root).join("PowerShell");
            if ["7", "7-preview"]
                .into_iter()
                .any(|version| base.join(version).join(name).is_file())
            {
                return true;
            }
        }
    }

    false
}

#[cfg(not(target_os = "windows"))]
fn default_shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty() && !shell_path_is_unavailable(shell))
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        })
}

fn default_working_directory() -> String {
    let home = if cfg!(target_os = "windows") {
        env::var_os("USERPROFILE")
    } else {
        env::var_os("HOME")
    };
    home.map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned()
}

fn shell_path_is_unavailable(shell: &str) -> bool {
    let has_path_separator = shell.contains('/') || shell.contains('\\');
    has_path_separator && !PathBuf::from(shell).is_file()
}

fn validate_launch(launch: &LocalTerminalLaunch) -> Result<(), String> {
    if launch.shell.trim().is_empty() {
        return Err("Local terminal shell is empty".to_string());
    }
    if launch.shell.contains('\0') {
        return Err("Local terminal shell contains a NUL byte".to_string());
    }
    if shell_path_is_unavailable(&launch.shell) {
        return Err(format!(
            "Local terminal shell does not exist: {}",
            launch.shell
        ));
    }
    if launch.cwd.trim().is_empty() {
        return Err("Local terminal working directory is empty".to_string());
    }
    if launch.cwd.contains('\0') {
        return Err("Local terminal working directory contains a NUL byte".to_string());
    }
    const MAX_LOCAL_SHELL_ARGS: usize = 128;
    const MAX_LOCAL_SHELL_ARG_BYTES: usize = 32 * 1024;
    if launch.args.len() > MAX_LOCAL_SHELL_ARGS {
        return Err(format!(
            "Local terminal accepts at most {MAX_LOCAL_SHELL_ARGS} shell arguments"
        ));
    }
    if let Some((index, _)) = launch
        .args
        .iter()
        .enumerate()
        .find(|(_, arg)| arg.contains('\0') || arg.len() > MAX_LOCAL_SHELL_ARG_BYTES)
    {
        return Err(format!(
            "Local terminal shell argument {index} is invalid or too large"
        ));
    }
    const MAX_LOCAL_ENV_ENTRIES: usize = 128;
    const MAX_LOCAL_ENV_VALUE_BYTES: usize = 64 * 1024;
    if launch.env.len() > MAX_LOCAL_ENV_ENTRIES {
        return Err(format!(
            "Local terminal accepts at most {MAX_LOCAL_ENV_ENTRIES} environment overrides"
        ));
    }
    if let Some((name, _)) = launch.env.iter().find(|(name, value)| {
        name.is_empty()
            || name.contains('=')
            || name.contains('\0')
            || value.contains('\0')
            || value.len() > MAX_LOCAL_ENV_VALUE_BYTES
    }) {
        return Err(format!(
            "Local terminal environment override {name:?} is invalid or too large"
        ));
    }
    Ok(())
}

fn shell_name(shell: &str) -> String {
    shell
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(shell)
        .trim_start_matches('-')
        .to_ascii_lowercase()
}

fn has_non_empty_env(name: &str) -> bool {
    env::var_os(name).is_some_and(|value| !value.is_empty())
}

fn set_default_env(
    command: &mut CommandBuilder,
    name: &str,
    value: &str,
    custom_env: &BTreeMap<String, String>,
) {
    if !custom_env.contains_key(name) {
        command.env(name, value);
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_shell_command(
    command: &mut CommandBuilder,
    shell: &str,
    extra_args: &[String],
    custom_env: &BTreeMap<String, String>,
) {
    let name = shell_name(shell);
    let inject_default_zsh_prompt = name == "zsh"
        && !custom_env.contains_key("PROMPT")
        && !custom_env.contains_key("PS1")
        && !has_non_empty_env("PROMPT")
        && !has_non_empty_env("PS1");
    if inject_default_zsh_prompt {
        // The prompt uses command substitution to percent-encode the current
        // directory for OSC 7. Enable it before user args so `-c`/`--` keep
        // their normal meaning.
        command.args(["-o", "promptsubst"]);
    }
    if matches!(
        name.as_str(),
        "bash" | "dash" | "fish" | "ksh" | "mksh" | "sh" | "zsh"
    ) {
        command.arg("-l");
    }
    command.args(extra_args);

    set_default_env(command, "TERM", "xterm-256color", custom_env);
    set_default_env(command, "COLORTERM", "truecolor", custom_env);
    set_default_env(command, "TERM_PROGRAM", "FileTerm", custom_env);

    // bash 原生读取 PROMPT_COMMAND，每次显示 prompt 前执行。zsh 默认使用
    // PROMPT；给它注入一个保持默认视觉样式的不可见 OSC 7 前缀。fish/sh 等
    // 没有安全的环境变量 prompt hook，用户需要在 rc 文件里手动加 hook。
    if name == "bash" {
        inject_bash_osc7_prompt_command(command, custom_env);
    } else if inject_default_zsh_prompt {
        inject_zsh_osc7_prompt(command, custom_env);
    }

    if !custom_env.contains_key("LANG")
        && !custom_env.contains_key("LC_ALL")
        && !has_non_empty_env("LANG")
        && !has_non_empty_env("LC_ALL")
    {
        command.env("LANG", default_utf8_locale());
    }
    if !custom_env.contains_key("LC_ALL")
        && !custom_env.contains_key("LC_CTYPE")
        && !has_non_empty_env("LC_ALL")
        && !has_non_empty_env("LC_CTYPE")
        && !custom_env
            .get("LANG")
            .cloned()
            .or_else(|| env::var("LANG").ok())
            .map(|value| value.to_ascii_lowercase().contains("utf-8"))
            .unwrap_or(false)
    {
        command.env("LC_CTYPE", default_utf8_locale());
    }
}

#[cfg(not(target_os = "windows"))]
fn inject_bash_osc7_prompt_command(
    command: &mut CommandBuilder,
    custom_env: &BTreeMap<String, String>,
) {
    // 用户显式传入 PROMPT_COMMAND 时不覆盖。
    if custom_env.contains_key("PROMPT_COMMAND") {
        return;
    }

    // PROMPT_COMMAND 在每次显示 prompt 前 emit 一次 OSC 7。将 `%` 先编码，
    // 避免目录名中的字面量 `%20` 被后端误解为一个空格。
    // 不使用 DEBUG trap：它会在每条简单命令（包括循环和子 shell）前产生
    // 额外 PTY 输出，反而会放大高输出场景的丢帧压力。
    const OSC7_HOOK: &str = "printf '\\033]7;file://%s\\007' \"${PWD//%/%25}\"";

    let combined = match env::var("PROMPT_COMMAND")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(existing) => format!("{OSC7_HOOK}; {existing}"),
        None => OSC7_HOOK.to_string(),
    };
    command.env("PROMPT_COMMAND", combined);
}

#[cfg(not(target_os = "windows"))]
fn inject_zsh_osc7_prompt(command: &mut CommandBuilder, custom_env: &BTreeMap<String, String>) {
    // 尊重用户显式提供的 prompt。没有自定义 prompt 时，使用 zsh 默认的
    // `%m%# ` 样式，仅在其前面加入不占列宽的 OSC 7 CWD 标记。
    if custom_env.contains_key("PROMPT")
        || custom_env.contains_key("PS1")
        || has_non_empty_env("PROMPT")
        || has_non_empty_env("PS1")
    {
        return;
    }

    const ZSH_PROMPT: &str = "%{$(printf '\\033]7;file://%s\\007' \"${PWD//%/%25}\")%}%m%# ";
    command.env("PROMPT", ZSH_PROMPT);
}

#[cfg(target_os = "windows")]
fn configure_shell_command(
    command: &mut CommandBuilder,
    shell: &str,
    extra_args: &[String],
    custom_env: &BTreeMap<String, String>,
) {
    let name = shell_name(shell);
    set_default_env(command, "TERM", "xterm-256color", custom_env);
    set_default_env(command, "COLORTERM", "truecolor", custom_env);
    set_default_env(command, "TERM_PROGRAM", "FileTerm", custom_env);

    match name.as_str() {
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => {
            command.args(["-NoLogo", "-NoExit"]);
            command.args(extra_args);
            // -Command / -CommandWithArgs / -File / -EncodedCommand 互斥：用户传了这些参数时
            // 不再自动追加 UTF-8 setup，避免 PowerShell 因参数冲突直接报错。
            // 用户需在自己的脚本/命令里设置 UTF-8 编码。
            if !powershell_args_have_explicit_command(extra_args) {
                command.args([
                    "-Command",
                    "$utf8 = [System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding = $utf8; [Console]::OutputEncoding = $utf8; $OutputEncoding = $utf8",
                ]);
            }
        }
        "cmd" | "cmd.exe" => {
            command.args(extra_args);
            // `/C` and `/K` consume the remaining command line. Appending our
            // own `/K` after an explicit command changes the user's command.
            if !cmd_args_have_explicit_command(extra_args) {
                command.args(["/K", "chcp 65001>nul"]);
            }
        }
        "bash" | "bash.exe" | "fish" | "fish.exe" | "zsh" | "zsh.exe" => {
            command.arg("--login");
            command.args(extra_args);
        }
        _ => {
            command.args(extra_args);
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn powershell_args_have_explicit_command(extra_args: &[String]) -> bool {
    // PowerShell 允许参数唯一前缀缩写，并把 `-c`、`-cwa`、`-f`、`-e`、`-ec`
    // 作为 Command/CommandWithArgs/File/EncodedCommand 的短写。命中任何显式命令模式后，
    // configure_shell_command 不再追加自己的 `-Command`。
    // ConfigurationFile/ConfigurationName 只是会话配置参数，仍可与
    // -Command 组合，不能把它们误判为命令模式。
    const EXPLICIT_FLAGS: &[&str] = &["command", "commandwithargs", "file", "encodedcommand"];
    const EXPLICIT_ALIASES: &[&str] = &["c", "cwa", "f", "e", "ec"];
    extra_args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        let Some(flag) = lower.strip_prefix('-') else {
            return false;
        };
        !flag.is_empty()
            && (EXPLICIT_ALIASES.contains(&flag)
                || EXPLICIT_FLAGS.iter().any(|known| known.starts_with(flag)))
    })
}

#[cfg(any(target_os = "windows", test))]
fn cmd_args_have_explicit_command(extra_args: &[String]) -> bool {
    extra_args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        matches!(lower.as_str(), "/c" | "/k")
    })
}

#[cfg(target_os = "macos")]
fn default_utf8_locale() -> &'static str {
    "en_US.UTF-8"
}

#[cfg(all(unix, not(target_os = "macos")))]
fn default_utf8_locale() -> &'static str {
    "C.UTF-8"
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    #[cfg(unix)]
    use std::io::Read;
    #[cfg(unix)]
    use std::sync::mpsc as std_mpsc;
    #[cfg(unix)]
    use std::time::Duration;

    #[cfg(unix)]
    use crate::services::workspace::WorkspaceTabStatus;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    use super::{
        append_local_output_chunk, clamp_u16, cmd_args_have_explicit_command,
        configure_shell_command, default_launch, local_shell_exit_summary,
        powershell_args_have_explicit_command, resolve_launch, run_pty_loop,
        scan_alt_screen_transition, shell_name, validate_launch, AltScreenTransitionScanner,
        LocalOsc7CwdTracker, LocalOutputChunk, LocalProcessTree, LocalPtyCommand,
        LocalTerminalLaunch, LocalTerminalLaunchOptions, Utf8StreamDecoder,
    };

    #[cfg(unix)]
    type TestPtyMaster = Box<dyn portable_pty::MasterPty + Send>;
    #[cfg(unix)]
    type TestPtyChild = Box<dyn portable_pty::Child + Send + Sync>;

    #[cfg(unix)]
    fn spawn_posix_test_pty(
        script: &str,
    ) -> (
        TestPtyMaster,
        TestPtyChild,
        std::sync::mpsc::Receiver<Vec<u8>>,
    ) {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", script]);
        let child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        drop(slave);

        let mut reader = master
            .try_clone_reader()
            .expect("local PTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => output.extend_from_slice(&buffer[..size]),
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });

        (master, child, output_rx)
    }

    #[test]
    fn pty_size_clamps_to_platform_u16_values() {
        assert_eq!(clamp_u16(0, 80), 80);
        assert_eq!(clamp_u16(120, 80), 120);
        assert_eq!(clamp_u16(u32::MAX, 80), u16::MAX);
    }

    #[test]
    fn utf8_stream_decoder_preserves_code_points_split_across_reads() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.decode("中".as_bytes().split_at(1).0), "");
        assert_eq!(decoder.decode(&"中".as_bytes()[1..]), "中");
        assert_eq!(decoder.decode(" + ".as_bytes()), " + ");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    fn utf8_stream_decoder_flushes_an_incomplete_tail_at_eof() {
        let mut decoder = Utf8StreamDecoder::default();
        assert_eq!(decoder.decode(&[0xf0, 0x9f]), "");
        assert_eq!(decoder.finish(), "�");
    }

    #[test]
    fn shell_name_handles_paths_and_login_shell_markers() {
        assert_eq!(shell_name("/bin/zsh"), "zsh");
        assert_eq!(shell_name("-bash"), "bash");
        assert_eq!(shell_name("C:\\Windows\\System32\\cmd.exe"), "cmd.exe");
    }

    #[test]
    fn launch_validation_rejects_empty_or_missing_explicit_shell_paths() {
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "  ".to_string(),
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_err());
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "/definitely/missing/fileterm-shell".to_string(),
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_err());
        assert!(validate_launch(&LocalTerminalLaunch {
            shell: "zsh".to_string(),
            cwd: "/tmp".to_string(),
            args: Vec::new(),
            env: BTreeMap::new(),
        })
        .is_ok());
    }

    #[test]
    fn launch_options_merge_platform_defaults_with_one_shot_overrides() {
        let mut environment = BTreeMap::new();
        environment.insert("FILETERM_TEST".to_string(), "present".to_string());
        let launch = resolve_launch(Some(LocalTerminalLaunchOptions {
            shell: Some("/bin/sh".to_string()),
            cwd: Some("/tmp".to_string()),
            args: Some(vec!["-i".to_string()]),
            env: Some(environment.clone()),
        }))
        .expect("valid local launch options should resolve");

        assert_eq!(launch.shell, "/bin/sh");
        assert_eq!(launch.cwd, "/tmp");
        assert_eq!(launch.args, vec!["-i"]);
        assert_eq!(launch.env, environment);
    }

    #[test]
    fn launch_validation_rejects_nul_and_oversized_overrides() {
        let mut launch = default_launch();
        launch.args = vec!["bad\0arg".to_string()];
        assert!(validate_launch(&launch).is_err());

        launch.args = vec!["x".repeat(32 * 1024 + 1)];
        assert!(validate_launch(&launch).is_err());
    }

    #[test]
    fn local_shell_exit_summary_keeps_exit_code_or_signal() {
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_exit_code(0)),
            "Local shell exited with code 0"
        );
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_exit_code(127)),
            "Local shell exited: Exited with code 127"
        );
        assert_eq!(
            local_shell_exit_summary(&portable_pty::ExitStatus::with_signal("SIGHUP")),
            "Local shell exited: Terminated by SIGHUP"
        );
    }

    #[test]
    fn local_output_drop_notice_is_inserted_before_resumed_output() {
        let mut batch = String::from("before");
        append_local_output_chunk(
            &mut batch,
            &LocalOutputChunk {
                data: "after".to_string(),
                dropped_bytes_before: 42,
                dropped_alt_screen_change: false,
            },
        );

        assert!(batch.starts_with("before\r\n[FileTerm: local terminal output dropped 42 bytes"));
        assert!(batch.ends_with("]\r\nafter"));
    }

    #[test]
    fn local_output_drop_notice_flags_alt_screen_transitions() {
        let mut batch = String::new();
        append_local_output_chunk(
            &mut batch,
            &LocalOutputChunk {
                data: "resumed".to_string(),
                dropped_bytes_before: 100,
                dropped_alt_screen_change: true,
            },
        );

        assert!(batch.contains("dropped 100 bytes"));
        assert!(batch.contains("alternate screen transitions"));
        assert!(batch.contains("reset"));
    }

    #[test]
    fn scan_alt_screen_transition_detects_common_modes() {
        // 1049 是 vim/less/nano 最常用的 alt screen 切换
        assert!(scan_alt_screen_transition("\x1b[?1049h"));
        assert!(scan_alt_screen_transition("\x1b[?1049l"));
        // 47 / 1047 是较早的 alt screen 实现
        assert!(scan_alt_screen_transition("\x1b[?47h"));
        assert!(scan_alt_screen_transition("\x1b[?1047l"));
        // 组合模式（同时设置多个私有模式）
        assert!(scan_alt_screen_transition("\x1b[?1;1049h"));
        assert!(scan_alt_screen_transition("\x1b[?47;1049h"));
    }

    #[test]
    fn scan_alt_screen_transition_ignores_unrelated_sequences() {
        // 普通光标移动、颜色等不应触发
        assert!(!scan_alt_screen_transition("\x1b[2J"));
        assert!(!scan_alt_screen_transition("\x1b[H"));
        assert!(!scan_alt_screen_transition("\x1b[31m"));
        assert!(!scan_alt_screen_transition("\x1b[?25h")); // 光标可见，不是 alt screen
        assert!(!scan_alt_screen_transition("\x1b[?2004h")); // bracketed paste
        assert!(!scan_alt_screen_transition("plain text"));
        assert!(!scan_alt_screen_transition(""));
    }

    #[test]
    fn scan_alt_screen_transition_handles_split_sequences() {
        let mut scanner = AltScreenTransitionScanner::default();
        assert!(!scanner.observe("output\x1b"));
        assert!(!scanner.observe("[?1049"));
        assert!(scanner.observe("h"));
    }

    #[test]
    fn scan_alt_screen_transition_handles_a_split_sequence_after_a_successful_chunk() {
        let mut scanner = AltScreenTransitionScanner::default();
        assert!(!scanner.observe("\x1b[?1049"));
        assert!(scanner.has_pending_sequence());
        assert!(scanner.observe("hrest"));
        assert!(!scanner.has_pending_sequence());
    }

    #[test]
    fn powershell_explicit_command_detection_accepts_abbreviations() {
        // 完整形式
        assert!(powershell_args_have_explicit_command(&[
            "-Command".to_string(),
            "Get-Date".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&[
            "-File".to_string(),
            "script.ps1".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&[
            "-EncodedCommand".to_string()
        ]));
        // PowerShell 唯一前缀缩写
        assert!(powershell_args_have_explicit_command(
            &["-Comm".to_string()]
        ));
        assert!(powershell_args_have_explicit_command(&[
            "-comma".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&["-fil".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-enc".to_string()]));
        assert!(powershell_args_have_explicit_command(&[
            "-CommandWithArgs".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(&["-cwa".to_string()]));

        // 官方短写
        assert!(powershell_args_have_explicit_command(&["-c".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-f".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-e".to_string()]));
        assert!(powershell_args_have_explicit_command(&["-ec".to_string()]));

        // 大小写不敏感
        assert!(powershell_args_have_explicit_command(&[
            "-COMMAND".to_string()
        ]));
        assert!(powershell_args_have_explicit_command(
            &["-File".to_string()]
        ));
    }

    #[test]
    fn powershell_explicit_command_detection_rejects_unrelated_args() {
        // 无关参数
        assert!(!powershell_args_have_explicit_command(&[
            "-NoLogo".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-NoExit".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[]));
        // 非 flag 参数
        assert!(!powershell_args_have_explicit_command(&[
            "script.ps1".to_string()
        ]));
        // 形似但不匹配
        assert!(!powershell_args_have_explicit_command(&[
            "-NoCommand".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-ConfigurationFile".to_string()
        ]));
        assert!(!powershell_args_have_explicit_command(&[
            "-ConfigurationName".to_string()
        ]));
    }

    #[test]
    fn cmd_explicit_command_detection_preserves_user_command_modes() {
        assert!(cmd_args_have_explicit_command(&["/c".to_string()]));
        assert!(cmd_args_have_explicit_command(&["/K".to_string()]));
        assert!(!cmd_args_have_explicit_command(&["/q".to_string()]));
        assert!(!cmd_args_have_explicit_command(&[]));
    }

    #[test]
    fn osc7_cwd_tracker_handles_split_and_percent_encoded_markers() {
        let mut tracker = LocalOsc7CwdTracker::default();
        assert_eq!(
            tracker.observe("\u{1b}]7;file://localhost/Users/stoffel/My%20Project"),
            None
        );
        assert_eq!(
            tracker.observe("\u{7}"),
            Some("/Users/stoffel/My Project".to_string())
        );

        assert_eq!(
            tracker.observe("\u{1b}]7;file:///tmp/project\u{1b}\\"),
            Some("/tmp/project".to_string())
        );
    }

    #[test]
    fn osc7_cwd_tracker_keeps_an_escape_prefix_split_across_reads() {
        let mut tracker = LocalOsc7CwdTracker::default();
        assert_eq!(tracker.observe("prompt\u{1b}]"), None);
        assert_eq!(
            tracker.observe("7;file:///tmp/next\u{7}"),
            Some("/tmp/next".to_string())
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn posix_shell_is_started_as_a_login_shell_with_terminal_capabilities() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        let argv = command.get_argv();
        assert!(argv.iter().any(|value| value.to_str() == Some("-l")));
        assert!(argv.windows(2).any(|values| {
            values[0].to_str() == Some("-o") && values[1].to_str() == Some("promptsubst")
        }));
        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("xterm-256color")
        );
        assert_eq!(
            command
                .get_env("COLORTERM")
                .and_then(|value| value.to_str()),
            Some("truecolor")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_local_environment_is_not_replaced_by_terminal_defaults() {
        let mut command = CommandBuilder::new("/bin/sh");
        let mut environment = BTreeMap::new();
        environment.insert("TERM".to_string(), "dumb".to_string());
        environment.insert("FILETERM_TEST".to_string(), "present".to_string());
        let arguments = vec!["-c".to_string(), "printf test".to_string()];

        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/sh", &arguments, &environment);

        assert_eq!(
            command.get_argv().get(1).and_then(|value| value.to_str()),
            Some("-l")
        );
        assert_eq!(
            command.get_argv().get(2).and_then(|value| value.to_str()),
            Some("-c")
        );
        assert_eq!(
            command.get_env("TERM").and_then(|value| value.to_str()),
            Some("dumb")
        );
        assert_eq!(
            command
                .get_env("FILETERM_TEST")
                .and_then(|value| value.to_str()),
            Some("present")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn bash_gets_osc7_prompt_command_injection() {
        let mut command = CommandBuilder::new("/bin/bash");
        configure_shell_command(&mut command, "/bin/bash", &[], &BTreeMap::new());

        let prompt_command = command
            .get_env("PROMPT_COMMAND")
            .and_then(|value| value.to_str())
            .expect("bash should receive a PROMPT_COMMAND");
        assert!(
            prompt_command.contains("\\033]7;"),
            "PROMPT_COMMAND should emit OSC 7: {prompt_command}"
        );
        assert!(
            prompt_command.contains("${PWD//"),
            "PROMPT_COMMAND should reference $PWD: {prompt_command}"
        );
        assert!(
            prompt_command.contains("${PWD//%/%25}"),
            "PROMPT_COMMAND should encode literal percent signs: {prompt_command}"
        );
        assert!(
            !prompt_command.contains("trap") && !prompt_command.contains("DEBUG"),
            "PROMPT_COMMAND should not install a high-frequency DEBUG trap: {prompt_command}"
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_prompt_command_is_not_overwritten_for_bash() {
        let mut command = CommandBuilder::new("/bin/bash");
        let mut environment = BTreeMap::new();
        environment.insert("PROMPT_COMMAND".to_string(), "custom-hook".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/bash", &[], &environment);

        assert_eq!(
            command
                .get_env("PROMPT_COMMAND")
                .and_then(|value| value.to_str()),
            Some("custom-hook")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zsh_gets_an_osc7_prompt_without_changing_default_prompt_style() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        let prompt = command
            .get_env("PROMPT")
            .and_then(|value| value.to_str())
            .expect("zsh should receive a default-compatible PROMPT");
        assert!(
            prompt.contains("$(printf") && prompt.contains("${PWD//%/%25}"),
            "zsh prompt should emit encoded OSC 7 with the current path: {prompt:?}"
        );
        assert!(
            prompt.ends_with("%m%# "),
            "zsh prompt should retain the default visual suffix: {prompt:?}"
        );
        assert!(command.get_argv().windows(2).any(|values| {
            values[0].to_str() == Some("-o") && values[1].to_str() == Some("promptsubst")
        }));
        assert!(command.get_env("PROMPT_COMMAND").is_none());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_zsh_prompt_is_not_overwritten() {
        let mut command = CommandBuilder::new("/bin/zsh");
        let mut environment = BTreeMap::new();
        environment.insert("PROMPT".to_string(), "custom-prompt".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/zsh", &[], &environment);

        assert_eq!(
            command.get_env("PROMPT").and_then(|value| value.to_str()),
            Some("custom-prompt")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn explicit_zsh_ps1_is_not_overwritten() {
        let mut command = CommandBuilder::new("/bin/zsh");
        let mut environment = BTreeMap::new();
        environment.insert("PS1".to_string(), "custom-prompt".to_string());
        for (name, value) in &environment {
            command.env(name, value);
        }
        configure_shell_command(&mut command, "/bin/zsh", &[], &environment);

        assert!(command.get_env("PROMPT").is_none());
        assert_eq!(
            command.get_env("PS1").and_then(|value| value.to_str()),
            Some("custom-prompt")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zsh_prompt_uses_the_terminal_program_marker_for_rc_overrides() {
        let mut command = CommandBuilder::new("/bin/zsh");
        configure_shell_command(&mut command, "/bin/zsh", &[], &BTreeMap::new());

        assert_eq!(
            command
                .get_env("TERM_PROGRAM")
                .and_then(|value| value.to_str()),
            Some("FileTerm")
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_preserves_utf8_output_and_exit_status() {
        use std::io::Read;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "printf 'FileTerm local 中文\\n'; exit 7"]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        drop(slave);
        let mut reader = master
            .try_clone_reader()
            .expect("local PTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => output.extend_from_slice(&buffer[..size]),
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });
        let _writer = master
            .take_writer()
            .expect("local PTY writer should be available");
        #[cfg(target_os = "macos")]
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(_writer);
        let status = child.wait().expect("local shell should exit");
        let output = output_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("local PTY reader should finish after shell exit");

        assert!(String::from_utf8_lossy(&output).contains("FileTerm local 中文"));
        assert_eq!(status.exit_code(), 7);
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_routes_ctrl_c_to_the_foreground_shell() {
        let (master, child, output_rx) = spawn_posix_test_pty(
            "trap 'echo FileTerm-ctrl-c; exit 42' INT; while :; do sleep 1; done",
        );
        let writer = master
            .take_writer()
            .expect("local PTY writer should be available");
        let process_tree = LocalProcessTree::attach(child.as_ref());
        let (control_tx, control_rx) = std_mpsc::channel();
        let (result_tx, result_rx) = std_mpsc::channel();
        let runner = std::thread::spawn(move || {
            let mut child = child;
            let result = run_pty_loop(control_rx, &mut child, master, writer, &process_tree);
            let _ = result_tx.send(result);
        });

        std::thread::sleep(Duration::from_millis(100));
        control_tx
            .send(LocalPtyCommand::Input("\u{3}".to_string()))
            .expect("local PTY should accept Ctrl+C input");

        let (summary, status) = match result_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(error) => {
                let _ = control_tx.send(LocalPtyCommand::Shutdown);
                let _ = result_rx.recv_timeout(Duration::from_secs(2));
                let _ = runner.join();
                panic!("Ctrl+C did not stop the local shell in time: {error}");
            }
        };
        runner
            .join()
            .expect("local PTY runner should finish after Ctrl+C");
        let output = output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("local PTY reader should finish after Ctrl+C");

        assert_eq!(status, WorkspaceTabStatus::Closed);
        assert!(
            summary.contains("42"),
            "unexpected shell summary: {summary}"
        );
        assert!(
            String::from_utf8_lossy(&output).contains("FileTerm-ctrl-c"),
            "unexpected Ctrl+C output: {:?}",
            String::from_utf8_lossy(&output)
        );
    }

    #[cfg(unix)]
    #[test]
    fn real_local_pty_can_restart_after_process_tree_shutdown() {
        let (first_master, first_child, first_output_rx) =
            spawn_posix_test_pty("printf 'FileTerm first\\n'; while :; do sleep 1; done");
        let first_writer = first_master
            .take_writer()
            .expect("first local PTY writer should be available");
        let first_process_tree = LocalProcessTree::attach(first_child.as_ref());
        let (first_control_tx, first_control_rx) = std_mpsc::channel();
        let (first_result_tx, first_result_rx) = std_mpsc::channel();
        let first_runner = std::thread::spawn(move || {
            let mut child = first_child;
            let result = run_pty_loop(
                first_control_rx,
                &mut child,
                first_master,
                first_writer,
                &first_process_tree,
            );
            let _ = first_result_tx.send(result);
        });

        std::thread::sleep(Duration::from_millis(100));
        first_control_tx
            .send(LocalPtyCommand::Shutdown)
            .expect("first local PTY should accept shutdown");
        let (first_summary, first_status) = first_result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first local PTY should stop in time");
        first_runner
            .join()
            .expect("first local PTY runner should finish");
        let first_output = first_output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first local PTY reader should finish");
        assert_eq!(first_status, WorkspaceTabStatus::Closed);
        assert_eq!(first_summary, "Local shell stopped");
        assert!(String::from_utf8_lossy(&first_output).contains("FileTerm first"));

        let (second_master, second_child, second_output_rx) =
            spawn_posix_test_pty("printf 'FileTerm second\\n'; exit 0");
        let second_writer = second_master
            .take_writer()
            .expect("second local PTY writer should be available");
        let second_process_tree = LocalProcessTree::attach(second_child.as_ref());
        let (second_control_tx, second_control_rx) = std_mpsc::channel();
        let (second_result_tx, second_result_rx) = std_mpsc::channel();
        let second_runner = std::thread::spawn(move || {
            let mut child = second_child;
            let result = run_pty_loop(
                second_control_rx,
                &mut child,
                second_master,
                second_writer,
                &second_process_tree,
            );
            let _ = second_result_tx.send(result);
        });

        let (second_summary, second_status) =
            match second_result_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(result) => result,
                Err(error) => {
                    let _ = second_control_tx.send(LocalPtyCommand::Shutdown);
                    let _ = second_runner.join();
                    panic!("second local PTY did not finish in time: {error}");
                }
            };
        second_runner
            .join()
            .expect("second local PTY runner should finish");
        let second_output = second_output_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second local PTY reader should finish");

        assert_eq!(second_status, WorkspaceTabStatus::Closed);
        assert!(second_summary.contains("code 0"));
        assert!(String::from_utf8_lossy(&second_output).contains("FileTerm second"));
    }

    #[cfg(windows)]
    #[test]
    fn real_local_conpty_preserves_output_and_exit_status() {
        use std::io::Read;

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local ConPTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/C", "echo FileTerm local && exit /B 7"]);
        let mut child = slave
            .spawn_command(command)
            .expect("cmd.exe should start in ConPTY");
        drop(slave);
        let mut reader = master
            .try_clone_reader()
            .expect("ConPTY reader should clone");
        let (output_tx, output_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut output = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => output.extend_from_slice(&buffer[..size]),
                    Err(_) => break,
                }
            }
            let _ = output_tx.send(output);
        });
        let writer = master
            .take_writer()
            .expect("ConPTY writer should be available");
        drop(writer);
        let status = child.wait().expect("cmd.exe should exit");
        let output = output_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("ConPTY reader should finish after shell exit");

        assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
        assert_eq!(status.exit_code(), 7);
    }

    #[cfg(unix)]
    #[test]
    fn local_process_tree_terminates_shell_process_group() {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master: _, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 30"]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        let process_tree = LocalProcessTree::attach(child.as_ref());

        process_tree.terminate(child.as_mut());
        let status = child.wait().expect("terminated shell should be reapable");
        assert!(!status.success());
    }

    /// 验证进程组终止能收掉孙进程，而不只是直接子 shell。
    /// 修复前实现依赖 portable_pty 的 forkpty 调了 setsid()，但测试没显式
    /// 覆盖 grandchild，回归会漏掉。
    #[cfg(unix)]
    #[test]
    fn local_process_tree_terminates_grandchild_process() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let pid_file = std::env::temp_dir().join(format!(
            "fileterm-grandchild-{}-{}.pid",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_file(&pid_file);

        // 启动一个后台 sleep（grandchild），把 pid 写到文件，shell wait 它。
        let script = format!(
            "sleep 30 &\necho $! > {pid_file}\nwait\n",
            pid_file = pid_file.display()
        );

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local PTY should open in the test environment");
        let portable_pty::PtyPair { master: _, slave } = pair;
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", &script]);
        let mut child = slave
            .spawn_command(command)
            .expect("local shell should start in a PTY");
        let process_tree = LocalProcessTree::attach(child.as_ref());

        // 等 grandchild pid 落盘
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let grandchild_pid = loop {
            if let Ok(content) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = content.trim().parse::<libc::pid_t>() {
                    if pid > 0 {
                        break pid;
                    }
                }
            }
            if std::time::Instant::now() > deadline {
                let _ = fs::remove_file(&pid_file);
                panic!("grandchild pid was not recorded in time");
            }
            std::thread::sleep(Duration::from_millis(50));
        };

        process_tree.terminate(child.as_mut());
        child.wait().expect("terminated shell should be reapable");

        // 给 SIGHUP/SIGKILL 时间生效，并容忍 init 回收孤儿的延迟。
        // kill -0 对僵尸进程也返回 0，所以需要轮询直到进程真正消失，
        // 避免 CI 高负载下 init 回收慢导致误报。
        let mut still_alive = true;
        for attempt in 0..15 {
            if unsafe { libc::kill(grandchild_pid, 0) != 0 } {
                still_alive = false;
                break;
            }
            if attempt < 14 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = fs::remove_file(&pid_file);
        assert!(
            !still_alive,
            "grandchild (pid={grandchild_pid}) survived process tree termination after 1.5s"
        );
    }

    #[cfg(windows)]
    #[test]
    fn local_process_tree_terminates_conpty_job() {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("local ConPTY should open in the test environment");
        let portable_pty::PtyPair { master, slave } = pair;
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/C", "ping 127.0.0.1 -n 30 > nul"]);
        let mut child = slave
            .spawn_command(command)
            .expect("cmd.exe should start in ConPTY");
        drop(slave);
        let _master = master;
        let process_tree = LocalProcessTree::attach(child.as_ref());

        process_tree.terminate(child.as_mut());
        let status = child
            .wait()
            .expect("terminated ConPTY process should be reapable");
        assert!(!status.success());
    }
}
