use super::*;
use std::collections::HashSet;
use std::sync::Mutex;

struct StalledWriter {
    writer: Writer,
    release: Option<mpsc::Sender<()>>,
    lines: Arc<Mutex<Vec<String>>>,
    threads: Arc<Mutex<HashSet<std::thread::ThreadId>>>,
}

impl StalledWriter {
    fn new() -> Self {
        let (started, ready) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&lines);
        let threads = Arc::new(Mutex::new(HashSet::new()));
        let worker_threads = Arc::clone(&threads);
        let mut first = true;
        let writer = Writer::start(PathBuf::from("unused"), move |_, line| {
            worker_threads
                .lock()
                .unwrap()
                .insert(std::thread::current().id());
            if first {
                first = false;
                started.send(()).unwrap();
                let _ = resume.recv();
            }
            output.lock().unwrap().push(line.to_owned());
            Ok(())
        })
        .unwrap();
        writer.enqueue(PathBuf::from("unused"), LogLevel::Info, "first".into());
        ready.recv_timeout(Duration::from_secs(2)).unwrap();
        Self {
            writer,
            release: Some(release),
            lines,
            threads,
        }
    }

    fn resume(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for StalledWriter {
    fn drop(&mut self) {
        self.resume();
    }
}

#[test]
fn stalled_disk_reserves_warn_and_error_capacity_preserves_fifo_and_bounds_flush() {
    let mut fixture = StalledWriter::new();
    let mut expected = vec!["first".to_owned()];
    for (level, capacity) in [
        (
            LogLevel::Debug,
            QUEUE_CAPACITY - WARN_RESERVE - ERROR_RESERVE,
        ),
        (LogLevel::Warn, WARN_RESERVE),
        (LogLevel::Error, ERROR_RESERVE),
    ] {
        for index in 0..capacity {
            let line = format!("queued-{}-{index}", level.as_str());
            expected.push(line.clone());
            fixture.writer.enqueue(PathBuf::from("unused"), level, line);
        }
        fixture
            .writer
            .enqueue(PathBuf::from("unused"), level, "overflow".into());
    }
    assert_eq!(fixture.writer.sender.capacity(), 0);
    let started = Instant::now();
    fixture.writer.flush(Duration::from_millis(20));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(fixture.lines.lock().unwrap().len(), 0);
    fixture.resume();
    fixture.writer.flush(Duration::from_secs(5));
    assert_eq!(fixture.writer.sender.capacity(), QUEUE_CAPACITY);
    let lines = fixture.lines.lock().unwrap();
    let delivered: Vec<_> = lines
        .iter()
        .filter(|line| !line.contains("diagnostic logs dropped"))
        .cloned()
        .collect();
    assert_eq!(delivered, expected);
    let report = lines
        .iter()
        .find(|line| line.contains("diagnostic logs dropped"))
        .unwrap();
    assert!(report.contains("[ERROR]"));
    assert!(report.contains("dropped_low_priority=1 dropped_warn=1 dropped_error=1"));
    assert_eq!(fixture.threads.lock().unwrap().len(), 1);
}

#[test]
fn flush_deadline_also_bounds_waiting_for_a_stalled_sink_acknowledgement() {
    let mut fixture = StalledWriter::new();
    let started = Instant::now();
    fixture.writer.flush(Duration::from_millis(20));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(fixture.lines.lock().unwrap().is_empty());
    fixture.resume();
    fixture.writer.flush(Duration::from_secs(5));
    assert_eq!(fixture.lines.lock().unwrap().as_slice(), &["first"]);
    assert_eq!(fixture.writer.sender.capacity(), QUEUE_CAPACITY);
}

#[test]
fn concurrent_alert_floods_keep_one_writer_and_a_bounded_queue_with_or_without_tokio() {
    for with_runtime in [false, true] {
        let runtime = with_runtime.then(|| {
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap()
        });
        let mut fixture = StalledWriter::new();
        std::thread::scope(|scope| {
            for producer in 0..8 {
                let writer = &fixture.writer;
                let runtime = &runtime;
                scope.spawn(move || {
                    let _entered = runtime.as_ref().map(|runtime| runtime.enter());
                    let level = if producer % 2 == 0 {
                        LogLevel::Warn
                    } else {
                        LogLevel::Error
                    };
                    for _ in 0..10_000 {
                        writer.enqueue(PathBuf::from("unused"), level, "alert".into());
                    }
                });
            }
        });
        assert_eq!(fixture.writer.sender.capacity(), 0);
        let warn = fixture.writer.dropped.warn.load(Ordering::Relaxed);
        let error = fixture.writer.dropped.error.load(Ordering::Relaxed);
        assert_eq!(warn + error, 80_000 - QUEUE_CAPACITY);
        fixture.resume();
        fixture.writer.flush(Duration::from_secs(5));
        assert_eq!(fixture.threads.lock().unwrap().len(), 1);
        let lines = fixture.lines.lock().unwrap();
        assert_eq!(
            lines.iter().filter(|line| line.as_str() == "alert").count(),
            QUEUE_CAPACITY
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line.contains("diagnostic logs dropped"))
                .count(),
            1
        );
        assert_eq!(fixture.writer.dropped.warn.load(Ordering::Relaxed), 0);
        assert_eq!(fixture.writer.dropped.error.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn closed_writer_counts_drops_and_flush_does_not_wait() {
    let (sender, receiver) = channel(QUEUE_CAPACITY);
    drop(receiver);
    let writer = Writer {
        sender,
        dropped: Arc::default(),
    };
    for level in [LogLevel::Info, LogLevel::Warn, LogLevel::Error] {
        for _ in 0..1000 {
            writer.enqueue(PathBuf::from("unused"), level, "lost".into());
        }
    }
    assert_eq!(writer.dropped.low_priority.load(Ordering::Relaxed), 1000);
    assert_eq!(writer.dropped.warn.load(Ordering::Relaxed), 1000);
    assert_eq!(writer.dropped.error.load(Ordering::Relaxed), 1000);
    let started = Instant::now();
    writer.flush(Duration::from_secs(5));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn drop_reports_are_rate_limited_retried_after_disk_failure_and_forced_at_flush() {
    let dropped = DroppedLogs::default();
    let mut last_report = None;
    dropped.record(LogLevel::Error);
    dropped.report(
        Path::new("unused"),
        &mut |_, _| Err(std::io::Error::other("disk unavailable")),
        &mut last_report,
        false,
    );
    assert_eq!(dropped.error.load(Ordering::Relaxed), 1);
    let mut lines = Vec::new();
    dropped.record(LogLevel::Error);
    dropped.report(
        Path::new("unused"),
        &mut |_, line| {
            lines.push(line.to_owned());
            Ok(())
        },
        &mut last_report,
        false,
    );
    assert!(lines.is_empty());
    assert_eq!(dropped.error.load(Ordering::Relaxed), 2);
    dropped.report(
        Path::new("unused"),
        &mut |_, line| {
            lines.push(line.to_owned());
            Ok(())
        },
        &mut last_report,
        true,
    );
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("dropped_error=2"));
    assert_eq!(dropped.error.load(Ordering::Relaxed), 0);
}

#[test]
fn failed_log_writes_are_counted_and_reported_without_recursive_logging() {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let output = Arc::clone(&reports);
    let writer = Writer::start(PathBuf::from("unused"), move |_, line| {
        if line.contains("diagnostic logs dropped") {
            output.lock().unwrap().push(line.to_owned());
            Ok(())
        } else {
            Err(std::io::Error::other("disk unavailable"))
        }
    })
    .unwrap();
    writer.enqueue(PathBuf::from("unused"), LogLevel::Error, "failed".into());
    writer.flush(Duration::from_secs(5));
    assert_eq!(reports.lock().unwrap().len(), 1);
    assert!(reports.lock().unwrap()[0].contains("dropped_error=1"));
}

#[test]
fn disk_rotation_preserves_the_previous_file_and_propagates_failures() {
    let directory =
        std::env::temp_dir().join(format!("fileterm-log-writer-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("app.log"),
        vec![b'x'; MAX_LOG_BYTES as usize],
    )
    .unwrap();
    append_sync(&directory, "next\n").unwrap();
    assert_eq!(
        fs::read_to_string(directory.join("app.log")).unwrap(),
        "next\n"
    );
    assert_eq!(
        fs::metadata(directory.join("app.log.1")).unwrap().len(),
        MAX_LOG_BYTES
    );
    fs::remove_file(directory.join("app.log.1")).unwrap();
    fs::create_dir(directory.join("app.log.1")).unwrap();
    fs::write(
        directory.join("app.log"),
        vec![b'x'; MAX_LOG_BYTES as usize],
    )
    .unwrap();
    assert!(append_sync(&directory, "lost\n").is_err());
    assert_eq!(
        fs::metadata(directory.join("app.log")).unwrap().len(),
        MAX_LOG_BYTES
    );
    fs::remove_dir_all(directory).unwrap();
}
