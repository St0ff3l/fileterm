use super::{
    host, processes, scheduler_etw,
    scheduler_model::SchedulerModel,
    scheduler_policy::{exceeds_cpu_budget, process_cpu, HELPER_CPU_LIMIT_100NS},
    scheduler_sessions, scheduler_threads,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_ALL_ASSIGNED};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetActiveProcessorGroupCount, GetCurrentProcess, OpenProcessToken,
    BELOW_NORMAL_PRIORITY_CLASS, CREATE_NO_WINDOW,
};

const MAX_REPORT_BYTES: usize = 12 * 1024;
static TRIGGER: OnceLock<mpsc::SyncSender<u64>> = OnceLock::new();
static STATUS: Mutex<Value> = Mutex::new(Value::Null);

pub(super) fn status() -> Value {
    STATUS
        .try_lock()
        .ok()
        .map(|status| status.clone())
        .unwrap_or(Value::Null)
}

pub(super) fn observe(scope: &str, message: &str) {
    let Some(sender) = TRIGGER.get() else {
        return;
    };
    if is_renderer_stall_warning(scope, message) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let _ = sender.try_send(now);
    }
}

fn is_renderer_stall_warning(scope: &str, message: &str) -> bool {
    scope == "renderer:performance"
        && message.len() <= 16 * 1024
        && serde_json::from_str::<Value>(message)
            .ok()
            .is_some_and(|report| {
                matches!(
                    report["event"].as_str(),
                    Some(
                        "event-loop-gap"
                            | "terminal-output-parse"
                            | "renderer-long-frame"
                            | "renderer-progress-slow"
                    )
                )
            })
}

pub(super) fn start(ui_tid: u32, version: String) {
    let (sender, receiver) = mpsc::sync_channel(1);
    if TRIGGER.set(sender).is_err() {
        return;
    }
    let result = std::thread::Builder::new().name("scheduler-diagnostics".into()).spawn(move || {
        // Let the main WebView start before taking the first thread snapshot.
        std::thread::sleep(Duration::from_secs(10));
        let mut reason = "startup-recovery";
        let mut trigger_at = None;
        let mut recovery_only = true;
        loop {
            let started = Instant::now();
            let mut report = run_child(ui_tid, recovery_only);
            report["event"] = json!("windows-thread-scheduling");
            report["pid"] = json!(std::process::id());
            report["version"] = json!(version);
            report["appForeground"] = json!(host::app_foreground());
            report["trigger"] = json!(reason);
            report["collectionMode"] = json!("warning-triggered");
            report["triggerUnixMs"] = json!(trigger_at);
            report["durationMs"] = json!(started.elapsed().as_millis());
            let stopped = !matches!(report["status"].as_str(), Some("captured" | "recovered")) || report["budgetExceeded"] == true
                || report["helperBudgetExceeded"] == true
                || report["stopErrorCode"].as_u64().is_some_and(|code| code != 0)
                || report["unsupportedEvents"].as_u64().is_some_and(|count| count != 0)
                || report["eventsLost"].as_u64().is_some_and(|count| count != 0)
                || report["buffersLost"].as_u64().is_some_and(|count| count != 0)
                || started.elapsed() > Duration::from_secs(5);
            report["automaticCaptureStopped"] = json!(stopped);
            if let Ok(mut status) = STATUS.lock() {
                *status = json!({"status": report["status"], "stage": report["stage"],
                    "errorCode": report["errorCode"], "requiresElevation": report["requiresElevation"],
                    "evidenceUsable": report["evidenceUsable"], "automaticCaptureStopped": stopped,
                    "collectionMode": "warning-triggered"});
            }
            super::super::info_global("app:windows-performance", report.to_string());
            // Failed capture/permissions/overload: keep health logs, avoid recurring expensive retries.
            if stopped { return; }
            let cooldown = if recovery_only { Duration::ZERO } else { Duration::from_secs(30).saturating_sub(started.elapsed()) };
            std::thread::sleep(cooldown);
            (reason, trigger_at) = match receiver.recv() {
                Ok(at) => ("renderer-warning", Some(at)),
                Err(_) => return,
            };
            recovery_only = false;
        }
    });
    if result.is_err() {
        if let Ok(mut status) = STATUS.lock() {
            *status = json!({"status": "unavailable", "stage": "scheduler-thread"});
        }
    }
}

fn run_child(ui_tid: u32, recovery_only: bool) -> Value {
    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return json!({"status": "unavailable", "stage": "helper-path"}),
    };
    let mut child = match Command::new(executable)
        .arg("--fileterm-scheduler-diagnostic")
        .arg(std::process::id().to_string())
        .arg(ui_tid.to_string())
        .args(recovery_only.then_some("--recover-only"))
        .creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "helper-spawn", "errorCode": error.raw_os_error()})
        }
    };
    // Read concurrently: a pipe smaller than the JSON report must not block the helper's exit.
    let stdout = child.stdout.take().unwrap();
    let reader = std::thread::Builder::new()
        .name("scheduler-helper-output".into())
        .spawn(move || {
            let mut data = Vec::new();
            stdout
                .take(MAX_REPORT_BYTES as u64 + 1)
                .read_to_end(&mut data)
                .map(|_| data)
        });
    let reader = match reader {
        Ok(reader) => reader,
        Err(_) => {
            let cleanup = scheduler_sessions::cleanup_session(std::process::id());
            let _ = child.kill();
            let _ = child.wait();
            return json!({"status": "unavailable", "stage": "helper-reader", "helperCleanupErrorCode": cleanup});
        }
    };
    let started = Instant::now();
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < Duration::from_secs(8) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            _ => {
                timed_out = true;
                // Attempt to stop only our session before terminating a stuck consumer.
                scheduler_sessions::cleanup_session(std::process::id());
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let output = reader.join();
    // Also clean up a session left by an unexpected helper exit.
    let cleanup = scheduler_sessions::cleanup_session(std::process::id());
    if timed_out {
        return json!({"status": "unavailable", "stage": "helper-timeout", "helperCleanupErrorCode": cleanup});
    }
    let mut report = match output {
        Ok(Ok(data)) if data.len() <= MAX_REPORT_BYTES => serde_json::from_slice::<Value>(&data)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({"status": "unavailable", "stage": "helper-json"})),
        _ => json!({"status": "unavailable", "stage": "helper-output"}),
    };
    report["helperCleanupErrorCode"] = json!(cleanup);
    report
}

pub(super) fn enable_profile_privilege() -> Result<(), u32> {
    let mut token = std::ptr::null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(unsafe { GetLastError() });
    }
    let token = processes::OwnedHandle(token);
    let mut luid = Default::default();
    let name: Vec<_> = "SeSystemProfilePrivilege"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 {
        return Err(unsafe { GetLastError() });
    }
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    if unsafe {
        AdjustTokenPrivileges(
            token.0,
            0,
            &privileges,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(unsafe { GetLastError() });
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_NOT_ALL_ASSIGNED {
        return Err(error);
    }
    Ok(())
}

/// Internal helper entry, before any WebView/Tauri runtime is initialized.
pub(super) fn run_helper() {
    let mut report = helper_report();
    // Keep structured JSON intact under the normal log formatter's 16KiB line bound.
    // Reserve space for final budget fields without breaking the pipe's JSON size limit.
    while report.to_string().len() + 1 > MAX_REPORT_BYTES - 128 {
        let Some(threads) = report["threads"].as_array_mut() else {
            break;
        };
        if threads.pop().is_none() {
            break;
        }
        report["threadsTruncated"] = json!(true);
    }
    // Include loader/startup CPU too: this is the helper's lifetime process total.
    let used = process_cpu();
    let exceeded = exceeds_cpu_budget(used, HELPER_CPU_LIMIT_100NS);
    report["helperCpuMs"] = json!(used.map(|used| used as f64 / 10_000.0));
    report["helperBudgetExceeded"] = json!(exceeded);
    if exceeded {
        report["evidenceUsable"] = json!(false);
    }
    println!("{report}");
}

fn helper_report() -> Value {
    let args: Vec<_> = std::env::args().skip(2).collect();
    let parsed = args
        .first()
        .and_then(|arg| arg.parse::<u32>().ok())
        .zip(args.get(1).and_then(|arg| arg.parse::<u32>().ok()));
    let recovery_only = args.get(2).is_some_and(|arg| arg == "--recover-only");
    let Some((root, ui_tid)) = parsed
        .filter(|(root, _)| *root != 0 && (args.len() == 2 || (args.len() == 3 && recovery_only)))
    else {
        return json!({"status": "unavailable", "stage": "helper-arguments"});
    };
    let pids = match processes::diagnostic_processes(root) {
        Ok(pids) => pids,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "process-tree", "errorCode": error})
        }
    };
    if let Err(error) = enable_profile_privilege() {
        return json!({"status": "unavailable", "stage": "profile-privilege", "errorCode": error, "requiresElevation": true});
    }
    let recovered = match scheduler_sessions::recover_orphans() {
        Ok(count) => count,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "orphan-recovery", "errorCode": error})
        }
    };
    if recovery_only {
        return json!({"status": "recovered", "recoveredSessions": recovered, "captureRequested": false, "evidenceUsable": false});
    }
    if pids.len() > 32 {
        return json!({"status": "unavailable", "stage": "process-tree", "errorCode": windows_sys::Win32::Foundation::ERROR_MORE_DATA});
    }
    let owner = match scheduler_sessions::Owner::for_process(root) {
        Ok(owner) => owner,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "owner-process", "errorCode": error})
        }
    };
    // Group-relative CPU numbers are unambiguous only for a single processor group.
    if unsafe { GetActiveProcessorGroupCount() } != 1 {
        return json!({"status": "unavailable", "stage": "multiple-processor-groups"});
    }
    let records = match host::cpu_set_records() {
        Ok(records) => records,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "cpu-topology", "details": error})
        }
    };
    let classes: BTreeMap<_, _> = records
        .into_iter()
        .filter(|(group, _, _, _)| *group == 0)
        .map(|(_, cpu, _, class)| (u16::from(cpu), class))
        .collect();
    if classes.len() != unsafe { GetActiveProcessorCount(0) } as usize {
        return json!({"status": "unavailable", "stage": "incomplete-cpu-topology"});
    }
    let threads = match scheduler_threads::snapshot(&pids, ui_tid) {
        Ok(threads) => threads,
        Err(error) => {
            return json!({"status": "unavailable", "stage": "thread-snapshot", "errorCode": error})
        }
    };
    let model = SchedulerModel::new(threads.identities.clone(), classes.clone());
    let mut report = scheduler_etw::capture(owner, model);
    report["recoveredSessions"] = json!(recovered);
    report["processorEfficiencyClasses"] = json!(classes);
    report["inaccessibleThreads"] = json!(threads.inaccessible);
    report["threadLimitReached"] = json!(threads.truncated);
    report["rendererMainThreads"] = json!(threads
        .identities
        .iter()
        .filter(|thread| thread.role == "renderer-main")
        .count());
    report["uiThreadTracked"] = json!(threads
        .identities
        .iter()
        .any(|thread| thread.role == "fileterm-ui"));
    report["newThreadsDuringCaptureTracked"] = json!(false);
    report["causality"] = json!("not-established");
    // Thread handles remain alive through capture; release all of them on helper exit.
    drop(threads);
    report
}

#[cfg(test)]
mod tests;
