use crate::{
    services::{WorkspaceState, WorkspaceTabStatus},
    sessions::{
        terminal::{emit_terminal_data, set_terminal_state},
        WorkerCmd,
    },
};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::{
    env,
    io::{Read, Write},
    path::PathBuf,
    sync::mpsc as std_mpsc,
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const DEFAULT_COLS: u16 = 80;
const DEFAULT_ROWS: u16 = 24;
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Debug)]
pub struct LocalTerminalLaunch {
    pub shell: String,
    pub cwd: String,
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

pub fn default_launch() -> LocalTerminalLaunch {
    LocalTerminalLaunch {
        shell: default_shell(),
        cwd: default_working_directory(),
    }
}

pub fn start_local_terminal_worker(
    tab_id: String,
    runtime_id: String,
    worker_rx: mpsc::Receiver<WorkerCmd>,
    terminal_input_rx: mpsc::UnboundedReceiver<String>,
    app: AppHandle,
    cancellation: CancellationToken,
    launch: LocalTerminalLaunch,
) -> Result<(), String> {
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
    #[cfg(target_os = "windows")]
    if launch.shell.eq_ignore_ascii_case("powershell.exe") {
        command.arg("-NoLogo");
    }
    command.cwd(&launch.cwd);
    command.env("TERM", "xterm-256color");

    let mut child = slave
        .spawn_command(command)
        .map_err(|error| format!("Unable to start local shell {}: {error}", launch.shell))?;
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

    let (control_tx, control_rx) = std_mpsc::channel::<LocalPtyCommand>();
    let relay_tx = control_tx.clone();
    tauri::async_runtime::spawn(async move {
        forward_terminal_commands(worker_rx, terminal_input_rx, cancellation, relay_tx).await;
    });

    let reader_app = app.clone();
    let reader_tab_id = tab_id.clone();
    thread::Builder::new()
        .name("fileterm-local-pty-reader".to_string())
        .spawn(move || {
            let mut reader = reader;
            let mut buffer = [0_u8; 8 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let chunk = String::from_utf8_lossy(&buffer[..size]).into_owned();
                        tauri::async_runtime::block_on(emit_terminal_data(
                            &reader_app,
                            &reader_tab_id,
                            &chunk,
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
        .map_err(|error| {
            let _ = child.kill();
            format!("Unable to start local PTY reader: {error}")
        })?;

    thread::Builder::new()
        .name("fileterm-local-pty".to_string())
        .spawn(move || {
            let (summary, status) = run_pty_loop(control_rx, &mut child, master, writer);
            tauri::async_runtime::block_on(async move {
                if cleanup_local_terminal_runtime(&app, &tab_id, &runtime_id).await {
                    set_terminal_state(&app, &tab_id, summary, status).await;
                }
            });
        })
        .map_err(|error| format!("Unable to start local PTY worker: {error}"))?;

    Ok(())
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
) -> (String, WorkspaceTabStatus) {
    loop {
        match control_rx.recv_timeout(CONTROL_POLL_INTERVAL) {
            Ok(LocalPtyCommand::Input(data)) => {
                if let Err(error) = writer
                    .write_all(data.as_bytes())
                    .and_then(|()| writer.flush())
                {
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
                let _ = master.resize(size);
            }
            Ok(LocalPtyCommand::Shutdown) | Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                return (
                    "Local shell stopped".to_string(),
                    WorkspaceTabStatus::Closed,
                );
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => {}
        }

        match child.try_wait() {
            Ok(Some(_)) => {
                return ("Local shell exited".to_string(), WorkspaceTabStatus::Closed);
            }
            Ok(None) => {}
            Err(error) => {
                return (
                    format!("Unable to observe local shell: {error}"),
                    WorkspaceTabStatus::Error,
                );
            }
        }
    }
}

async fn cleanup_local_terminal_runtime(app: &AppHandle, tab_id: &str, runtime_id: &str) -> bool {
    let state = app.state::<WorkspaceState>();
    let owns_runtime = state
        .local_terminal_runtime_ids
        .read()
        .await
        .get(tab_id)
        .is_some_and(|current_id| current_id == runtime_id);
    if !owns_runtime {
        return false;
    }
    state.terminal_inputs.write().await.remove(tab_id);
    state.workers.write().await.remove(tab_id);
    state.worker_controls.write().await.remove(tab_id);
    state
        .local_terminal_runtime_ids
        .write()
        .await
        .remove(tab_id);
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
    "powershell.exe".to_string()
}

#[cfg(not(target_os = "windows"))]
fn default_shell() -> String {
    env::var("SHELL")
        .ok()
        .filter(|shell| !shell.trim().is_empty())
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

#[cfg(test)]
mod tests {
    use super::clamp_u16;

    #[test]
    fn pty_size_clamps_to_platform_u16_values() {
        assert_eq!(clamp_u16(0, 80), 80);
        assert_eq!(clamp_u16(120, 80), 120);
        assert_eq!(clamp_u16(u32::MAX, 80), u16::MAX);
    }
}
