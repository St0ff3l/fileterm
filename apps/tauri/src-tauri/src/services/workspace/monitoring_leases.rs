/// One metrics channel per captured connection target. Profile edits cannot
/// make a new target follow the collector of an existing SSH session.
pub struct MonitoringLeases {
    holders: StdMutex<HashMap<String, String>>,
    changed: watch::Sender<u64>,
}

impl Default for MonitoringLeases {
    fn default() -> Self {
        Self {
            holders: StdMutex::new(HashMap::new()),
            changed: watch::channel(0).0,
        }
    }
}

/// Held by the session whose collector owns the channel. Dropping it (stop,
/// failure, disconnect or panic) wakes the followers so one can take over.
pub struct MonitoringLease {
    leases: Arc<MonitoringLeases>,
    key: String,
    tab_id: String,
}

impl MonitoringLeases {
    /// Returns the lease, or the tab that already holds it. A reconnect in the
    /// same tab waits too, until the previous collector has released it.
    pub fn try_acquire(
        self: &Arc<Self>,
        key: &str,
        tab_id: &str,
    ) -> Result<MonitoringLease, String> {
        let mut holders = self.holders.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(holder) = holders.get(key) {
            return Err(holder.clone());
        }
        holders.insert(key.to_string(), tab_id.to_string());
        Ok(MonitoringLease {
            leases: self.clone(),
            key: key.to_string(),
            tab_id: tab_id.to_string(),
        })
    }

    /// Changes whenever a lease is released.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

impl Drop for MonitoringLease {
    fn drop(&mut self) {
        let mut holders = self
            .leases
            .holders
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if holders.get(&self.key) == Some(&self.tab_id) {
            holders.remove(&self.key);
        }
        drop(holders);
        self.leases.changed.send_modify(|version| *version += 1);
    }
}

#[cfg(test)]
#[path = "tests/monitoring_lease_tests.rs"]
mod monitoring_lease_tests;
