#[cfg(windows)]
use std::io::Read;

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
    let mut command = CommandBuilder::new("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "[Console]::WriteLine('FileTerm local'); exit 7",
    ]);
    let mut child = slave
        .spawn_command(command)
        .expect("powershell.exe should start in ConPTY");
    eprintln!("conpty stage: child spawned");
    drop(slave);

    eprintln!("conpty stage: clone reader");
    let mut reader = master
        .try_clone_reader()
        .expect("ConPTY reader should clone");
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    let (reader_done_tx, reader_done_rx) = std::sync::mpsc::channel();
    eprintln!("conpty stage: start reader");
    let reader_thread = std::thread::spawn(move || {
        let mut first_output_tx = Some(output_tx);
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(size) => {
                    if let Some(output_tx) = first_output_tx.take() {
                        let _ = output_tx.send(Ok(buffer[..size].to_vec()));
                    }
                }
                Err(error) => {
                    if let Some(output_tx) = first_output_tx.take() {
                        let _ = output_tx.send(Err(error));
                    }
                    break;
                }
            }
        }
        let _ = reader_done_tx.send(());
    });

    let writer = master
        .take_writer()
        .expect("ConPTY writer should be available");
    eprintln!("conpty stage: drop writer");
    drop(writer);
    // Poll instead of calling wait() directly so a runner-level ConPTY
    // problem fails with a useful assertion rather than hanging the job.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    eprintln!("conpty stage: wait for child");
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .expect("PowerShell status should be readable")
        {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            drop(master);
            eprintln!("PowerShell did not exit within the ConPTY test timeout");
            std::process::exit(1);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let output = match output_rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => panic!("ConPTY reader failed: {error}"),
        Err(_) => {
            let _ = child.kill();
            drop(master);
            eprintln!("ConPTY reader did not produce output within the test timeout");
            std::process::exit(1);
        }
    };
    drop(master);
    if reader_done_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .is_err()
    {
        eprintln!("ConPTY reader did not finish after master close");
        std::process::exit(1);
    }
    reader_thread
        .join()
        .expect("ConPTY reader thread should finish after master close");

    assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
    assert_eq!(status.exit_code(), 7);
}
