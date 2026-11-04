#[derive(Default)]
pub(crate) struct CopyIoGates(
    tokio::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<tokio::sync::Mutex<()>>>>,
);

impl CopyIoGates {
    async fn acquire(
        &self,
        source: &str,
        target: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<tokio::sync::OwnedMutexGuard<()>>, AppError> {
        let mut keys = vec![source, target];
        keys.sort_unstable();
        keys.dedup();
        let gates = {
            let mut slots = self.0.lock().await;
            slots.retain(|_, gate| gate.strong_count() > 0);
            keys.into_iter()
                .map(|key| {
                    let gate = slots
                        .get(key)
                        .and_then(std::sync::Weak::upgrade)
                        .unwrap_or_else(|| std::sync::Arc::new(tokio::sync::Mutex::new(())));
                    slots.insert(key.to_string(), std::sync::Arc::downgrade(&gate));
                    gate
                })
                .collect::<Vec<_>>()
        };
        let mut leases = Vec::new();
        for gate in gates {
            leases.push(tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(transfer_error("传输已取消")),
                lease = gate.lock_owned() => lease,
            });
        }
        Ok(leases)
    }
}

#[cfg(test)]
#[path = "tests/copy_queue_tests.rs"]
mod copy_queue_tests;
