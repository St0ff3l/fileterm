/// russh PTY/shell requests enqueue messages; their futures do not wait for
/// server acknowledgement. Bound those sends as well as channel creation.
async fn wait_for_shell_startup_step<T, E: std::fmt::Display>(
    stage: &str,
    deadline: Duration,
    cancellation: &CancellationToken,
    operation: impl std::future::Future<Output = Result<T, E>>,
) -> Result<T, String> {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => Err("SSH connection canceled".to_string()),
        result = timeout(deadline, operation) => match result {
            Ok(result) => result.map_err(|error| format!("Shell startup {stage} failed: {error}")),
            Err(_) => Err(format!("Shell startup {stage} timed out")),
        },
    }
}

/// The terminal owns the session lifetime. Slow auxiliary initialization must
/// neither delay IO nor outlive a closed/cancelled terminal worker.
async fn run_with_auxiliary_startup<T>(
    terminal: impl std::future::Future<Output = T>,
    auxiliary: impl std::future::Future<Output = ()>,
) -> T {
    tokio::select! {
        result = terminal => result,
        _ = async {
            auxiliary.await;
            std::future::pending::<()>().await;
        } => unreachable!("completed auxiliary initialization remains pending"),
    }
}

#[cfg(test)]
#[path = "tests/startup_lifecycle_tests.rs"]
mod startup_lifecycle_tests;
