use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::{format::build_line, LogCategory, LogLevel};

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 1024;
static LOG_LOCK: Mutex<()> = Mutex::new(());
static WRITER: OnceLock<Option<SyncSender<Message>>> = OnceLock::new();
static DROPPED_LOW_PRIORITY: AtomicUsize = AtomicUsize::new(0);
static EMERGENCY_WRITES: AtomicUsize = AtomicUsize::new(0);

enum Message {
    Line { directory: PathBuf, line: String },
    Flush(mpsc::Sender<()>),
}

pub(super) fn init(directory: PathBuf) {
    WRITER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<Message>(QUEUE_CAPACITY);
        let result = std::thread::Builder::new()
            .name("fileterm-log-writer".into())
            .spawn(move || {
                while let Ok(message) = receiver.recv() {
                    report_dropped(&directory);
                    match message {
                        Message::Line { directory, line } => append_sync(&directory, &line),
                        Message::Flush(acknowledge) => {
                            let _ = acknowledge.send(());
                        }
                    }
                }
            });
        result.ok().map(|_| sender)
    });
}

fn report_dropped(directory: &Path) {
    let dropped = DROPPED_LOW_PRIORITY.swap(0, Ordering::Relaxed);
    if dropped == 0 || !LogLevel::Warn.enabled() {
        return;
    }
    let line = build_line(
        LogLevel::Warn,
        LogCategory::Logging,
        "logging",
        &format!("queue saturated dropped_low_priority={dropped} capacity={QUEUE_CAPACITY}"),
    );
    append_sync(directory, &line);
}

fn append_sync(directory: &Path, line: &str) {
    let Ok(_guard) = LOG_LOCK.lock() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let path = directory.join("app.log");
    // Rotate before the next line exceeds the limit, keeping each record intact.
    if fs::metadata(&path)
        .map(|metadata| metadata.len().saturating_add(line.len() as u64) > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let backup = directory.join("app.log.1");
        if backup.exists() && fs::remove_file(&backup).is_err() {
            return;
        }
        if fs::rename(&path, backup).is_err() {
            return;
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

pub(super) fn dispatch(directory: PathBuf, level: LogLevel, line: String) {
    init(directory.clone());
    let message = Message::Line { directory, line };
    let Some(sender) = WRITER.get().and_then(Option::as_ref) else {
        emergency_write(message);
        return;
    };
    match sender.try_send(message) {
        Ok(()) => {}
        Err(TrySendError::Full(message)) if level < LogLevel::Warn => {
            // A bounded queue limits memory and never blocks protocol workers.
            drop(message);
            DROPPED_LOW_PRIORITY.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Full(message) | TrySendError::Disconnected(message)) => {
            // Retain WARN/ERROR using the former background-write path on saturation.
            emergency_write(message);
        }
    }
}

fn emergency_write(message: Message) {
    let Message::Line { directory, line } = message else {
        return;
    };
    EMERGENCY_WRITES.fetch_add(1, Ordering::SeqCst);
    let write = move || {
        append_sync(&directory, &line);
        EMERGENCY_WRITES.fetch_sub(1, Ordering::SeqCst);
    };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn_blocking(write);
    } else if std::thread::Builder::new()
        .name("fileterm-log-fallback".into())
        .spawn(write)
        .is_err()
    {
        EMERGENCY_WRITES.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(super) fn flush(timeout: Duration) {
    let Some(sender) = WRITER.get().and_then(Option::as_ref) else {
        return;
    };
    let deadline = Instant::now() + timeout;
    let (acknowledge, receiver) = mpsc::channel();
    let mut message = Message::Flush(acknowledge);
    loop {
        match sender.try_send(message) {
            Ok(()) => break,
            Err(TrySendError::Full(returned)) if Instant::now() < deadline => {
                message = returned;
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return,
        }
    }
    if receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_err()
    {
        return;
    }
    while EMERGENCY_WRITES.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
}
