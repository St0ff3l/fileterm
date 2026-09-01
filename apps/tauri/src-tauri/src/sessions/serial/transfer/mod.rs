//! Serial file-transfer protocols.
//!
//! The worker owns the port while a transfer is active. Keeping the protocol
//! state machine here makes its checksum/frame rules testable without a
//! physical adapter; the renderer serializes ordinary controls and quick sends
//! behind the active transfer so protocol bytes cannot be interleaved.

use std::collections::HashSet;
use std::path::Path;
use std::pin::Pin;

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::super::{
    SerialTransferDirection, SerialTransferMode, SerialTransferRequest, SerialTransferResult,
};
pub(super) use super::file_safety::is_safe_transfer_file_name;
use super::file_safety::StagedReceiveFile;
#[path = "frame.rs"]
mod frame;
use super::limits::{SerialTransferLimits, TransferBudget};
use super::progress::SerialTransferReporter;
use super::timing::SerialTransferTiming;
use crate::services::session_logs::{SerialLogDirection, SerialLogSink};

use self::frame::{
    cancel_protocol, parse_ymodem_header, read_byte, read_file, read_next_protocol_byte,
    read_packet_tail, receive_protocol_start, send_eot, send_packet, wait_for_sender_start,
    ymodem_target, ACK, CAN, CRC_REQUEST, EOT, NAK, PAD, SOH, STX,
};
pub(super) use self::frame::{create_target, flush, write_all};

#[derive(Clone, Copy, Debug)]
struct BlockTransferOptions {
    block_size: usize,
    use_crc: bool,
    offset: u64,
}

#[derive(Clone, Copy, Debug)]
struct YmodemFileOptions {
    size: u64,
    use_crc: bool,
    offset: u64,
}

pub(super) struct TransferContext<'a> {
    pub(super) timing: SerialTransferTiming,
    pub(super) limits: SerialTransferLimits,
    pub(super) log_sink: Option<SerialLogSink>,
    pub(super) encoding: &'a str,
    pub(super) reporter: &'a mut SerialTransferReporter,
    pub(super) cancellation: CancellationToken,
}

pub(super) async fn execute<S>(
    stream: &mut S,
    request: SerialTransferRequest,
    context: TransferContext<'_>,
) -> Result<SerialTransferResult, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let TransferContext {
        timing,
        limits,
        log_sink,
        encoding,
        reporter,
        cancellation,
    } = context;
    let mut stream = TransferLogStream {
        inner: stream,
        log_sink,
        encoding,
    };
    let path = request.local_path.clone();
    let paths = if request.local_paths.is_empty() {
        vec![path.clone()]
    } else {
        request.local_paths.clone()
    };
    let mode = request.mode;
    let xmodem_preserve_padding = request.xmodem_preserve_padding;
    let mut budget = TransferBudget::new(limits);
    let result: Result<u64, String> = match (request.direction, mode) {
        (SerialTransferDirection::Send, SerialTransferMode::Raw) => {
            send_raw(
                &mut stream,
                Path::new(&path),
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Raw) => {
            receive_raw(
                &mut stream,
                Path::new(&path),
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Xmodem) => {
            send_xmodem(
                &mut stream,
                Path::new(&path),
                false,
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Xmodem) => {
            receive_xmodem(
                &mut stream,
                Path::new(&path),
                timing,
                xmodem_preserve_padding,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Ymodem) => {
            send_ymodem(
                &mut stream,
                &paths,
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Ymodem) => {
            receive_ymodem(
                &mut stream,
                Path::new(&path),
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Zmodem) => {
            super::zmodem::send(
                &mut stream,
                &request,
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Zmodem) => {
            super::zmodem::receive(
                &mut stream,
                Path::new(&path),
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Send, SerialTransferMode::Kermit) => {
            super::kermit::send(
                &mut stream,
                &request,
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
        (SerialTransferDirection::Receive, SerialTransferMode::Kermit) => {
            super::kermit::receive(
                &mut stream,
                Path::new(&path),
                timing,
                &mut budget,
                reporter,
                &cancellation,
            )
            .await
        }
    };
    match &result {
        Ok(bytes) => reporter.finish("completed", *bytes, None, None),
        Err(error) => reporter.finish(
            if cancellation.is_cancelled() {
                "canceled"
            } else {
                "failed"
            },
            0,
            None,
            Some(error.clone()),
        ),
    }
    if result.is_err() && mode != SerialTransferMode::Raw {
        cancel_protocol(&mut stream).await;
    }
    let bytes_transferred = result?;
    Ok(SerialTransferResult {
        bytes_transferred,
        local_path: path,
    })
}

/// Records logical transfer bytes without putting async log IO on the serial
/// worker. `SerialIo` separately records physical wire bytes for raw logs.
struct TransferLogStream<'a, S> {
    inner: &'a mut S,
    log_sink: Option<SerialLogSink>,
    encoding: &'a str,
}

impl<S> Unpin for TransferLogStream<'_, S> {}

impl<S> AsyncRead for TransferLogStream<'_, S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buffer.filled().len();
        let result = Pin::new(&mut *this.inner).poll_read(cx, buffer);
        if let std::task::Poll::Ready(Ok(())) = &result {
            if let Some(sink) = &this.log_sink {
                sink.append(
                    SerialLogDirection::Rx,
                    &buffer.filled()[before..],
                    None,
                    this.encoding,
                );
            }
        }
        result
    }
}

impl<S> AsyncWrite for TransferLogStream<'_, S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut *this.inner).poll_write(cx, buffer) {
            std::task::Poll::Ready(Ok(written)) => {
                if let Some(sink) = &this.log_sink {
                    sink.append(
                        SerialLogDirection::Tx,
                        &buffer[..written],
                        None,
                        this.encoding,
                    );
                }
                std::task::Poll::Ready(Ok(written))
            }
            result => result,
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut *self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut *self.get_mut().inner).poll_shutdown(cx)
    }
}
include!("raw.rs");
include!("xmodem.rs");
include!("ymodem.rs");
include!("tests.rs");
