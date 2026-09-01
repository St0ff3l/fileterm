// Stateful MCP remote-command registry.
//
// A normal MCP tool call is intentionally bounded and returns one result. A
// deployment, image build, or migration is different: the caller needs a
// stable command identity while output continues to arrive. This registry
// keeps the SSH channel and its bounded output buffer in the desktop process,
// so the individual loopback request is no longer the lifetime of the remote job.

use std::cmp::min;
use std::time::{SystemTime, UNIX_EPOCH};

use russh::{ChannelMsg, ChannelReadHalf, ChannelWriteHalf};
use tokio::sync::{Mutex as AsyncMutex, Notify, RwLock};
use tokio::time::sleep_until;

const MAX_BACKGROUND_REMOTE_COMMANDS: usize = 128;
/// Mirror ssh-mcp's default `sessionMaxPerConnection`: one FileTerm tab must
/// not fan out an unbounded number of detached exec channels while a deploy
/// agent is polling or retrying requests.
const MAX_ACTIVE_BACKGROUND_REMOTE_COMMANDS_PER_TAB: usize = 5;
const BACKGROUND_REMOTE_COMMAND_RETENTION: Duration = Duration::from_secs(30 * 60);
const BACKGROUND_REMOTE_COMMAND_OUTPUT_CAP: usize = 256 * 1024;
const BACKGROUND_REMOTE_COMMAND_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
const BACKGROUND_REMOTE_COMMAND_SIGNAL_TIMEOUT: Duration = Duration::from_millis(300);
const BACKGROUND_REMOTE_COMMAND_TERMINATION_WAIT: Duration = Duration::from_secs(2);
const BACKGROUND_REMOTE_COMMAND_MAX_READ_BYTES: usize = 64 * 1024;

pub(crate) type BackgroundRemoteCommandWriter = ChannelWriteHalf<russh::client::Msg>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemoteCommandStart {
    pub command_id: String,
    pub tab_id: String,
    pub started_at: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemoteCommandSnapshot {
    pub command_id: String,
    pub tab_id: String,
    pub output: String,
    pub next_offset: u64,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub exit_signal: Option<String>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub output_truncated: bool,
    pub started_at: u64,
    pub finished_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemoteCommandSummary {
    pub command_id: String,
    pub tab_id: String,
    pub output_bytes: u64,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub exit_signal: Option<String>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub output_truncated: bool,
    pub started_at: u64,
    pub finished_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundRemoteCommandPage {
    pub total: usize,
    pub count: usize,
    pub offset: usize,
    pub items: Vec<BackgroundRemoteCommandSummary>,
    pub has_more: bool,
    pub next_offset: Option<usize>,
}

#[derive(Debug)]
struct BackgroundRemoteCommandState {
    output: Vec<u8>,
    running: bool,
    exit_code: Option<u32>,
    exit_signal: Option<String>,
    timed_out: bool,
    cancelled: bool,
    output_truncated: bool,
    finished_at: Option<u64>,
}

#[derive(Debug)]
struct BackgroundRemoteCommand {
    command_id: String,
    tab_id: String,
    started_at: u64,
    writer: AsyncMutex<Option<BackgroundRemoteCommandWriter>>,
    state: AsyncMutex<BackgroundRemoteCommandState>,
    cancellation: CancellationToken,
    notify: Notify,
}

/// Owns the lifetime of MCP background commands independently from the
/// short-lived loopback bridge connection. The SSH worker supplies the already
/// opened channel halves after the command has been accepted by the server.
#[derive(Debug, Default)]
pub struct BackgroundRemoteCommandRegistry {
    commands: RwLock<HashMap<String, Arc<BackgroundRemoteCommand>>>,
}

impl BackgroundRemoteCommandRegistry {
    /// Fast-fail before opening a remote channel when the tab or registry is
    /// already at capacity. `register` repeats the check after channel setup
    /// to close the small race between concurrent starts safely.
    pub(crate) async fn ensure_capacity(&self, tab_id: &str) -> Result<(), String> {
        self.prune_finished().await;
        let commands = self.commands.read().await;
        if commands.len() >= MAX_BACKGROUND_REMOTE_COMMANDS {
            return Err(format!(
                "{FILETERM_REMOTE_COMMAND_LIMIT}: too many background remote commands"
            ));
        }
        if count_active_commands_for_tab(&commands, tab_id).await
            >= MAX_ACTIVE_BACKGROUND_REMOTE_COMMANDS_PER_TAB
        {
            return Err(format!(
                "{FILETERM_REMOTE_COMMAND_SESSION_LIMIT}: tab has too many active background remote commands"
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn register(
        &self,
        tab_id: String,
        started_at: u64,
        timeout_duration: Duration,
        reader: ChannelReadHalf,
        writer: BackgroundRemoteCommandWriter,
        pending_pty_stdin: Option<Vec<u8>>,
        lifetime_cancellation: CancellationToken,
    ) -> Result<BackgroundRemoteCommandStart, String> {
        self.prune_finished().await;
        let command_id = uuid::Uuid::new_v4().simple().to_string();
        let cancellation = CancellationToken::new();
        let command = Arc::new(BackgroundRemoteCommand {
            command_id: command_id.clone(),
            tab_id: tab_id.clone(),
            started_at,
            writer: AsyncMutex::new(Some(writer)),
            state: AsyncMutex::new(BackgroundRemoteCommandState {
                output: Vec::new(),
                running: true,
                exit_code: None,
                exit_signal: None,
                timed_out: false,
                cancelled: false,
                output_truncated: false,
                finished_at: None,
            }),
            cancellation: cancellation.clone(),
            notify: Notify::new(),
        });

        {
            let mut commands = self.commands.write().await;
            if commands.len() >= MAX_BACKGROUND_REMOTE_COMMANDS {
                drop(commands);
                terminate_unregistered_command(&command).await;
                return Err(format!(
                    "{FILETERM_REMOTE_COMMAND_LIMIT}: too many background remote commands"
                ));
            }
            if count_active_commands_for_tab(&commands, &tab_id).await
                >= MAX_ACTIVE_BACKGROUND_REMOTE_COMMANDS_PER_TAB
            {
                drop(commands);
                terminate_unregistered_command(&command).await;
                return Err(format!(
                    "{FILETERM_REMOTE_COMMAND_SESSION_LIMIT}: tab has too many active background remote commands"
                ));
            }
            commands.insert(command_id.clone(), Arc::clone(&command));
        }

        tokio::spawn(run_background_remote_command(
            Arc::clone(&command),
            reader,
            timeout_duration,
            pending_pty_stdin,
            lifetime_cancellation,
        ));

        Ok(BackgroundRemoteCommandStart {
            command_id,
            tab_id,
            started_at,
        })
    }

    pub(crate) async fn read(
        &self,
        tab_id: &str,
        command_id: &str,
        offset: u64,
        max_bytes: usize,
        wait: Duration,
    ) -> Result<BackgroundRemoteCommandSnapshot, String> {
        let command = self.lookup_for_tab(tab_id, command_id).await?;
        let max_bytes = max_bytes.clamp(1, BACKGROUND_REMOTE_COMMAND_MAX_READ_BYTES);
        let deadline = tokio::time::Instant::now() + wait;

        loop {
            // Create the notification future before taking the snapshot. If
            // output arrives between these operations, Notify retains the
            // wake-up and the read cannot sleep through it.
            let notified = command.notify.notified();
            let snapshot = snapshot_command(&command, offset, max_bytes).await;
            if !snapshot.running
                || snapshot.next_offset > offset
                || wait.is_zero()
                || tokio::time::Instant::now() >= deadline
            {
                return Ok(snapshot);
            }

            tokio::select! {
                _ = notified => {}
                _ = sleep_until(deadline) => return Ok(snapshot),
            }
        }
    }

    pub(crate) async fn list(
        &self,
        tab_id: &str,
        limit: usize,
        offset: usize,
    ) -> BackgroundRemoteCommandPage {
        self.prune_finished().await;
        let commands = self.commands.read().await;
        let mut summaries = Vec::new();
        for command in commands.values().filter(|command| command.tab_id == tab_id) {
            let state = command.state.lock().await;
            summaries.push(BackgroundRemoteCommandSummary {
                command_id: command.command_id.clone(),
                tab_id: command.tab_id.clone(),
                output_bytes: state.output.len() as u64,
                running: state.running,
                exit_code: state.exit_code,
                exit_signal: state.exit_signal.clone(),
                timed_out: state.timed_out,
                cancelled: state.cancelled,
                output_truncated: state.output_truncated,
                started_at: command.started_at,
                finished_at: state.finished_at,
            });
        }
        summaries.sort_by(|left, right| {
            left.started_at
                .cmp(&right.started_at)
                .then_with(|| left.command_id.cmp(&right.command_id))
        });

        let total = summaries.len();
        let items = summaries
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        let next_offset = offset + items.len();
        BackgroundRemoteCommandPage {
            total,
            count: items.len(),
            offset,
            items,
            has_more: next_offset < total,
            next_offset: (next_offset < total).then_some(next_offset),
        }
    }

    pub(crate) async fn terminate(
        &self,
        tab_id: &str,
        command_id: &str,
    ) -> Result<BackgroundRemoteCommandSnapshot, String> {
        let command = self.lookup_for_tab(tab_id, command_id).await?;
        let notified = command.notify.notified();
        let running = command.state.lock().await.running;
        if running {
            command.cancellation.cancel();
            terminate_writer(&command).await;
            if command.state.lock().await.running {
                let _ = tokio::time::timeout(
                    BACKGROUND_REMOTE_COMMAND_TERMINATION_WAIT,
                    notified,
                )
                .await;
            }
        }
        Ok(snapshot_command(&command, 0, BACKGROUND_REMOTE_COMMAND_MAX_READ_BYTES).await)
    }

    pub(crate) async fn close(&self, tab_id: &str, command_id: &str) -> Result<(), String> {
        let command = self.lookup_for_tab(tab_id, command_id).await?;
        let notified = command.notify.notified();
        let running = command.state.lock().await.running;
        command.cancellation.cancel();
        terminate_writer(&command).await;
        if running && command.state.lock().await.running {
            let _ = tokio::time::timeout(
                BACKGROUND_REMOTE_COMMAND_TERMINATION_WAIT,
                notified,
            )
            .await;
        }
        let mut commands = self.commands.write().await;
        // Keep the same identity guard as ssh-mcp's session manager: an old
        // close task must never remove a newer entry if IDs are ever reused.
        if commands
            .get(command_id)
            .is_some_and(|candidate| Arc::ptr_eq(candidate, &command))
        {
            commands.remove(command_id);
        }
        Ok(())
    }

    async fn lookup_for_tab(
        &self,
        tab_id: &str,
        command_id: &str,
    ) -> Result<Arc<BackgroundRemoteCommand>, String> {
        let command = self
            .commands
            .read()
            .await
            .get(command_id)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "{FILETERM_REMOTE_COMMAND_NOT_FOUND}: background command was not found"
                )
            })?;
        if command.tab_id != tab_id {
            return Err(format!(
                "{FILETERM_REMOTE_COMMAND_SCOPE_MISMATCH}: command belongs to another session"
            ));
        }
        Ok(command)
    }

    async fn prune_finished(&self) {
        let candidates = self
            .commands
            .read()
            .await
            .iter()
            .map(|(id, command)| (id.clone(), Arc::clone(command)))
            .collect::<Vec<_>>();
        let now = SystemTime::now();
        let mut expired = Vec::new();
        for (id, command) in candidates {
            let state = command.state.lock().await;
            let Some(finished_at) = state.finished_at else {
                continue;
            };
            let finished_at = UNIX_EPOCH + Duration::from_millis(finished_at);
            if now
                .duration_since(finished_at)
                .unwrap_or_default()
                >= BACKGROUND_REMOTE_COMMAND_RETENTION
            {
                expired.push(id);
            }
        }
        if expired.is_empty() {
            return;
        }
        let mut commands = self.commands.write().await;
        for id in expired {
            commands.remove(&id);
        }
    }
}

async fn count_active_commands_for_tab(
    commands: &HashMap<String, Arc<BackgroundRemoteCommand>>,
    tab_id: &str,
) -> usize {
    let mut active = 0;
    for command in commands.values().filter(|command| command.tab_id == tab_id) {
        if command.state.lock().await.running {
            active += 1;
        }
    }
    active
}

async fn run_background_remote_command(
    command: Arc<BackgroundRemoteCommand>,
    mut reader: ChannelReadHalf,
    timeout_duration: Duration,
    mut pending_pty_stdin: Option<Vec<u8>>,
    lifetime_cancellation: CancellationToken,
) {
    let command_deadline = tokio::time::Instant::now() + timeout_duration;
    let mut drain_deadline = None;
    let mut prompt_window = Vec::new();
    let mut exit_code = None;
    let mut exit_signal = None;
    let mut timed_out = false;
    let mut cancelled = false;

    loop {
        let deadline = drain_deadline
            .map(|drain| min(drain, command_deadline))
            .unwrap_or(command_deadline);
        let message = tokio::select! {
            message = reader.wait() => message,
            _ = sleep_until(deadline) => {
                timed_out = drain_deadline.is_none();
                if timed_out {
                    terminate_writer(&command).await;
                }
                break;
            }
            _ = command.cancellation.cancelled() => {
                cancelled = true;
                terminate_writer(&command).await;
                break;
            }
            _ = lifetime_cancellation.cancelled() => {
                cancelled = true;
                terminate_writer(&command).await;
                break;
            }
        };

        let Some(message) = message else {
            break;
        };
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                append_output(&command, data.as_ref()).await;
                if pending_pty_stdin.is_some() {
                    append_prompt_window(&mut prompt_window, data.as_ref());
                    if crate::sessions::system_metrics::pty_password_prompt_detected(&prompt_window)
                    {
                        let stdin = pending_pty_stdin
                            .take()
                            .expect("pending PTY input was checked above");
                        let writer = command.writer.lock().await;
                        if let Some(writer) = writer.as_ref() {
                            let _ = timeout(
                                BACKGROUND_REMOTE_COMMAND_SIGNAL_TIMEOUT,
                                writer.data_bytes(stdin),
                            )
                            .await;
                            let _ = timeout(
                                BACKGROUND_REMOTE_COMMAND_SIGNAL_TIMEOUT,
                                writer.data_bytes(vec![0x04]),
                            )
                            .await;
                        }
                    }
                }
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status);
                drain_deadline = Some(
                    tokio::time::Instant::now() + BACKGROUND_REMOTE_COMMAND_DRAIN_TIMEOUT,
                );
            }
            ChannelMsg::ExitSignal {
                signal_name, ..
            } => {
                exit_signal = Some(format!("{signal_name:?}"));
                drain_deadline = Some(
                    tokio::time::Instant::now() + BACKGROUND_REMOTE_COMMAND_DRAIN_TIMEOUT,
                );
            }
            ChannelMsg::Eof | ChannelMsg::Close => break,
            _ => {}
        }
    }

    {
        let mut state = command.state.lock().await;
        state.running = false;
        state.exit_code = exit_code;
        state.exit_signal = exit_signal;
        state.timed_out = timed_out;
        state.cancelled = cancelled;
        state.finished_at = Some(now_millis());
    }
    if let Some(writer) = command.writer.lock().await.take() {
        let _ = close_writer(&writer).await;
    }
    command.notify.notify_waiters();
}

async fn append_output(command: &BackgroundRemoteCommand, chunk: &[u8]) {
    let mut state = command.state.lock().await;
    if !state.output_truncated {
        let remaining = BACKGROUND_REMOTE_COMMAND_OUTPUT_CAP.saturating_sub(state.output.len());
        if chunk.len() <= remaining {
            state.output.extend_from_slice(chunk);
        } else {
            state.output.extend_from_slice(&chunk[..remaining]);
            state.output_truncated = true;
        }
    }
    drop(state);
    command.notify.notify_waiters();
}

async fn snapshot_command(
    command: &BackgroundRemoteCommand,
    offset: u64,
    max_bytes: usize,
) -> BackgroundRemoteCommandSnapshot {
    let state = command.state.lock().await;
    let start = min(offset, state.output.len() as u64) as usize;
    let end = min(start.saturating_add(max_bytes), state.output.len());
    BackgroundRemoteCommandSnapshot {
        command_id: command.command_id.clone(),
        tab_id: command.tab_id.clone(),
        output: String::from_utf8_lossy(&state.output[start..end]).into_owned(),
        next_offset: end as u64,
        running: state.running,
        exit_code: state.exit_code,
        exit_signal: state.exit_signal.clone(),
        timed_out: state.timed_out,
        cancelled: state.cancelled,
        output_truncated: state.output_truncated,
        started_at: command.started_at,
        finished_at: state.finished_at,
    }
}

async fn terminate_writer(command: &BackgroundRemoteCommand) {
    let writer = command.writer.lock().await;
    if let Some(writer) = writer.as_ref() {
        let _ = crate::sessions::system_metrics::terminate_exec_channel_writer(writer).await;
    }
}

async fn terminate_unregistered_command(command: &BackgroundRemoteCommand) {
    let writer = command.writer.lock().await;
    if let Some(writer) = writer.as_ref() {
        let _ = crate::sessions::system_metrics::terminate_exec_channel_writer(writer).await;
    }
}

async fn close_writer(writer: &BackgroundRemoteCommandWriter) -> bool {
    timeout(
        BACKGROUND_REMOTE_COMMAND_SIGNAL_TIMEOUT,
        writer.close(),
    )
    .await
    .is_ok_and(|result| result.is_ok())
}

fn append_prompt_window(window: &mut Vec<u8>, chunk: &[u8]) {
    const PROMPT_WINDOW_BYTES: usize = 2 * 1024;
    window.extend_from_slice(chunk);
    if window.len() > PROMPT_WINDOW_BYTES {
        let keep_from = window.len() - PROMPT_WINDOW_BYTES;
        window.drain(..keep_from);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
