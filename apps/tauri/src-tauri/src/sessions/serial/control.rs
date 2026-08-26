use std::time::Duration;

use tokio_serial::{ClearBuffer, SerialPort, SerialStream};
use tokio_util::sync::CancellationToken;

use super::super::{SerialControlAction, SerialLineStatus};
use super::platform::read_output_lines;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SerialControlState {
    pub(super) dtr: Option<bool>,
    pub(super) rts: Option<bool>,
    initial_dtr: Option<bool>,
    initial_rts: Option<bool>,
    pub(super) hardware_flow_control: bool,
    pub(super) rts_managed: bool,
    pub(super) dtr_on_close: Option<bool>,
    pub(super) rts_on_close: Option<bool>,
}

impl SerialControlState {
    pub(super) fn from_profile(profile: &serde_json::Value) -> Self {
        let initial_dtr = profile
            .get("dtrOnOpen")
            .and_then(serde_json::Value::as_bool)
            .or(Some(true));
        let initial_rts = profile
            .get("rtsOnOpen")
            .and_then(serde_json::Value::as_bool)
            .or(Some(false));
        let hardware_flow_control = profile
            .get("flowControl")
            .and_then(serde_json::Value::as_str)
            == Some("hardware");
        let rs485_enabled =
            profile.get("rs485Mode").and_then(serde_json::Value::as_str) == Some("half-duplex");
        Self {
            dtr: initial_dtr,
            rts: initial_rts,
            initial_dtr,
            initial_rts,
            hardware_flow_control,
            rts_managed: hardware_flow_control || rs485_enabled,
            dtr_on_close: profile
                .get("dtrOnClose")
                .and_then(serde_json::Value::as_bool),
            rts_on_close: profile
                .get("rtsOnClose")
                .and_then(serde_json::Value::as_bool),
        }
    }
}

pub(super) fn apply_initial_lines(
    stream: &mut SerialStream,
    state: SerialControlState,
) -> Result<(), String> {
    if let Some(rts) = state.rts {
        if !state.rts_managed {
            stream
                .write_request_to_send(rts)
                .map_err(|error| format!("无法设置串口 RTS 初始状态：{error}"))?;
        }
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
            if state.rts_managed {
                return Err(rts_managed_error(state));
            }
            let value = value.ok_or_else(|| "串口 RTS 状态不能为空".to_string())?;
            stream
                .write_request_to_send(value)
                .map_err(|error| format!("设置串口 RTS 失败：{error}"))?;
            state.rts = Some(value);
        }
        SerialControlAction::PulseDtr => {
            pulse_dtr(stream, duration_ms, state, cancellation).await?;
        }
        SerialControlAction::PulseRts => {
            if state.rts_managed {
                return Err(rts_managed_error(state));
            }
            pulse_rts(stream, duration_ms, state, cancellation).await?;
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
            state.dtr = state.initial_dtr;
            if let Some(dtr) = state.initial_dtr {
                stream
                    .write_data_terminal_ready(dtr)
                    .map_err(|error| format!("复位串口 DTR 失败：{error}"))?;
            }
            state.rts = state.initial_rts;
            if let Some(rts) = state.initial_rts {
                if !state.rts_managed {
                    stream
                        .write_request_to_send(rts)
                        .map_err(|error| format!("复位串口 RTS 失败：{error}"))?;
                }
            }
        }
        SerialControlAction::Status => {}
    }

    Ok(read_status(stream, *state))
}

async fn pulse_dtr(
    stream: &mut SerialStream,
    duration_ms: Option<u64>,
    state: &SerialControlState,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let original = state.dtr.unwrap_or(false);
    stream
        .write_data_terminal_ready(!original)
        .map_err(|error| format!("设置串口 DTR 脉冲失败：{error}"))?;
    let canceled = wait_for_pulse(duration_ms, cancellation).await;
    stream
        .write_data_terminal_ready(original)
        .map_err(|error| format!("恢复串口 DTR 状态失败：{error}"))?;
    if canceled {
        return Err("串口控制已取消".to_string());
    }
    Ok(())
}

async fn pulse_rts(
    stream: &mut SerialStream,
    duration_ms: Option<u64>,
    state: &SerialControlState,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let original = state.rts.unwrap_or(false);
    stream
        .write_request_to_send(!original)
        .map_err(|error| format!("设置串口 RTS 脉冲失败：{error}"))?;
    let canceled = wait_for_pulse(duration_ms, cancellation).await;
    stream
        .write_request_to_send(original)
        .map_err(|error| format!("恢复串口 RTS 状态失败：{error}"))?;
    if canceled {
        return Err("串口控制已取消".to_string());
    }
    Ok(())
}

async fn wait_for_pulse(duration_ms: Option<u64>, cancellation: &CancellationToken) -> bool {
    let duration = Duration::from_millis(duration_ms.unwrap_or(100).clamp(1, 5_000));
    tokio::select! {
        _ = cancellation.cancelled() => true,
        _ = tokio::time::sleep(duration) => false,
    }
}

pub(super) fn apply_close_lines(
    stream: &mut SerialStream,
    state: SerialControlState,
) -> Result<(), String> {
    if let Some(dtr) = state.dtr_on_close {
        stream
            .write_data_terminal_ready(dtr)
            .map_err(|error| format!("关闭串口时设置 DTR 失败：{error}"))?;
    }
    if let Some(rts) = state.rts_on_close {
        if !state.rts_managed {
            stream
                .write_request_to_send(rts)
                .map_err(|error| format!("关闭串口时设置 RTS 失败：{error}"))?;
        }
    }
    Ok(())
}

fn read_status(stream: &mut SerialStream, state: SerialControlState) -> SerialLineStatus {
    let (actual_dtr, actual_rts) = read_output_lines(stream);
    SerialLineStatus {
        dtr: actual_dtr.or(state.dtr),
        rts: actual_rts.or(state.rts),
        dtr_readback: actual_dtr.is_some(),
        rts_readback: actual_rts.is_some(),
        rts_manual: !state.rts_managed,
        cts: stream.read_clear_to_send().ok(),
        dsr: stream.read_data_set_ready().ok(),
        ring: stream.read_ring_indicator().ok(),
        carrier_detect: stream.read_carrier_detect().ok(),
    }
}

fn rts_managed_error(state: &SerialControlState) -> String {
    if state.hardware_flow_control {
        "串口启用硬件流控时不能手动控制 RTS".to_string()
    } else {
        "串口启用 RS-485 半双工时不能手动控制 RTS".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::SerialControlState;

    #[test]
    fn marks_rts_as_driver_managed_for_hardware_flow_and_rs485() {
        let hardware = SerialControlState::from_profile(&serde_json::json!({
            "flowControl": "hardware"
        }));
        assert!(hardware.hardware_flow_control);
        assert!(hardware.rts_managed);

        let rs485 = SerialControlState::from_profile(&serde_json::json!({
            "rs485Mode": "half-duplex"
        }));
        assert!(!rs485.hardware_flow_control);
        assert!(rs485.rts_managed);

        let manual = SerialControlState::from_profile(&serde_json::json!({}));
        assert!(!manual.rts_managed);
    }

    #[test]
    fn reset_state_keeps_the_open_levels_after_manual_changes() {
        let mut state = SerialControlState::from_profile(&serde_json::json!({
            "dtrOnOpen": false,
            "rtsOnOpen": true
        }));
        state.dtr = Some(true);
        state.rts = Some(false);

        state.dtr = state.initial_dtr;
        state.rts = state.initial_rts;

        assert_eq!(state.dtr, Some(false));
        assert_eq!(state.rts, Some(true));
    }
}
