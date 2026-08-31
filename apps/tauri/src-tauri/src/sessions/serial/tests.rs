#[cfg(test)]
mod tests {
    use super::{serial_port_matches_identity, SerialWorkerError};

    #[test]
    fn exposes_retryable_worker_errors_for_reconnect() {
        assert!(!SerialWorkerError::fatal("invalid configuration").retryable);
        assert!(SerialWorkerError::retryable("device unavailable").retryable);
    }

    #[test]
    fn matches_non_usb_identity_by_port_type_without_guessing_a_device() {
        let port = tokio_serial::SerialPortInfo {
            port_name: "/dev/ttyS0".to_string(),
            port_type: tokio_serial::SerialPortType::PciPort,
        };
        assert!(serial_port_matches_identity(
            &port,
            None,
            None,
            None,
            Some("pci")
        ));
        assert!(!serial_port_matches_identity(
            &port,
            None,
            None,
            None,
            Some("usb")
        ));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn virtual_pty_round_trip_exercises_the_real_serial_stack() {
        use std::process::Stdio;

        use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;
        use tokio_serial::SerialPortBuilderExt;

        // `openpty` is provided by Python's standard library. Linux's serial
        // backend accepts a PTY as a real serial endpoint, so CI can exercise
        // the complete async read/write lifecycle without a USB device.
        // macOS's backend rejects PTYs with ENOTTY; its acceptance requires an
        // actual /dev/cu.* device (tracked in the release checklist instead of
        // silently pretending a PTY is representative).
        let script = r#"import os, pty, sys
master, slave = pty.openpty()
print(os.ttyname(slave), flush=True)
while True:
    data = os.read(master, 4096)
    if not data:
        break
    os.write(master, b'echo:' + data)
"#;
        let mut child = Command::new("python3")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("python3 must be available for the virtual serial fixture");
        let stdout = child.stdout.take().expect("fixture stdout must be piped");
        let mut lines = BufReader::new(stdout).lines();
        let device_path =
            tokio::time::timeout(std::time::Duration::from_secs(3), lines.next_line())
                .await
                .expect("virtual serial fixture timed out")
                .expect("virtual serial fixture output failed")
                .expect("virtual serial fixture did not provide a device path");

        let stream = tokio_serial::new(&device_path, 115_200)
            .open_native_async()
            .expect("virtual serial device must open");
        let (mut reader, mut writer) = tokio::io::split(stream);
        writer.write_all(b"ping\n").await.unwrap();
        writer.flush().await.unwrap();

        let mut received = Vec::new();
        let mut buffer = [0_u8; 64];
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !received
                .windows(b"echo:ping".len())
                .any(|window| window == b"echo:ping")
            {
                let count = reader.read(&mut buffer).await.unwrap();
                assert!(count > 0, "virtual serial peer closed before echoing data");
                received.extend_from_slice(&buffer[..count]);
            }
        })
        .await
        .expect("virtual serial round trip timed out");

        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}
