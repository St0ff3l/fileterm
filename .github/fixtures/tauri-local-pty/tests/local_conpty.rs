#[cfg(windows)]
use std::io::{Read, Write};

#[cfg(windows)]
use std::sync::{mpsc, Arc, Mutex};

#[cfg(windows)]
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

#[cfg(windows)]
#[test]
fn conpty_preserves_output_and_exit_status() {
    eprintln!("conpty stage: create pty system");
    let pty_system = native_pty_system();
    eprintln!("conpty stage: open pty");
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("local ConPTY should open in the test environment");
    eprintln!("conpty stage: build command");
    let portable_pty::PtyPair { master, slave } = pair;
    let mut command = CommandBuilder::new("cmd.exe");
    command.args(["/C", "echo FileTerm local && exit /B 7"]);
    let mut child = slave
        .spawn_command(command)
        .expect("cmd.exe should start in ConPTY");
    eprintln!("conpty stage: child spawned");
    drop(slave);

    eprintln!("conpty stage: clone reader");
    let mut reader = master
        .try_clone_reader()
        .expect("ConPTY reader should clone");
    let writer = master
        .take_writer()
        .expect("ConPTY writer should be available");
    let writer = Arc::new(Mutex::new(Some(writer)));
    let writer_for_reader = writer.clone();
    let (reader_done_tx, reader_done_rx) = mpsc::channel();
    eprintln!("conpty stage: start reader");
    let reader_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        let mut pending_queries = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    output.extend_from_slice(&buffer[..size]);
                    if let Err(error) = respond_to_cursor_queries(
                        &mut pending_queries,
                        &buffer[..size],
                        &writer_for_reader,
                    ) {
                        eprintln!("ConPTY cursor response failed: {error}");
                        break;
                    }
                }
                Err(error) => {
                    eprintln!("ConPTY reader ended: {error}");
                    break;
                }
            }
        }
        let _ = reader_done_tx.send(output);
    });

    // Keep the input side alive: ConPTY may ask the terminal host for its
    // cursor position before the shell can finish starting.
    eprintln!("conpty stage: keep writer for cursor responses");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    eprintln!("conpty stage: wait for child");
    let status = loop {
        if let Some(status) = child.try_wait().expect("cmd.exe status should be readable") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = writer
                .lock()
                .expect("ConPTY writer mutex should not be poisoned")
                .take();
            // ClosePseudoConsole can block indefinitely on Windows Server
            // 2022 when a pseudo console is still draining. Leak the handle
            // on this failure path so the test reports promptly instead of
            // consuming the entire Actions job timeout.
            std::mem::forget(master);
            panic!("cmd.exe did not exit within the ConPTY test timeout");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };

    let _ = writer
        .lock()
        .expect("ConPTY writer mutex should not be poisoned")
        .take();

    // ClosePseudoConsole must not run on the reader thread, and on older
    // Windows it may wait for the output pipe to drain. Isolate that wait so
    // a runner-level cleanup problem has a bounded failure mode too.
    let (close_tx, close_rx) = mpsc::channel();
    std::thread::spawn(move || {
        drop(master);
        let _ = close_tx.send(());
    });
    if close_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .is_err()
    {
        panic!("ConPTY master did not close after the child exited");
    }

    let output = reader_done_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("ConPTY reader should finish after master close");
    reader_thread
        .join()
        .expect("ConPTY reader thread should finish after master close");

    assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
    assert_eq!(status.exit_code(), 7);
}

#[cfg(windows)]
fn respond_to_cursor_queries(
    pending_queries: &mut Vec<u8>,
    chunk: &[u8],
    writer: &Arc<Mutex<Option<Box<dyn Write + Send>>>>,
) -> std::io::Result<()> {
    const CURSOR_QUERY: &[u8] = b"\x1b[6n";
    const PRIVATE_CURSOR_QUERY: &[u8] = b"\x1b[?6n";
    const CURSOR_RESPONSE: &[u8] = b"\x1b[1;1R";
    const PRIVATE_CURSOR_RESPONSE: &[u8] = b"\x1b[?1;1R";

    pending_queries.extend_from_slice(chunk);
    loop {
        let normal = pending_queries
            .windows(CURSOR_QUERY.len())
            .position(|window| window == CURSOR_QUERY)
            .map(|position| (position, CURSOR_QUERY.len(), CURSOR_RESPONSE));
        let private = pending_queries
            .windows(PRIVATE_CURSOR_QUERY.len())
            .position(|window| window == PRIVATE_CURSOR_QUERY)
            .map(|position| {
                (
                    position,
                    PRIVATE_CURSOR_QUERY.len(),
                    PRIVATE_CURSOR_RESPONSE,
                )
            });
        let Some((position, query_len, response)) = (match (normal, private) {
            (Some(normal), Some(private)) => Some(if normal.0 <= private.0 {
                normal
            } else {
                private
            }),
            (Some(normal), None) => Some(normal),
            (None, Some(private)) => Some(private),
            (None, None) => None,
        }) else {
            break;
        };

        let mut writer = writer
            .lock()
            .map_err(|_| std::io::Error::other("ConPTY writer mutex was poisoned"))?;
        let writer = writer.as_mut().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "ConPTY writer closed")
        })?;
        writer.write_all(response)?;
        writer.flush()?;
        pending_queries.drain(..position + query_len);
    }

    // Keep enough bytes to detect a query split across two reads without
    // allowing arbitrary shell output to accumulate in the scanner.
    let keep = PRIVATE_CURSOR_QUERY.len() - 1;
    if pending_queries.len() > keep {
        let remove = pending_queries.len() - keep;
        pending_queries.drain(..remove);
    }
    Ok(())
}
