use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_serial::SerialPortBuilderExt;
use tokio_util::sync::CancellationToken;

use super::telnet::reject_unsupported;
use super::terminal::{emit_terminal_data, set_terminal_state};
use super::WorkerCmd;
use crate::services::WorkspaceTabStatus;

const SERIAL_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

mod codec;
mod config;
mod control;
mod file_safety;
mod kermit;
mod limits;
mod pacing;
mod platform;
mod progress;
mod timing;
mod transfer;
mod zmodem;

use self::codec::{
    baud_rate as serial_baud_rate, consume_hex_input as consume_serial_hex_input,
    consume_line_input as consume_serial_line_input, display as serial_display,
    encode_input as encode_serial_input, stream_display as serial_stream_display,
    validate_modes as validate_serial_modes, SerialInputChunk, TextDecoder as SerialTextDecoder,
};
use self::config::{data_bits, flow_control, parity, serial_error, stop_bits};
use self::control::{
    apply_close_lines, apply_initial_lines, execute as execute_serial_control, SerialControlState,
};
use self::limits::SerialTransferLimits;
use self::pacing::{write_serial_bytes, SerialPacing};
use self::platform::{
    apply_parity as apply_platform_parity, apply_rs485,
    parity_wire_mode as serial_parity_wire_mode, wire_data_bits, SerialIo, SerialParityWireMode,
};
use self::progress::SerialTransferReporter;
use self::timing::SerialTransferTiming;
use super::reconnect::ReconnectPolicy;

include!("device.rs");
include!("worker.rs");
include!("reconnect.rs");
include!("tests.rs");
