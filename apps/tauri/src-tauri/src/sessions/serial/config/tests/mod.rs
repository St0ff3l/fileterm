use super::*;

#[test]
fn accepts_supported_serial_line_options() {
    assert!(data_bits(8).is_ok());
    assert!(stop_bits(2).is_ok());
    assert!(parity("even").is_ok());
    assert_eq!(parity("mark").unwrap(), SerialParity::Mark);
    assert_eq!(parity("space").unwrap(), SerialParity::Space);
    assert!(flow_control("hardware").is_ok());
    assert_eq!(flow_control("software").unwrap(), FlowControl::Software);
    assert!(parity("invalid").is_err());
}

#[test]
fn maps_common_windows_and_unix_serial_errors() {
    assert!(serial_error("COM3", "Access is denied").contains("权限不足"));
    assert!(serial_error("COM3", "The system cannot find the file specified").contains("不存在"));
    assert!(serial_error("/dev/cu.usbserial", "resource is in use").contains("占用"));
}
