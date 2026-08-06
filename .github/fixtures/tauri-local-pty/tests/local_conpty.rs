#[cfg(windows)]
use std::io::Read;

#[cfg(windows)]
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

#[cfg(windows)]
#[test]
fn conpty_preserves_output_and_exit_status() {
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
    let reader_thread = std::thread::spawn(move || {
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
    // Closing the master is required for ConPTY's cloned reader to observe
    // EOF after the child exits. Keep this explicit so the fixture cannot
    // leave the workflow waiting on a live pseudo-console handle.
    drop(master);
    let output = output_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("ConPTY reader should finish after shell exit");
    reader_thread
        .join()
        .expect("ConPTY reader thread should finish after shell exit");

    assert!(String::from_utf8_lossy(&output).contains("FileTerm local"));
    assert_eq!(status.exit_code(), 7);
}
