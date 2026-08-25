use tokio_serial::SerialStream;

use super::config::SerialParity;

const UNSUPPORTED_EXTENDED_PARITY: &str =
    "当前平台或串口驱动不支持标记/空格校验，请选择无、奇或偶校验";

pub(super) fn apply_parity(stream: &SerialStream, parity: SerialParity) -> Result<(), String> {
    match parity {
        SerialParity::None | SerialParity::Odd | SerialParity::Even => Ok(()),
        SerialParity::Mark | SerialParity::Space => apply_extended_parity(stream, parity),
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

#[cfg(not(unix))]
fn read_output_lines_platform(_stream: &SerialStream) -> (Option<bool>, Option<bool>) {
    // Windows exposes modem input lines through GetCommModemStatus, but not
    // the current DTR/RTS output levels. Keep the last requested values as the
    // fallback in control.rs rather than pretending they are read-back values.
    (None, None)
}
