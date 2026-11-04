use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{channel, error::TrySendError, Sender};

use super::{format::build_line, LogCategory, LogLevel};

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 1024;
const WARN_RESERVE: usize = 128;
const ERROR_RESERVE: usize = 32;
const DROP_REPORT_INTERVAL: Duration = Duration::from_secs(30);
static WRITER: OnceLock<Option<Writer>> = OnceLock::new();

enum Message {
    Line {
        directory: PathBuf,
        level: LogLevel,
        line: String,
    },
    Flush(mpsc::Sender<()>),
}

#[derive(Default)]
struct DroppedLogs {
    low_priority: AtomicUsize,
    warn: AtomicUsize,
    error: AtomicUsize,
}

impl DroppedLogs {
    fn record(&self, level: LogLevel) {
        let counter = match level {
            LogLevel::Error => &self.error,
            LogLevel::Warn => &self.warn,
            _ => &self.low_priority,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }

    fn report(
        &self,
        directory: &Path,
        write: &mut impl FnMut(&Path, &str) -> std::io::Result<()>,
        last_report: &mut Option<Instant>,
        force: bool,
    ) {
        if !force && last_report.is_some_and(|last| last.elapsed() < DROP_REPORT_INTERVAL) {
            return;
        }
        let counters = [&self.low_priority, &self.warn, &self.error];
        let dropped = counters.map(|counter| counter.swap(0, Ordering::Relaxed));
        if dropped == [0; 3] {
            return;
        }
        let level = if dropped[2] > 0 {
            LogLevel::Error
        } else {
            LogLevel::Warn
        };
        *last_report = Some(Instant::now());
        let written = level.enabled()
            && write(
                directory,
                &build_line(
                    level,
                    LogCategory::Logging,
                    "logging",
                    &format!(
                        "diagnostic logs dropped dropped_low_priority={} dropped_warn={} dropped_error={} capacity={QUEUE_CAPACITY}",
                        dropped[0], dropped[1], dropped[2]
                    ),
                ),
            )
            .is_ok();
        if !written {
            for (counter, count) in counters.into_iter().zip(dropped) {
                counter.fetch_add(count, Ordering::Relaxed);
            }
        }
    }
}

struct Writer {
    sender: Sender<Message>,
    dropped: Arc<DroppedLogs>,
}

impl Writer {
    fn start(
        directory: PathBuf,
        mut write: impl FnMut(&Path, &str) -> std::io::Result<()> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, mut receiver) = channel::<Message>(QUEUE_CAPACITY);
        let dropped = Arc::new(DroppedLogs::default());
        let worker_dropped = Arc::clone(&dropped);
        std::thread::Builder::new()
            .name("fileterm-log-writer".into())
            .spawn(move || {
                let mut last_report = None;
                while let Some(message) = receiver.blocking_recv() {
                    match message {
                        Message::Line {
                            directory: target,
                            level,
                            line,
                        } => {
                            if write(&target, &line).is_err() {
                                worker_dropped.record(level);
                            }
                            worker_dropped.report(&directory, &mut write, &mut last_report, false);
                        }
                        Message::Flush(acknowledge) => {
                            worker_dropped.report(&directory, &mut write, &mut last_report, true);
                            let _ = acknowledge.send(());
                        }
                    }
                }
            })?;
        Ok(Self { sender, dropped })
    }

    fn enqueue(&self, directory: PathBuf, level: LogLevel, line: String) {
        let Ok(permit) = self.sender.try_reserve() else {
            self.dropped.record(level);
            return;
        };
        let reserved = match level {
            LogLevel::Error => 0,
            LogLevel::Warn => ERROR_RESERVE,
            _ => WARN_RESERVE + ERROR_RESERVE,
        };
        // Admission reserves slots for severe logs without changing FIFO order.
        if self.sender.capacity() < reserved {
            self.dropped.record(level);
            return;
        }
        permit.send(Message::Line {
            directory,
            level,
            line,
        });
    }

    fn flush(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let (acknowledge, receiver) = mpsc::channel();
        let mut message = Message::Flush(acknowledge);
        loop {
            match self.sender.try_send(message) {
                Ok(()) => break,
                Err(TrySendError::Full(returned)) if Instant::now() < deadline => {
                    message = returned;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(_) => return,
            }
        }
        let _ = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

pub(super) fn init(directory: PathBuf) {
    WRITER.get_or_init(|| Writer::start(directory, append_sync).ok());
}

fn append_sync(directory: &Path, line: &str) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let path = directory.join("app.log");
    // Rotate before the next line exceeds the limit, keeping each record intact.
    if fs::metadata(&path)
        .map(|metadata| metadata.len().saturating_add(line.len() as u64) > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let backup = directory.join("app.log.1");
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(&path, backup)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

pub(super) fn dispatch(directory: PathBuf, level: LogLevel, line: String) {
    init(directory.clone());
    if let Some(writer) = WRITER.get().and_then(Option::as_ref) {
        writer.enqueue(directory, level, line);
    }
}

pub(super) fn flush(timeout: Duration) {
    if let Some(writer) = WRITER.get().and_then(Option::as_ref) {
        writer.flush(timeout);
    }
}

#[cfg(test)]
mod tests;
