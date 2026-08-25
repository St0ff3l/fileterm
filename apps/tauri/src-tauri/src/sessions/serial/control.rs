use std::time::Duration;

use tokio_serial::{ClearBuffer, SerialPort, SerialStream};
use tokio_util::sync::CancellationToken;

use super::super::{SerialControlAction, SerialLineStatus};
use super::platform::read_output_lines;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SerialControlState {
    pub(super) dtr: Option<bool>,
    pub(super) rts: Option<bool>,
}

impl SerialControlState {
    pub(super) fn from_profile(profile: &serde_json::Value) -> Self {
        Self {
            dtr: profile
                .get("dtrOnOpen")
                .and_then(serde_json::Value::as_bool)
                .or(Some(true)),
            rts: profile
                .get("rtsOnOpen")
                .and_then(serde_json::Value::as_bool)
                .or(Some(false)),
        }
    }
}

pub(super) fn apply_initial_lines(
    stream: &mut SerialStream,
    state: SerialControlState,
) -> Result<(), String> {
    if let Some(rts) = state.rts {
        stream
            .write_request_to_send(rts)
            .map_err(|error| format!("无法设置串口 RTS 初始状态：{error}"))?;
    }
    Ok(())
}

pub(super) async fn execute(
    stream: &mut SerialStream,
    action: SerialControlAction,
    value: Option<bool>,
    duration_ms: Option<u64>,
    state: &mut SerialControlState,
    cancellation: &CancellationToken,
) -> Result<SerialLineStatus, String> {
    match action {
        SerialControlAction::SetDtr => {
            let value = value.ok_or_else(|| "串口 DTR 状态不能为空".to_string())?;
            stream
                .write_data_terminal_ready(value)
                .map_err(|error| format!("设置串口 DTR 失败：{error}"))?;
            state.dtr = Some(value);
        }
        SerialControlAction::SetRts => {
            let value = value.ok_or_else(|| "串口 RTS 状态不能为空".to_string())?;
            stream
                .write_request_to_send(value)
                .map_err(|error| format!("设置串口 RTS 失败：{error}"))?;
            state.rts = Some(value);
        }
        SerialControlAction::SendBreak => {
            let duration = Duration::from_millis(duration_ms.unwrap_or(250).clamp(1, 5_000));
            stream
                .set_break()
                .map_err(|error| format!("发送串口 Break 失败：{error}"))?;
            let canceled = tokio::select! {
                _ = cancellation.cancelled() => true,
                _ = tokio::time::sleep(duration) => false,
            };
            if canceled {
                stream
                    .clear_break()
                    .map_err(|error| format!("结束串口 Break 失败：{error}"))?;
                return Err("串口控制已取消".to_string());
            }
            stream
                .clear_break()
                .map_err(|error| format!("结束串口 Break 失败：{error}"))?;
        }
        SerialControlAction::ClearBuffers => {
            stream
                .clear(ClearBuffer::All)
                .map_err(|error| format!("清空串口缓冲区失败：{error}"))?;
        }
        SerialControlAction::Reset => {
            stream
                .clear(ClearBuffer::All)
                .map_err(|error| format!("复位串口缓冲区失败：{error}"))?;
            if let Some(dtr) = state.dtr {
                stream
                    .write_data_terminal_ready(dtr)
                    .map_err(|error| format!("复位串口 DTR 失败：{error}"))?;
            }
            if let Some(rts) = state.rts {
                stream
                    .write_request_to_send(rts)
                    .map_err(|error| format!("复位串口 RTS 失败：{error}"))?;
            }
        }
        SerialControlAction::Status => {}
    }

    Ok(read_status(stream, *state))
}

fn read_status(stream: &mut SerialStream, state: SerialControlState) -> SerialLineStatus {
    let (actual_dtr, actual_rts) = read_output_lines(stream);
    SerialLineStatus {
        dtr: actual_dtr.or(state.dtr),
        rts: actual_rts.or(state.rts),
        cts: stream.read_clear_to_send().ok(),
        dsr: stream.read_data_set_ready().ok(),
        ring: stream.read_ring_indicator().ok(),
        carrier_detect: stream.read_carrier_detect().ok(),
    }
}
