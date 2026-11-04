//! Cache browser identities at window creation; native sampling never waits on the UI.
use super::processes::creation_time;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tauri::{WebviewWindow, WindowEvent, Wry};

#[derive(Clone)]
enum Registration {
    Pending,
    Ready {
        pid: u32,
        created: u64,
    },
    Failed {
        stage: &'static str,
        error: i64,
        pid: Option<u32>,
    },
}
static WINDOWS: Mutex<BTreeMap<String, Arc<Mutex<Registration>>>> = Mutex::new(BTreeMap::new());

fn forget(label: &str, closing: &Arc<Mutex<Registration>>) {
    if let Ok(mut windows) = WINDOWS.lock() {
        if windows
            .get(label)
            .is_some_and(|current| Arc::ptr_eq(current, closing))
        {
            windows.remove(label);
        }
    }
}

pub(crate) fn track(window: &WebviewWindow<Wry>) {
    let label = window.label().to_owned();
    let state = Arc::new(Mutex::new(Registration::Pending));
    let Ok(mut windows) = WINDOWS.lock() else {
        return;
    };
    windows.insert(label.clone(), Arc::clone(&state));
    drop(windows);
    let closing = Arc::clone(&state);
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            forget(&label, &closing);
        }
    });
    let captured = Arc::clone(&state);
    let result = window.with_webview(move |webview| {
        let identity = unsafe {
            webview.controller().CoreWebView2().and_then(|core| {
                let mut pid = 0;
                core.BrowserProcessId(&mut pid)?;
                Ok(pid)
            })
        };
        let registration = match identity {
            Ok(pid) => match creation_time(pid) {
                Ok(created) => Registration::Ready { pid, created },
                Err(error) => Registration::Failed {
                    stage: "browser-identity",
                    pid: Some(pid),
                    error: error.into(),
                },
            },
            Err(error) => Registration::Failed {
                stage: "browser-pid",
                pid: None,
                error: error.code().0.into(),
            },
        };
        if let Ok(mut state) = captured.lock() {
            *state = registration;
        }
    });
    if result.is_err() {
        if let Ok(mut state) = state.lock() {
            *state = Registration::Failed {
                stage: "webview-dispatch",
                pid: None,
                error: -1,
            };
        }
    }
}

/// A PID alone can be recycled after a browser exits; validate its creation time each minute.
pub(super) fn snapshot() -> (Vec<u32>, Value) {
    let Ok(windows) = WINDOWS.try_lock() else {
        return (Vec::new(), json!({"status": "registry-busy"}));
    };
    let registrations: Vec<_> = windows
        .iter()
        .map(|(window, state)| (window.clone(), Arc::clone(state)))
        .collect();
    drop(windows);
    let window_count = registrations.len();
    let mut complete = true;
    let mut roots = Vec::new();
    let mut reports = Vec::new();
    for (window, state) in registrations {
        let registration = state.try_lock().ok().map(|state| state.clone());
        let report = match registration {
            Some(Registration::Ready { pid, created }) => match creation_time(pid) {
                Ok(current) if current == created => {
                    roots.push(pid);
                    json!({"window": window, "status": "available", "browserPid": pid})
                }
                Ok(_) => json!({"window": window, "status": "browser-replaced", "browserPid": pid}),
                Err(error) => {
                    json!({"window": window, "status": "unavailable", "browserPid": pid, "errorCode": error})
                }
            },
            Some(Registration::Failed { stage, error, pid }) => {
                json!({"window": window, "status": "unavailable", "stage": stage, "errorCode": error, "browserPid": pid})
            }
            Some(Registration::Pending) => json!({"window": window, "status": "pending"}),
            None => json!({"window": window, "status": "registry-busy"}),
        };
        complete &= report["status"] == "available";
        if reports.len() < 16 {
            reports.push(report);
        }
    }
    let status = if window_count == 0 {
        "awaiting-webview"
    } else if complete {
        "available"
    } else {
        "partial"
    };
    (
        roots,
        json!({"status": status, "windowCount": window_count, "omittedWindows": window_count.saturating_sub(reports.len()), "windows": reports}),
    )
}

#[cfg(test)]
mod tests;
