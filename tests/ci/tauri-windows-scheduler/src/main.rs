//! Exercise production scheduler code without loading Tauri's desktop DLLs.
#[cfg(any(target_os = "windows", test))]
#[allow(dead_code)]
#[path = "../../../../apps/tauri/src-tauri/src/services/logging/windows_performance/mod.rs"]
mod windows_performance;

#[cfg(target_os = "windows")]
fn info_global(_: &str, _: impl AsRef<str>) {}
#[cfg(target_os = "windows")]
fn warn_global(_: &str, _: impl AsRef<str>) {}

fn main() {
    #[cfg(target_os = "windows")]
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--fileterm-scheduler-diagnostic")
    {
        windows_performance::run_helper();
    }
}
