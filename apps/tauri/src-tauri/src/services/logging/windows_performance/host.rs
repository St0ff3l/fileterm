use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemInformation::{
    CpuSetInformation, GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
};

fn registry_string(path: &str, name: &str) -> Value {
    let path: Vec<_> = path.encode_utf16().chain(Some(0)).collect();
    let name: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
    let mut buffer = [0u16; 256];
    let mut bytes = size_of::<[u16; 256]>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if status != 0 {
        return json!({"errorCode": status});
    }
    let end = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    json!(String::from_utf16_lossy(&buffer[..end]).trim())
}

pub(super) fn cpu_set_records() -> Result<Vec<(u16, u8, u8, u8)>, Value> {
    let mut bytes = 0;
    unsafe {
        GetSystemCpuSetInformation(null_mut(), 0, &mut bytes, null_mut(), 0);
    }
    if bytes == 0 || bytes > 1024 * 1024 {
        return Err(json!({"errorCode": unsafe { GetLastError() }}));
    }
    // The API writes variable-size records; keep the allocation aligned and validate each record.
    let mut storage = vec![0u64; (bytes as usize).div_ceil(size_of::<u64>())];
    let capacity = bytes;
    if unsafe {
        GetSystemCpuSetInformation(
            storage.as_mut_ptr().cast(),
            capacity,
            &mut bytes,
            null_mut(),
            0,
        )
    } == 0
    {
        return Err(json!({"errorCode": unsafe { GetLastError() }}));
    }
    let data = unsafe {
        std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), bytes.min(capacity) as usize)
    };
    let mut offset = 0;
    let mut records = Vec::new();
    while offset + size_of::<SYSTEM_CPU_SET_INFORMATION>() <= data.len() {
        let record = unsafe {
            data.as_ptr()
                .add(offset)
                .cast::<SYSTEM_CPU_SET_INFORMATION>()
                .read_unaligned()
        };
        let size = record.Size as usize;
        if size < size_of::<SYSTEM_CPU_SET_INFORMATION>() || size > data.len() - offset {
            return Err(json!({"error": "invalid-cpu-set-record"}));
        }
        if record.Type == CpuSetInformation {
            let cpu = unsafe { record.Anonymous.CpuSet };
            records.push((
                cpu.Group,
                cpu.LogicalProcessorIndex,
                cpu.CoreIndex,
                cpu.EfficiencyClass,
            ));
        }
        offset += size;
    }
    if offset != data.len() || records.is_empty() {
        return Err(json!({"error": "incomplete-cpu-set-records"}));
    }
    Ok(records)
}

fn cpu_sets() -> Value {
    let records = match cpu_set_records() {
        Ok(records) => records,
        Err(error) => return error,
    };
    let mut classes: BTreeMap<u8, (usize, BTreeSet<(u16, u8)>)> = BTreeMap::new();
    for (group, _, core, class) in records {
        let entry = classes.entry(class).or_default();
        entry.0 += 1;
        entry.1.insert((group, core));
    }
    json!({
        "efficiencyClasses": classes.iter().map(|(class, (logical, cores))| json!({
            "class": class, "logicalProcessors": logical, "physicalCores": cores.len()
        })).collect::<Vec<_>>(),
        "multipleEfficiencyClasses": classes.len() > 1,
    })
}

pub(super) fn snapshot() -> Value {
    let windows = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    json!({
        "windowsBuild": registry_string(windows, "CurrentBuildNumber"),
        "displayVersion": registry_string(windows, "DisplayVersion"),
        "edition": registry_string(windows, "EditionID"),
        "architecture": std::env::consts::ARCH,
        "cpuModel": registry_string(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString"),
        "cpuTopology": cpu_sets(),
    })
}

pub(super) fn power_status() -> Value {
    let mut status = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut status) } == 0 {
        return json!({"errorCode": unsafe { GetLastError() }});
    }
    json!({"acLineStatus": status.ACLineStatus, "batterySaver": status.SystemStatusFlag})
}

pub(super) fn app_foreground() -> Option<bool> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    let mut pid = 0;
    let thread = unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    (thread != 0).then_some(pid == std::process::id())
}

#[cfg(test)]
mod tests;
