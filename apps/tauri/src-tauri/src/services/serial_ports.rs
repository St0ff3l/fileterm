//! Serial-port enumeration and low-cost hot-plug detection.
//!
//! `tokio-serial` does not expose one portable OS notification API. A bounded
//! snapshot watcher gives macOS, Windows, and Linux the same behavior while
//! avoiding a platform-specific native event implementation in the renderer.

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::time::timeout;

const SCAN_TIMEOUT: Duration = Duration::from_secs(5);
const WATCH_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortSnapshot {
    pub port_name: String,
    pub port_type: String,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

pub async fn list() -> Result<Vec<SerialPortSnapshot>, String> {
    let scan_result = timeout(
        SCAN_TIMEOUT,
        tauri::async_runtime::spawn_blocking(tokio_serial::available_ports),
    )
    .await
    .map_err(|_| "串口设备扫描超时".to_string())?;
    let ports = scan_result
        .map_err(|error| format!("串口设备扫描失败：{error}"))?
        .map_err(|error| format!("串口设备扫描失败：{error}"))?;
    Ok(ports.into_iter().map(map_serial_port_info).collect())
}

/// Start a process-lifetime watcher. The connection modal still performs a
/// direct refresh for the initial state and as a fallback; this event makes a
/// cable insertion/removal visible while the modal is already open without
/// requiring renderer-only polling to be the sole source of truth.
pub fn start_watcher(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut previous = None;
        loop {
            if let Ok(ports) = list().await {
                let next = fingerprint(&ports);
                if previous.as_deref() != Some(next.as_str()) {
                    let _ = app.emit("serial:ports-changed", &ports);
                    previous = Some(next);
                }
            }
            tokio::time::sleep(WATCH_INTERVAL).await;
        }
    });
}

pub(crate) fn map_serial_port_info(port: tokio_serial::SerialPortInfo) -> SerialPortSnapshot {
    let (port_type, manufacturer, product, serial_number, vendor_id, product_id) = match port
        .port_type
    {
        tokio_serial::SerialPortType::UsbPort(info) => (
            "usb",
            info.manufacturer,
            info.product,
            info.serial_number,
            Some(info.vid),
            Some(info.pid),
        ),
        tokio_serial::SerialPortType::PciPort => ("pci", None, None, None, None, None),
        tokio_serial::SerialPortType::BluetoothPort => ("bluetooth", None, None, None, None, None),
        tokio_serial::SerialPortType::Unknown => ("unknown", None, None, None, None, None),
    };

    SerialPortSnapshot {
        port_name: port.port_name,
        port_type: port_type.to_string(),
        manufacturer,
        product,
        serial_number,
        vendor_id,
        product_id,
    }
}

fn fingerprint(ports: &[SerialPortSnapshot]) -> String {
    let mut values = ports
        .iter()
        .map(|port| {
            format!(
                "{}|{}|{}|{}|{}|{}|{}",
                port.port_name,
                port.port_type,
                port.manufacturer.as_deref().unwrap_or_default(),
                port.product.as_deref().unwrap_or_default(),
                port.serial_number.as_deref().unwrap_or_default(),
                port.vendor_id.unwrap_or_default(),
                port.product_id.unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.join("\n")
}
