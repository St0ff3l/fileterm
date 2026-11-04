use super::WorkspaceState;
use serde_json::{json, Value};

/// Enrich the existing minute report without another IPC request or a waiting task.
pub fn augment_renderer_health(state: &WorkspaceState, message: &str) -> Option<String> {
    if message.len() > 16 * 1024 {
        return None;
    }
    let mut report: Value = serde_json::from_str(message).ok()?;
    if report.get("event")?.as_str()? != "renderer-health" {
        return None;
    }
    let runtime = tauri::async_runtime::handle().inner().metrics();
    let workers = state.workers.try_read().ok();
    report["native"] = json!({
        "tokioAliveTasks": runtime.num_alive_tasks(),
        "tokioGlobalQueueDepth": runtime.global_queue_depth(),
        "sessions": state.sessions.try_read().ok().map(|items| items.len()),
        "workers": workers.as_ref().map(|items| items.len()),
        "workerQueuedCommands": workers.as_ref().map(|items| items.values()
            .map(|sender| sender.max_capacity() - sender.capacity()).sum::<usize>()),
        "sessionLogWriters": state.session_log_writers.try_read().ok().map(|items| items.len()),
        "transferTasks": state.transfers.try_read().ok().map(|items| items.len()),
        "transferRuns": state.transfer_runs.try_read().ok().map(|items| items.len()),
        "transferProgressSamples": state.transfer_progress_samples.try_lock().ok().map(|items| items.len()),
        "terminalOutput": state.terminal_output_health(),
    });
    serde_json::to_string(&report).ok()
}

#[cfg(test)]
mod tests;
