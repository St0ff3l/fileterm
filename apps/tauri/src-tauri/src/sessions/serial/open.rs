use std::time::Duration;

use tokio_serial::{SerialPortBuilder, SerialPortBuilderExt, SerialStream};
use tokio_util::sync::CancellationToken;

use super::control::{apply_close_lines, SerialControlState};

const SERIAL_OPEN_TIMEOUT: Duration = Duration::from_secs(15);

// A canceled blocking open can finish later; its result still owns line cleanup.
struct PendingSerialPort {
    stream: Option<SerialStream>,
    control: SerialControlState,
}

impl Drop for PendingSerialPort {
    fn drop(&mut self) {
        if let Some(stream) = self.stream.as_mut() {
            if let Err(error) = apply_close_lines(stream, self.control) {
                crate::services::logging::write_global(
                    "WARN",
                    "serial",
                    format!("late serial open cleanup failed: {error}"),
                );
            }
        }
    }
}

pub(super) async fn open_serial_port(
    builder: SerialPortBuilder,
    control: SerialControlState,
    cancellation: &CancellationToken,
) -> Result<SerialStream, String> {
    let runtime = tokio::runtime::Handle::current();
    let mut opened = wait_for_open(
        move || {
            let _entered = runtime.enter();
            builder
                .open_native_async()
                .map(|stream| PendingSerialPort {
                    stream: Some(stream),
                    control,
                })
                .map_err(|error| error.to_string())
        },
        cancellation,
        SERIAL_OPEN_TIMEOUT,
    )
    .await?;
    Ok(opened.stream.take().unwrap())
}

async fn wait_for_open<T: Send + 'static>(
    open: impl FnOnce() -> Result<T, String> + Send + 'static,
    cancellation: &CancellationToken,
    wait: Duration,
) -> Result<T, String> {
    if cancellation.is_cancelled() {
        return Err("串口打开已取消".to_string());
    }
    let mut task = tokio::task::spawn_blocking(open);
    let result = tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err("串口打开已取消".to_string()),
        result = tokio::time::timeout(wait, &mut task) => match result {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(format!("串口打开任务失败：{error}")),
            Err(_) => Err("串口打开超时，请检查设备占用和驱动状态".to_string()),
        },
    };
    // abort only stops work that has not begun. Running opens release their late result on drop.
    if result.is_err() {
        task.abort();
    }
    result
}

#[cfg(test)]
mod tests;
