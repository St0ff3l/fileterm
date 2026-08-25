use tokio_serial::{DataBits, FlowControl, Parity, StopBits};

pub(super) fn data_bits(value: u64) -> Result<DataBits, String> {
    match value {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err("串口数据位必须是 5、6、7 或 8".to_string()),
    }
}

pub(super) fn stop_bits(value: u64) -> Result<StopBits, String> {
    match value {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        _ => Err("串口停止位必须是 1 或 2".to_string()),
    }
}

pub(super) fn parity(value: &str) -> Result<Parity, String> {
    match value {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        _ => Err("当前平台的串口校验位必须是无、奇或偶校验".to_string()),
    }
}

pub(super) fn flow_control(value: &str) -> Result<FlowControl, String> {
    match value {
        "none" => Ok(FlowControl::None),
        "software" => Ok(FlowControl::Software),
        "hardware" => Ok(FlowControl::Hardware),
        _ => Err("串口流控必须是无、软件或硬件流控".to_string()),
    }
}

pub(super) fn serial_error(device_path: &str, error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("permission denied")
        || normalized.contains("access is denied")
        || normalized.contains("operation not permitted")
        || normalized.contains("eacces")
    {
        return format!(
            "无法访问串口 {device_path}：权限不足。Linux 请将当前用户加入 dialout 组；Windows/macOS 请确认驱动和设备访问权限。"
        );
    }
    if normalized.contains("no such file")
        || normalized.contains("cannot find the file")
        || normalized.contains("system cannot find")
        || normalized.contains("enoent")
    {
        return format!("串口设备 {device_path} 不存在、不可用或已断开。");
    }
    if normalized.contains("busy")
        || normalized.contains("in use")
        || normalized.contains("resource is in use")
        || normalized.contains("ebusy")
    {
        return format!("串口设备 {device_path} 已被其他程序占用。");
    }
    format!("串口 {device_path}：{message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_serial_line_options() {
        assert!(data_bits(8).is_ok());
        assert!(stop_bits(2).is_ok());
        assert!(parity("even").is_ok());
        assert!(flow_control("hardware").is_ok());
        assert_eq!(flow_control("software").unwrap(), FlowControl::Software);
        assert!(parity("mark").is_err());
        assert!(parity("space").is_err());
    }

    #[test]
    fn maps_common_windows_and_unix_serial_errors() {
        assert!(serial_error("COM3", "Access is denied").contains("权限不足"));
        assert!(
            serial_error("COM3", "The system cannot find the file specified").contains("不存在")
        );
        assert!(serial_error("/dev/cu.usbserial", "resource is in use").contains("占用"));
    }
}
