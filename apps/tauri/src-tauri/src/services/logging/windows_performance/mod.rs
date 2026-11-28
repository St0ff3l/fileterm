//! Low-frequency native evidence that continues while the WebView is stalled.
#[cfg(target_os = "windows")]
mod host;
mod process_tree;
#[cfg(target_os = "windows")]
mod processes;
#[cfg(target_os = "windows")]
mod scheduler;
#[cfg(target_os = "windows")]
mod scheduler_etw;
mod scheduler_model;
mod scheduler_policy;
#[cfg(target_os = "windows")]
mod scheduler_sessions;
#[cfg(target_os = "windows")]
mod scheduler_threads;
#[cfg(all(
    feature = "desktop",
    target_os = "windows",
    not(feature = "windows-scheduler-fixture")
))]
pub(crate) mod webview_processes;
#[cfg(all(
    target_os = "windows",
    any(not(feature = "desktop"), feature = "windows-scheduler-fixture")
))]
pub(crate) mod webview_processes {
    pub(crate) fn track<T>(_: &T) {}

    pub(super) fn snapshot() -> (Vec<u32>, serde_json::Value) {
        (
            Vec::new(),
            serde_json::json!({"status": "fixture-disabled"}),
        )
    }
}

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
pub(super) fn observe(scope: &str, message: &str) {
    scheduler::observe(scope, message);
}
#[cfg(target_os = "windows")]
pub(super) fn run_helper() {
    scheduler::run_helper();
}

#[cfg(target_os = "windows")]
pub(super) fn start(version: String) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    scheduler::start(
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
        version.clone(),
    );
    if let Err(error) = std::thread::Builder::new()
        .name("windows-performance".into())
        .spawn(move || {
            let host = host::snapshot();
            let started = Instant::now();
            let mut sampler = processes::Sampler::default();
            let mut previous_sample = None;
            loop {
                let sample_started = Instant::now();
                let interval_ms =
                    previous_sample.map(|at| sample_started.duration_since(at).as_millis());
                previous_sample = Some(sample_started);
                let processes = sampler.sample();
                super::info_global(
                    "app:windows-performance",
                    serde_json::json!({
                        "event": "windows-native-health",
                        "version": version,
                        "pid": std::process::id(),
                        "uptimeMs": started.elapsed().as_millis(),
                        "sampleIntervalMs": interval_ms,
                        "appForeground": host::app_foreground(),
                        "host": host,
                        "power": host::power_status(),
                        "processes": processes,
                        "scheduler": scheduler::status(),
                        "sampleDurationMs": sample_started.elapsed().as_millis(),
                    })
                    .to_string(),
                );
                std::thread::sleep(Duration::from_secs(60));
            }
        })
    {
        super::warn_global(
            "app:windows-performance",
            format!("sampler start failed: {error}"),
        );
    }
}
