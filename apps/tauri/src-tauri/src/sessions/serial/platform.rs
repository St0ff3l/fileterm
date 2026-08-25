use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::Sleep;
use tokio_serial::{SerialPort, SerialStream};

use super::config::SerialParity;
use crate::services::session_logs::{SerialLogDirection, SerialLogSink};

const UNSUPPORTED_EXTENDED_PARITY: &str =
    "当前平台或串口驱动不支持标记/空格校验，请选择无、奇或偶校验";
#[cfg(target_os = "macos")]
const MACOS_EXTENDED_PARITY_REQUIRES_7_BITS: &str =
    "macOS 的标记/空格校验模拟仅支持 7 数据位，请将数据位设为 7";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SerialParityWireMode {
    Native,
    MacSevenBitMark,
    MacSevenBitSpace,
}

impl SerialParityWireMode {
    pub(super) fn is_emulated(self) -> bool {
        !matches!(self, Self::Native)
    }
}

/// macOS's legacy termios interface has no CMSPAR equivalent. For a 7-bit
/// mark/space link, use the eighth bit of an 8N{1,2} wire frame as the parity
/// bit and validate/strip it on the way back into the logical byte stream.
pub(super) fn parity_wire_mode(
    parity: SerialParity,
    data_bits: u8,
) -> Result<SerialParityWireMode, String> {
    #[cfg(not(target_os = "macos"))]
    let _ = data_bits;

    if matches!(parity, SerialParity::Mark | SerialParity::Space) {
        #[cfg(target_os = "macos")]
        {
            if data_bits != 7 {
                return Err(MACOS_EXTENDED_PARITY_REQUIRES_7_BITS.to_string());
            }
            return Ok(match parity {
                SerialParity::Mark => SerialParityWireMode::MacSevenBitMark,
                SerialParity::Space => SerialParityWireMode::MacSevenBitSpace,
                SerialParity::None | SerialParity::Odd | SerialParity::Even => {
                    SerialParityWireMode::Native
                }
            });
        }
    }
    Ok(SerialParityWireMode::Native)
}

pub(super) fn wire_data_bits(data_bits: u64, mode: SerialParityWireMode) -> u64 {
    if mode.is_emulated() {
        8
    } else {
        data_bits
    }
}

pub(super) fn apply_parity(stream: &SerialStream, parity: SerialParity) -> Result<(), String> {
    match parity {
        SerialParity::None | SerialParity::Odd | SerialParity::Even => Ok(()),
        SerialParity::Mark | SerialParity::Space => apply_extended_parity(stream, parity),
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SerialRs485Mode {
    Disabled,
    #[cfg(target_os = "linux")]
    DriverManaged,
    Software {
        rts_on_send: bool,
        before: Duration,
        after: Duration,
    },
}

pub(super) fn apply_rs485(
    stream: &mut SerialStream,
    mode: &str,
    rts_on_send: bool,
    delay_before_send_ms: u32,
    delay_after_send_ms: u32,
) -> Result<SerialRs485Mode, String> {
    match mode {
        "none" => {
            apply_rs485_platform(stream, false, rts_on_send, 0, 0)?;
            Ok(SerialRs485Mode::Disabled)
        }
        "half-duplex" => {
            apply_rs485_platform(
                stream,
                true,
                rts_on_send,
                delay_before_send_ms,
                delay_after_send_ms,
            )?;
            #[cfg(target_os = "linux")]
            {
                Ok(SerialRs485Mode::DriverManaged)
            }
            #[cfg(target_os = "macos")]
            {
                Ok(SerialRs485Mode::Software {
                    rts_on_send,
                    before: Duration::from_millis(u64::from(delay_before_send_ms)),
                    after: Duration::from_millis(u64::from(delay_after_send_ms)),
                })
            }
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            {
                Err("当前平台暂不支持内置 RS-485 半双工控制，请使用驱动或外部转换器".to_string())
            }
        }
        _ => Err("串口 RS-485 模式无效".to_string()),
    }
}

#[cfg(target_os = "linux")]
fn apply_rs485_platform(
    stream: &mut SerialStream,
    enabled: bool,
    rts_on_send: bool,
    delay_before_send_ms: u32,
    delay_after_send_ms: u32,
) -> Result<(), String> {
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;

    // Linux's serial_rs485 layout is five u32 fields followed by padding for
    // ABI compatibility. libc exposes the ioctl numbers but not this struct.
    #[repr(C)]
    struct SerialRs485 {
        flags: u32,
        delay_rts_before_send: u32,
        delay_rts_after_send: u32,
        padding: [u32; 5],
    }

    const SER_RS485_ENABLED: u32 = 1;
    const SER_RS485_RTS_ON_SEND: u32 = 1 << 1;
    const SER_RS485_RTS_AFTER_SEND: u32 = 1 << 2;

    let flags = if enabled {
        SER_RS485_ENABLED
            | if rts_on_send {
                SER_RS485_RTS_ON_SEND
            } else {
                SER_RS485_RTS_AFTER_SEND
            }
    } else {
        0
    };
    let options = SerialRs485 {
        flags,
        delay_rts_before_send: delay_before_send_ms,
        delay_rts_after_send: delay_after_send_ms,
        padding: [0; 5],
    };
    let result = unsafe { libc::ioctl(stream.as_raw_fd(), libc::TIOCSRS485, &options) };
    if result != 0 {
        return Err(format!(
            "应用 Linux RS-485 配置失败：{}",
            std::io::Error::last_os_error()
        ));
    }

    // TIOCSRS485 can be accepted by a driver while silently normalizing or
    // dropping unsupported flags. Read the effective value back before the
    // worker starts; otherwise a profile can claim half-duplex while the
    // adapter remains in ordinary UART mode.
    let mut effective = MaybeUninit::<SerialRs485>::zeroed();
    let result =
        unsafe { libc::ioctl(stream.as_raw_fd(), libc::TIOCGRS485, effective.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "读取 Linux RS-485 生效配置失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    let effective = unsafe { effective.assume_init() };
    let direction_mask = SER_RS485_RTS_ON_SEND | SER_RS485_RTS_AFTER_SEND;
    let effective_enabled = effective.flags & SER_RS485_ENABLED != 0;
    if effective_enabled != enabled {
        return Err(format!(
            "Linux RS-485 驱动未接受启用状态（请求：{enabled}，实际：{effective_enabled}）"
        ));
    }
    if enabled && effective.flags & direction_mask != flags & direction_mask {
        return Err("Linux RS-485 驱动未接受 RTS 方向配置".to_string());
    }
    if enabled
        && (effective.delay_rts_before_send != delay_before_send_ms
            || effective.delay_rts_after_send != delay_after_send_ms)
    {
        return Err(format!(
            "Linux RS-485 驱动未接受延迟配置（实际：{}ms/{}ms）",
            effective.delay_rts_before_send, effective.delay_rts_after_send
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_rs485_platform(
    stream: &mut SerialStream,
    enabled: bool,
    rts_on_send: bool,
    _delay_before_send_ms: u32,
    _delay_after_send_ms: u32,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if enabled {
            stream
                .write_request_to_send(!rts_on_send)
                .map_err(|error| format!("配置 macOS RS-485 软件 RTS 失败：{error}"))?;
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (stream, rts_on_send);
        if enabled {
            Err("当前平台暂不支持内置 RS-485 半双工控制，请使用驱动或外部转换器".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn apply_extended_parity(stream: &SerialStream, parity: SerialParity) -> Result<(), String> {
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;

    let fd = stream.as_raw_fd();
    let mut options = MaybeUninit::<libc::termios2>::uninit();
    let result = unsafe { libc::ioctl(fd, libc::TCGETS2, options.as_mut_ptr()) };
    if result != 0 {
        return Err(format!(
            "{UNSUPPORTED_EXTENDED_PARITY}：读取 Linux termios2 失败：{}",
            std::io::Error::last_os_error()
        ));
    }

    let mut options = unsafe { options.assume_init() };
    options.c_cflag |= libc::PARENB | libc::CMSPAR;
    match parity {
        SerialParity::Mark => options.c_cflag |= libc::PARODD,
        SerialParity::Space => options.c_cflag &= !libc::PARODD,
        SerialParity::None | SerialParity::Odd | SerialParity::Even => return Ok(()),
    }
    options.c_iflag |= libc::INPCK;
    options.c_iflag &= !libc::IGNPAR;

    let result = unsafe { libc::ioctl(fd, libc::TCSETS2, &options) };
    if result != 0 {
        return Err(format!(
            "{UNSUPPORTED_EXTENDED_PARITY}：写入 Linux termios2 失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_extended_parity(stream: &SerialStream, parity: SerialParity) -> Result<(), String> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Devices::Communication::{
        GetCommState, SetCommState, DCB, MARKPARITY, SPACEPARITY,
    };

    let handle = stream.as_raw_handle();
    let mut state = DCB {
        DCBlength: size_of::<DCB>() as u32,
        ..Default::default()
    };
    if unsafe { GetCommState(handle, &mut state) } == 0 {
        return Err(format!(
            "{UNSUPPORTED_EXTENDED_PARITY}：读取 Windows DCB 失败：{}",
            std::io::Error::last_os_error()
        ));
    }

    state.Parity = match parity {
        SerialParity::Mark => MARKPARITY,
        SerialParity::Space => SPACEPARITY,
        SerialParity::None | SerialParity::Odd | SerialParity::Even => return Ok(()),
    };
    // DCB.fParity is bit 1 in the documented bit-field. Keep all other
    // builder-selected DCB flags untouched.
    state._bitfield |= 1 << 1;

    if unsafe { SetCommState(handle, &state) } == 0 {
        return Err(format!(
            "{UNSUPPORTED_EXTENDED_PARITY}：写入 Windows DCB 失败：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn apply_extended_parity(_stream: &SerialStream, _parity: SerialParity) -> Result<(), String> {
    Err(UNSUPPORTED_EXTENDED_PARITY.to_string())
}

fn encode_wire_bytes(mode: SerialParityWireMode, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match mode {
        SerialParityWireMode::Native => Ok(bytes.to_vec()),
        SerialParityWireMode::MacSevenBitMark => bytes
            .iter()
            .map(|byte| {
                if byte & 0x80 != 0 {
                    Err(
                        "macOS 标记/空格校验模拟只能发送 7 位数据，请使用 ASCII 或关闭该校验"
                            .to_string(),
                    )
                } else {
                    Ok(byte | 0x80)
                }
            })
            .collect(),
        SerialParityWireMode::MacSevenBitSpace => bytes
            .iter()
            .map(|byte| {
                if byte & 0x80 != 0 {
                    Err(
                        "macOS 标记/空格校验模拟只能发送 7 位数据，请使用 ASCII 或关闭该校验"
                            .to_string(),
                    )
                } else {
                    Ok(*byte)
                }
            })
            .collect(),
    }
}

fn decode_wire_bytes(mode: SerialParityWireMode, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match mode {
        SerialParityWireMode::Native => Ok(bytes.to_vec()),
        SerialParityWireMode::MacSevenBitMark => bytes
            .iter()
            .map(|byte| {
                if byte & 0x80 == 0 {
                    Err("macOS 标记校验收到校验位错误的数据".to_string())
                } else {
                    Ok(byte & 0x7f)
                }
            })
            .collect(),
        SerialParityWireMode::MacSevenBitSpace => bytes
            .iter()
            .map(|byte| {
                if byte & 0x80 != 0 {
                    Err("macOS 空格校验收到校验位错误的数据".to_string())
                } else {
                    Ok(*byte)
                }
            })
            .collect(),
    }
}

fn serial_io_error(kind: io::ErrorKind, message: impl Into<String>) -> io::Error {
    io::Error::new(kind, message.into())
}

pub(super) struct SerialIo {
    inner: SerialStream,
    parity: SerialParityWireMode,
    rs485: SerialRs485Mode,
    wire_log: Option<SerialLogSink>,
    tx_active: bool,
    before_sleep: Option<Pin<Box<Sleep>>>,
    after_sleep: Option<Pin<Box<Sleep>>>,
}

impl SerialIo {
    pub(super) fn new(
        inner: SerialStream,
        parity: SerialParityWireMode,
        rs485: SerialRs485Mode,
    ) -> Self {
        Self {
            inner,
            parity,
            rs485,
            wire_log: None,
            tx_active: false,
            before_sleep: None,
            after_sleep: None,
        }
    }

    pub(super) fn serial_mut(&mut self) -> &mut SerialStream {
        &mut self.inner
    }

    pub(super) fn set_wire_log(&mut self, sink: Option<SerialLogSink>) {
        self.wire_log = sink;
    }

    pub(super) fn release_rs485(&mut self) -> Result<(), String> {
        if let SerialRs485Mode::Software { rts_on_send, .. } = self.rs485 {
            self.before_sleep = None;
            self.after_sleep = None;
            self.tx_active = false;
            self.inner
                .write_request_to_send(!rts_on_send)
                .map_err(|error| format!("释放 macOS RS-485 软件 RTS 失败：{error}"))?;
        }
        Ok(())
    }

    fn poll_before_send(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let SerialRs485Mode::Software {
            rts_on_send,
            before,
            ..
        } = self.rs485
        else {
            return Poll::Ready(Ok(()));
        };

        if !self.tx_active {
            if let Err(error) = self.inner.write_request_to_send(rts_on_send) {
                return Poll::Ready(Err(serial_io_error(
                    io::ErrorKind::Other,
                    format!("设置 RS-485 RTS 失败：{error}"),
                )));
            }
            self.tx_active = true;
            if !before.is_zero() {
                self.before_sleep = Some(Box::pin(tokio::time::sleep(before)));
            }
        }

        if let Some(sleep) = self.before_sleep.as_mut() {
            if sleep.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.before_sleep = None;
        }
        Poll::Ready(Ok(()))
    }

    fn poll_finish_send(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let SerialRs485Mode::Software {
            rts_on_send, after, ..
        } = self.rs485
        else {
            return Poll::Ready(Ok(()));
        };
        if !self.tx_active {
            return Poll::Ready(Ok(()));
        }

        if !after.is_zero() {
            let sleep = self
                .after_sleep
                .get_or_insert_with(|| Box::pin(tokio::time::sleep(after)));
            if sleep.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            self.after_sleep = None;
        }

        match self.inner.write_request_to_send(!rts_on_send) {
            Ok(()) => {
                self.tx_active = false;
                Poll::Ready(Ok(()))
            }
            Err(error) => Poll::Ready(Err(serial_io_error(
                io::ErrorKind::Other,
                format!("释放 RS-485 RTS 失败：{error}"),
            ))),
        }
    }

    fn poll_flush_inner(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => self.poll_finish_send(cx),
            Poll::Ready(Err(error)) => {
                let _ = self.release_rs485();
                Poll::Ready(Err(error))
            }
        }
    }
}

impl Unpin for SerialIo {}

impl Drop for SerialIo {
    fn drop(&mut self) {
        if let SerialRs485Mode::Software { rts_on_send, .. } = self.rs485 {
            let _ = self.inner.write_request_to_send(!rts_on_send);
        }
    }
}

impl AsyncRead for SerialIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if !self.parity.is_emulated() {
            let before = buf.filled().len();
            let result = Pin::new(&mut self.inner).poll_read(cx, buf);
            if let Poll::Ready(Ok(())) = &result {
                if let Some(sink) = &self.wire_log {
                    sink.append_wire(SerialLogDirection::Rx, &buf.filled()[before..]);
                }
            }
            return result;
        }

        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut wire = vec![0_u8; buf.remaining()];
        let mut wire_buf = ReadBuf::new(&mut wire);
        match Pin::new(&mut self.inner).poll_read(cx, &mut wire_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                if let Some(sink) = &self.wire_log {
                    sink.append_wire(SerialLogDirection::Rx, wire_buf.filled());
                }
                match decode_wire_bytes(self.parity, wire_buf.filled()) {
                    Ok(decoded) => {
                        buf.put_slice(&decoded);
                        Poll::Ready(Ok(()))
                    }
                    Err(error) => {
                        Poll::Ready(Err(serial_io_error(io::ErrorKind::InvalidData, error)))
                    }
                }
            }
        }
    }
}

impl AsyncWrite for SerialIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let wire = match encode_wire_bytes(self.parity, bytes) {
            Ok(wire) => wire,
            Err(error) => {
                return Poll::Ready(Err(serial_io_error(io::ErrorKind::InvalidInput, error)))
            }
        };
        match self.poll_before_send(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => match Pin::new(&mut self.inner).poll_write(cx, &wire) {
                Poll::Ready(Ok(written)) => {
                    if let Some(sink) = &self.wire_log {
                        sink.append_wire(SerialLogDirection::Tx, &wire[..written]);
                    }
                    Poll::Ready(Ok(written))
                }
                Poll::Ready(Err(error)) => {
                    let _ = self.release_rs485();
                    Poll::Ready(Err(error))
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().poll_flush_inner(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match this.poll_flush_inner(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => Pin::new(&mut this.inner).poll_shutdown(cx),
        }
    }
}

pub(super) fn read_output_lines(stream: &SerialStream) -> (Option<bool>, Option<bool>) {
    read_output_lines_platform(stream)
}

#[cfg(unix)]
fn read_output_lines_platform(stream: &SerialStream) -> (Option<bool>, Option<bool>) {
    use std::os::fd::AsRawFd;

    let mut status = 0_i32;
    let result = unsafe {
        libc::ioctl(
            stream.as_raw_fd(),
            libc::TIOCMGET,
            &mut status as *mut libc::c_int,
        )
    };
    if result != 0 {
        return (None, None);
    }
    (
        Some(status & libc::TIOCM_DTR != 0),
        Some(status & libc::TIOCM_RTS != 0),
    )
}

#[cfg(target_os = "windows")]
fn read_output_lines_platform(_stream: &SerialStream) -> (Option<bool>, Option<bool>) {
    // Windows GetCommState reports the DCB's requested control mode, not the
    // electrical output level. Keep the fallback in control.rs so the UI says
    // "remembered" instead of presenting configuration as physical readback.
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::{decode_wire_bytes, encode_wire_bytes, wire_data_bits, SerialParityWireMode};

    #[test]
    fn mac_mark_and_space_wire_bytes_round_trip_seven_bit_values() {
        let logical = [0x00, 0x41, 0x7f];
        let mark_wire = encode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &logical).unwrap();
        let space_wire =
            encode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &logical).unwrap();
        assert_eq!(mark_wire, [0x80, 0xc1, 0xff]);
        assert_eq!(space_wire, logical);
        assert_eq!(
            decode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &mark_wire).unwrap(),
            logical
        );
        assert_eq!(
            decode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &space_wire).unwrap(),
            logical
        );
    }

    #[test]
    fn mac_mark_and_space_reject_eight_bit_payloads_or_bad_wire_parity() {
        assert!(encode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &[0x80]).is_err());
        assert!(encode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &[0x80]).is_err());
        assert!(decode_wire_bytes(SerialParityWireMode::MacSevenBitMark, &[0x01]).is_err());
        assert!(decode_wire_bytes(SerialParityWireMode::MacSevenBitSpace, &[0x81]).is_err());
    }

    #[test]
    fn emulated_parity_uses_eight_wire_data_bits() {
        assert_eq!(wire_data_bits(7, SerialParityWireMode::MacSevenBitMark), 8);
        assert_eq!(wire_data_bits(8, SerialParityWireMode::Native), 8);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn serial_io_applies_mark_parity_on_the_wire_without_a_physical_device() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_serial::SerialStream;

        let (left, right) = SerialStream::pair().unwrap();
        let mut writer = super::SerialIo::new(
            left,
            SerialParityWireMode::MacSevenBitMark,
            super::SerialRs485Mode::Disabled,
        );
        let mut raw_reader = right;
        writer.write_all(&[0x41]).await.unwrap();
        let mut wire = [0_u8; 1];
        raw_reader.read_exact(&mut wire).await.unwrap();
        assert_eq!(wire, [0xc1]);

        let (raw_writer, right) = SerialStream::pair().unwrap();
        let mut raw_writer = raw_writer;
        let mut reader = super::SerialIo::new(
            right,
            SerialParityWireMode::MacSevenBitMark,
            super::SerialRs485Mode::Disabled,
        );
        raw_writer.write_all(&[0xc1]).await.unwrap();
        let mut logical = [0_u8; 1];
        reader.read_exact(&mut logical).await.unwrap();
        assert_eq!(logical, [0x41]);
    }
}

#[cfg(all(not(unix), not(target_os = "windows")))]
fn read_output_lines_platform(_stream: &SerialStream) -> (Option<bool>, Option<bool>) {
    // Windows exposes modem input lines through GetCommModemStatus, but not
    // the current DTR/RTS output levels. Keep the last requested values as the
    // fallback in control.rs rather than pretending they are read-back values.
    (None, None)
}
