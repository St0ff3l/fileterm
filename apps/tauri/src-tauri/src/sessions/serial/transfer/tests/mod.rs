#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tokio::io::{duplex, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    use super::super::super::{SerialTransferDirection, SerialTransferMode};
    use super::super::limits::{SerialTransferLimits, TransferBudget};
    use super::super::progress::SerialTransferReporter;
    use super::super::timing::SerialTransferTiming;
    use super::frame::{checksum, crc16, parse_ymodem_size, read_packet_tail_with_timeout};
    use super::{
        create_target, finalize_xmodem_payload, is_safe_transfer_file_name, parse_ymodem_header,
        read_byte, read_packet_tail, receive_raw, receive_xmodem, receive_ymodem,
        receive_ymodem_file, send_xmodem, send_ymodem, YmodemFileOptions, SOH,
    };

    #[test]
    fn calculates_xmodem_checksums() {
        assert_eq!(checksum(b"123456789"), 0xdd);
        assert_eq!(crc16(b"123456789"), 0x31c3);
    }

    #[test]
    fn parses_ymodem_header_size() {
        let mut header = [0_u8; 128];
        let bytes = [
            b'f', b'i', b'l', b'e', b'.', b'b', b'i', b'n', 0, b'1', b'2', b'3', 0,
        ];
        header[..bytes.len()].copy_from_slice(&bytes);
        assert_eq!(parse_ymodem_size(&header).unwrap(), 123);
        assert_eq!(
            parse_ymodem_header(&header).unwrap(),
            Some(("file.bin".to_string(), 123))
        );
        assert_eq!(parse_ymodem_header(&[0_u8; 128]).unwrap(), None);
        assert!(parse_ymodem_size(b"file.bin").is_err());
    }

    #[test]
    fn rejects_cross_platform_unsafe_transfer_names() {
        for name in [
            "../escape",
            "folder\\escape",
            "CON",
            "COM1",
            "report:",
            "trailing.",
        ] {
            assert!(
                !is_safe_transfer_file_name(name),
                "name should be rejected: {name}"
            );
        }
        assert!(is_safe_transfer_file_name("report.bin"));
    }

    #[test]
    fn xmodem_padding_is_only_trimmed_when_explicitly_requested() {
        let payload = vec![0x41, 0x1a, 0x1a];
        assert_eq!(finalize_xmodem_payload(payload.clone(), true), payload);
        assert_eq!(finalize_xmodem_payload(payload, false), vec![0x41]);
    }

    #[tokio::test]
    async fn xmodem_round_trip_works_without_a_physical_device() {
        let source = temporary_path("source");
        let target = temporary_path("target");
        let bytes = (0..251_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let cancellation = CancellationToken::new();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Xmodem,
            source.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Xmodem,
            target.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let receiver_target = target.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            send_xmodem(
                &mut sender_stream,
                &sender_source,
                false,
                timing,
                &mut budget,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            receive_xmodem(
                &mut receiver_stream,
                &receiver_target,
                timing,
                false,
                &mut budget,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(sender.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(receiver.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(tokio::fs::read(&target).await.unwrap(), bytes);
        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn ymodem_round_trip_works_without_a_physical_device() {
        let source = temporary_path("ymodem-source");
        let target_directory = temporary_path("ymodem-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let target = target_directory.join(source.file_name().unwrap());
        let bytes = (0..1537_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source, &bytes).await.unwrap();
        let cancellation = CancellationToken::new();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Ymodem,
            source.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target_directory.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_source = source.clone();
        let sender_paths = vec![sender_source.to_string_lossy().into_owned()];
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            send_ymodem(
                &mut sender_stream,
                &sender_paths,
                timing,
                &mut budget,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            receive_ymodem(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut budget,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(sender.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(receiver.await.unwrap().unwrap(), bytes.len() as u64);
        assert_eq!(tokio::fs::read(&target).await.unwrap(), bytes);
        let _ = tokio::fs::remove_file(source).await;
        let _ = tokio::fs::remove_dir_all(target_directory).await;
    }

    #[tokio::test]
    async fn ymodem_round_trip_supports_a_batch_without_a_physical_device() {
        let source_a = temporary_path("ymodem-batch-a");
        let source_b = temporary_path("ymodem-batch-b");
        let target_directory = temporary_path("ymodem-batch-target");
        tokio::fs::create_dir_all(&target_directory).await.unwrap();
        let bytes_a = (0..257_u16)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let bytes_b = (0..2049_u16)
            .map(|value| (value % 239) as u8)
            .collect::<Vec<_>>();
        tokio::fs::write(&source_a, &bytes_a).await.unwrap();
        tokio::fs::write(&source_b, &bytes_b).await.unwrap();
        let cancellation = CancellationToken::new();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut sender_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Ymodem,
            source_a.to_str().unwrap(),
        );
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target_directory.to_str().unwrap(),
        );
        let receiver_cancellation = cancellation.clone();
        let sender_paths = vec![
            source_a.to_string_lossy().into_owned(),
            source_b.to_string_lossy().into_owned(),
        ];
        let receiver_directory = target_directory.clone();
        let (mut sender_stream, mut receiver_stream) = duplex(4096);
        let sender = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            send_ymodem(
                &mut sender_stream,
                &sender_paths,
                timing,
                &mut budget,
                &mut sender_reporter,
                &cancellation,
            )
            .await
        });
        let receiver = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            receive_ymodem(
                &mut receiver_stream,
                &receiver_directory,
                timing,
                &mut budget,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        assert_eq!(
            sender.await.unwrap().unwrap(),
            (bytes_a.len() + bytes_b.len()) as u64
        );
        assert_eq!(
            receiver.await.unwrap().unwrap(),
            (bytes_a.len() + bytes_b.len()) as u64
        );
        assert_eq!(
            tokio::fs::read(target_directory.join(source_a.file_name().unwrap()))
                .await
                .unwrap(),
            bytes_a
        );
        assert_eq!(
            tokio::fs::read(target_directory.join(source_b.file_name().unwrap()))
                .await
                .unwrap(),
            bytes_b
        );
        let _ = tokio::fs::remove_file(source_a).await;
        let _ = tokio::fs::remove_file(source_b).await;
        let _ = tokio::fs::remove_dir_all(target_directory).await;
    }

    #[tokio::test]
    async fn receive_target_never_overwrites_existing_file() {
        let target = temporary_path("existing-target");
        let original = b"keep this file";
        tokio::fs::write(&target, original).await.unwrap();
        let error = create_target(&target, SerialTransferLimits::default().max_file_bytes)
            .await
            .unwrap_err();
        assert!(error.contains("已存在"));
        assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn ymodem_existing_target_is_not_removed_after_create_failure() {
        let target = temporary_path("existing-ymodem-target");
        let original = b"keep this YMODEM file";
        tokio::fs::write(&target, original).await.unwrap();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let mut reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Ymodem,
            target.to_str().unwrap(),
        );
        let (_writer, mut reader) = duplex(64);
        let mut budget = TransferBudget::new(SerialTransferLimits::default());
        let error = receive_ymodem_file(
            &mut reader,
            &target,
            YmodemFileOptions {
                size: original.len() as u64,
                use_crc: true,
                offset: 0,
            },
            timing,
            &mut budget,
            &mut reporter,
            &cancellation,
        )
        .await
        .unwrap_err();
        assert!(error.contains("已存在"));
        assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
        let _ = tokio::fs::remove_file(target).await;
    }

    #[tokio::test]
    async fn canceled_raw_receive_removes_partial_target() {
        let target = temporary_path("canceled-raw-target");
        let cancellation = CancellationToken::new();
        let receiver_cancellation = cancellation.clone();
        let receiver_target = target.clone();
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let mut receiver_reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Raw,
            target.to_str().unwrap(),
        );
        let (mut sender_stream, mut receiver_stream) = duplex(64);
        let receiver = tokio::spawn(async move {
            let mut budget = TransferBudget::new(SerialTransferLimits::default());
            receive_raw(
                &mut receiver_stream,
                &receiver_target,
                timing,
                &mut budget,
                &mut receiver_reporter,
                &receiver_cancellation,
            )
            .await
        });
        sender_stream.write_all(b"partial").await.unwrap();
        // The receive path intentionally keeps the final target absent while
        // bytes are staged. Give the duplex worker a chance to consume the
        // bytes, then cancel and verify that no partial final file is
        // published.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        cancellation.cancel();
        assert!(receiver.await.unwrap().is_err());
        assert!(!target.exists());
    }

    #[tokio::test]
    async fn incomplete_packet_is_not_retried_with_leftover_bytes() {
        let timing =
            SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false)
                .unwrap();
        let cancellation = CancellationToken::new();
        let (mut writer, mut reader) = duplex(256);
        writer.write_all(&[1, 254, 0x41]).await.unwrap();
        writer.shutdown().await.unwrap();
        let error = read_packet_tail(&mut reader, 128, true, timing, &cancellation)
            .await
            .unwrap_err();
        assert!(
            error.contains("数据帧不完整") || error.contains("读取串口文件传输数据失败"),
            "unexpected incomplete-frame error: {error}"
        );
    }

    #[tokio::test]
    async fn partial_packet_is_drained_before_a_fresh_retry_marker() {
        let cancellation = CancellationToken::new();
        let (mut writer, mut reader) = duplex(512);
        let payload = vec![0x42_u8; 128];
        let mut packet = vec![1_u8, 254_u8];
        packet.extend_from_slice(&payload);
        packet.extend_from_slice(&crc16(&payload).to_be_bytes());
        let retry_packet = packet.clone();
        let writer_task = tokio::spawn(async move {
            // Only a prefix of the first frame reaches the receiver. The
            // sender later retransmits a complete frame after the receiver's
            // bounded drain window has elapsed.
            writer.write_all(&packet[..9]).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            writer.write_all(&[SOH]).await.unwrap();
            writer.write_all(&retry_packet).await.unwrap();
        });

        let first = read_packet_tail_with_timeout(
            &mut reader,
            128,
            true,
            std::time::Duration::from_millis(5),
            &cancellation,
        )
        .await
        .unwrap();
        assert!(first.is_none());

        let marker = read_byte(
            &mut reader,
            std::time::Duration::from_millis(100),
            &cancellation,
        )
        .await
        .unwrap();
        assert_eq!(marker, Some(SOH));
        let retry = read_packet_tail_with_timeout(
            &mut reader,
            128,
            true,
            std::time::Duration::from_millis(100),
            &cancellation,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(retry, (1, payload));
        writer_task.await.unwrap();
    }

    fn temporary_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fileterm-serial-{label}-{}", uuid::Uuid::new_v4()))
    }
}
