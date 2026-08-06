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
    let mut command = CommandBuilder::new("cmd.exe");
    // Keep the command tokens separate so CommandBuilder does not wrap the
    // complete `/C` payload in another pair of quotes. `/D` also prevents a
    // runner-level cmd autorun hook from changing the test's lifecycle.
    command.args(["/D", "/C", "echo", "FileTerm", "local"]);
    let mut child = slave
        .spawn_command(command)
        .expect("cmd.exe should start in ConPTY");
    eprintln!("conpty stage: child spawned");
    drop(slave);

    eprintln!("conpty stage: clone reader");
    let mut reader = master
        .try_clone_reader()
        .expect("ConPTY reader should clone");
    let (output_tx, output_rx) = std::sync::mpsc::channel();
    eprintln!("conpty stage: start reader");
    let reader_thread = std::thread::spawn(move || {
        let mut output = [0_u8; 4096];
        let result = reader.read(&mut output).map(|size| output[..size].to_vec());
        let _ = output_tx.send(result);
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
        if let Some(status) = child.try_wait().expect("cmd.exe status should be readable") {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            drop(master);
            eprintln!("cmd.exe did not exit within the ConPTY test timeout");
            std::process::exit(1);
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let output = match output_rx.recv_timeout(std::time::Duration::from_secs(2)) {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => panic!("ConPTY reader failed: {error}"),
        Err(_) => {
            let _ = child.kill();
            eprintln!("ConPTY reader did not produce output within the test timeout");
            std::process::exit(1);
        }
    };
    reader_thread
        .join()
        .expect("ConPTY reader thread should finish after one output frame");
    drop(master);

    assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
    assert_eq!(status.exit_code(), 0);
}
