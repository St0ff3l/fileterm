use super::process_tree::{webview_tree, Process};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;
use std::time::Instant;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, FILETIME, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{
    GetPriorityClass, GetProcessInformation, GetProcessTimes, OpenProcess, ProcessPowerThrottling,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_QUERY_LIMITED_INFORMATION,
};

pub(super) struct OwnedHandle(pub HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn process_list() -> Result<Vec<Process>, u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(unsafe { GetLastError() });
    }
    let snapshot = OwnedHandle(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(unsafe { GetLastError() });
    }
    let mut items = Vec::new();
    loop {
        let end = entry
            .szExeFile
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(entry.szExeFile.len());
        items.push(Process {
            pid: entry.th32ProcessID,
            parent: entry.th32ParentProcessID,
            webview: String::from_utf16_lossy(&entry.szExeFile[..end])
                .eq_ignore_ascii_case("msedgewebview2.exe"),
        });
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            let error = unsafe { GetLastError() };
            if error != windows_sys::Win32::Foundation::ERROR_NO_MORE_FILES {
                return Err(error);
            }
            break;
        }
    }
    Ok(items)
}

pub(super) fn diagnostic_processes(root: u32) -> Result<BTreeSet<u32>, u32> {
    let items = process_list()?;
    if !items
        .iter()
        .any(|item| item.pid == std::process::id() && item.parent == root)
    {
        return Err(windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER);
    }
    Ok(webview_tree(root, &items, &[]))
}

fn filetime(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}

#[cfg(not(feature = "windows-scheduler-fixture"))]
pub(super) fn creation_time(pid: u32) -> Result<u64, u32> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(unsafe { GetLastError() });
    }
    let handle = OwnedHandle(process);
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) } == 0
    {
        return Err(unsafe { GetLastError() });
    }
    Ok(filetime(created))
}

#[derive(Default)]
pub(super) struct Sampler {
    previous: BTreeMap<u32, (u64, u64, Instant)>,
}

fn cpu_percent(
    previous: Option<&(u64, u64, Instant)>,
    creation: u64,
    cpu: u64,
    now: Instant,
) -> Option<f64> {
    let &(old_creation, old_cpu, old_at) = previous?;
    let elapsed = now.checked_duration_since(old_at)?.as_secs_f64();
    if creation != old_creation || cpu < old_cpu || elapsed <= 0.0 {
        return None;
    }
    Some((cpu - old_cpu) as f64 / 100_000.0 / elapsed)
}

impl Sampler {
    fn process(&mut self, pid: u32) -> Value {
        let role = if pid == std::process::id() {
            "fileterm"
        } else {
            "webview2"
        };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return json!({"pid": pid, "role": role, "openErrorCode": unsafe { GetLastError() }});
        }
        let handle = OwnedHandle(process);
        let mut report = json!({"pid": pid, "role": role});
        let (mut created, mut exited, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let now = Instant::now();
        if unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
            != 0
        {
            let creation = filetime(created);
            let cpu = filetime(kernel).saturating_add(filetime(user));
            report["cpuPercentOneCore"] =
                json!(cpu_percent(self.previous.get(&pid), creation, cpu, now));
            report["cpuTime100ns"] = json!(cpu);
            report["creationTime100ns"] = json!(creation);
            self.previous.insert(pid, (creation, cpu, now));
        } else {
            report["cpuErrorCode"] = json!(unsafe { GetLastError() });
        }
        let mut memory = PROCESS_MEMORY_COUNTERS_EX {
            cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        if unsafe {
            K32GetProcessMemoryInfo(
                handle.0,
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                memory.cb,
            )
        } != 0
        {
            report["workingSetBytes"] = json!(memory.WorkingSetSize);
            report["privateBytes"] = json!(memory.PrivateUsage);
        } else {
            report["memoryErrorCode"] = json!(unsafe { GetLastError() });
        }
        let mut throttling = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ..Default::default()
        };
        if unsafe {
            GetProcessInformation(
                handle.0,
                ProcessPowerThrottling,
                (&mut throttling as *mut PROCESS_POWER_THROTTLING_STATE).cast(),
                size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            )
        } != 0
        {
            report["powerThrottling"] =
                json!({"controlMask": throttling.ControlMask, "stateMask": throttling.StateMask});
        } else {
            report["powerThrottlingErrorCode"] = json!(unsafe { GetLastError() });
        }
        let priority = unsafe { GetPriorityClass(handle.0) };
        if priority != 0 {
            report["priorityClass"] = json!(priority);
        } else {
            report["priorityErrorCode"] = json!(unsafe { GetLastError() });
        }
        report
    }

    pub(super) fn sample(&mut self) -> Value {
        let items = match process_list() {
            Ok(items) => items,
            Err(error) => return json!({"snapshotErrorCode": error}),
        };
        let (browsers, registrations) = super::webview_processes::snapshot();
        let selected = webview_tree(std::process::id(), &items, &browsers);
        self.previous.retain(|pid, _| selected.contains(pid));
        // Bound the log line and retained state even if WebView process count grows abnormally.
        let sampled: BTreeSet<_> = std::iter::once(std::process::id())
            .chain(
                selected
                    .iter()
                    .copied()
                    .filter(|pid| *pid != std::process::id()),
            )
            .take(32)
            .collect();
        self.previous.retain(|pid, _| sampled.contains(pid));
        let samples: Vec<_> = sampled.iter().map(|pid| self.process(*pid)).collect();
        let memory_read_count = samples
            .iter()
            .filter(|sample| sample.get("privateBytes").is_some())
            .count();
        json!({
            "discoveryMethod": "parent-tree+webview2-browser-id",
            "webview2Count": selected.len().saturating_sub(1),
            "browserRegistrations": registrations,
            "memoryReadCount": memory_read_count,
            "accessMode": "query-limited",
            "count": selected.len(),
            "truncated": selected.len() > sampled.len(),
            "samples": samples
        })
    }
}

#[cfg(test)]
mod tests;
