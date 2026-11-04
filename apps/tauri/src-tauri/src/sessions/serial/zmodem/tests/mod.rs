use std::path::PathBuf;

use tokio::io::duplex;
use tokio_util::sync::CancellationToken;

use super::super::super::{SerialTransferDirection, SerialTransferMode, SerialTransferRequest};
use super::super::limits::{SerialTransferLimits, TransferBudget};
use super::super::progress::SerialTransferReporter;
use super::super::timing::SerialTransferTiming;
use super::{receive, send};

#[tokio::test]
async fn round_trip_works_without_a_physical_device() {
    let source = temporary_path("zmodem-source");
    let target_directory = temporary_path("zmodem-target");
    tokio::fs::create_dir_all(&target_directory).await.unwrap();
    let bytes = (0..4097_u32)
        .map(|value| (value.wrapping_mul(17) & 0xff) as u8)
        .collect::<Vec<_>>();
    tokio::fs::write(&source, &bytes).await.unwrap();
    let timing =
        SerialTransferTiming::from_profile(&serde_json::json!({}), 115_200, 8, 1, false).unwrap();
    let cancellation = CancellationToken::new();
    let receiver_cancellation = cancellation.clone();
    let sender_source = source.clone();
    let receiver_directory = target_directory.clone();
    let (mut sender_stream, mut receiver_stream) = duplex(64 * 1024);
    let sender = tokio::spawn(async move {
        let mut budget = TransferBudget::new(SerialTransferLimits::default());
        let request = SerialTransferRequest {
            direction: SerialTransferDirection::Send,
            mode: SerialTransferMode::Zmodem,
            local_path: sender_source.to_string_lossy().into_owned(),
            local_paths: Vec::new(),
            xmodem_preserve_padding: true,
        };
        let mut reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Send,
            SerialTransferMode::Zmodem,
            &request.local_path,
        );
        send(
            &mut sender_stream,
            &request,
            timing,
            &mut budget,
            &mut reporter,
            &cancellation,
        )
        .await
    });
    let receiver = tokio::spawn(async move {
        let mut budget = TransferBudget::new(SerialTransferLimits::default());
        let mut reporter = SerialTransferReporter::disabled(
            SerialTransferDirection::Receive,
            SerialTransferMode::Zmodem,
            receiver_directory.to_string_lossy().as_ref(),
        );
        receive(
            &mut receiver_stream,
            &receiver_directory,
            timing,
            &mut budget,
            &mut reporter,
            &receiver_cancellation,
        )
        .await
    });
    let (sender_result, receiver_result) = tokio::join!(sender, receiver);
    let sender_result = sender_result.unwrap();
    let receiver_result = receiver_result.unwrap();
    assert_eq!(sender_result, Ok(bytes.len() as u64));
    assert_eq!(receiver_result, Ok(bytes.len() as u64));
    assert_eq!(
        tokio::fs::read(target_directory.join(source.file_name().unwrap()))
            .await
            .unwrap(),
        bytes
    );
    let _ = tokio::fs::remove_file(source).await;
    let _ = tokio::fs::remove_dir_all(target_directory).await;
}

fn temporary_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("fileterm-serial-{label}-{}", uuid::Uuid::new_v4()))
}
