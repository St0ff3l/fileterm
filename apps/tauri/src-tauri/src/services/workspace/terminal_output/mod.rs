use super::WorkspaceState;
use crate::runtime::StreamChannel;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;

struct OutputBatch {
    tab_id: String,
    chunk: String,
    skipped_bytes_before: usize,
}

const IN_FLIGHT_BATCHES: usize = 4;
const BATCH_BYTES: usize = 16 * 1024;

pub(crate) struct TerminalOutputSubscription {
    window_label: String,
    output: Mutex<OutputQueue>,
    output_ready: Notify,
    closed: CancellationToken,
    channel: StreamChannel<serde_json::Value>,
    credit: Arc<Semaphore>,
    next_delivery_id: AtomicU64,
    pending: Mutex<HashMap<u64, OwnedSemaphorePermit>>,
    last_warning: Mutex<Option<Instant>>,
    /// Last valid renderer acknowledgement; idle output can also make it stale.
    last_ack: Mutex<Option<(Instant, u64)>>,
}

impl TerminalOutputSubscription {
    pub(crate) fn close(&self) {
        self.closed.cancel();
        self.credit.close();
        if let Ok(mut output) = self.output.lock() {
            *output = OutputQueue::default();
        }
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
    }

    fn enqueue(&self, tab_id: &str, chunk: &str) {
        let Ok(mut output) = self.output.lock() else {
            return;
        };
        if self.closed.is_cancelled() {
            return;
        }
        if output.enqueue(tab_id, chunk) {
            drop(output);
            self.output_ready.notify_one();
            return;
        }
        let skipped = output
            .skipped_bytes
            .get(tab_id)
            .copied()
            .unwrap_or_default();
        drop(output);
        if self.should_warn() {
            let since_last_ack = self.last_ack_age_ms().map_or_else(
                || "none".to_owned(),
                |milliseconds| milliseconds.to_string(),
            );
            crate::services::logging::warn_global(
                "terminal:output",
                format!(
                    "window display queue saturated window={} tab={} skipped_display_bytes={} since_last_ack_ms={} progress={}",
                    self.window_label, tab_id, skipped, since_last_ack, self.progress()
                ),
            );
        }
    }

    /// `None` until the renderer acknowledges a delivery for this window.
    fn last_ack_age_ms(&self) -> Option<u128> {
        self.last_ack
            .lock()
            .ok()
            .and_then(|last| last.map(|(acknowledged, _)| acknowledged.elapsed().as_millis()))
    }

    // Try-lock snapshots stay available from native warnings when the page stops reporting.
    fn progress(&self) -> serde_json::Value {
        let pending = self.pending.try_lock().ok();
        let output = self.output.try_lock().ok();
        let last_ack = self.last_ack.try_lock().ok().and_then(|last| *last);
        serde_json::json!({
            "window": self.window_label,
            "issuedBatches": self.next_delivery_id.load(Ordering::Relaxed),
            "lastAcknowledgedDeliveryId": last_ack.map(|(_, id)| id),
            "sinceLastAckMs": last_ack.map(|(at, _)| at.elapsed().as_millis()),
            "inFlight": pending.as_ref().map(|items| items.len()),
            "oldestPendingDeliveryId": pending.as_ref().and_then(|items| items.keys().min().copied()),
            "queuedBatches": output.as_ref().map(|queue| queue.batches.len()),
            "queuedBytes": output.as_ref().map(|queue| queue.batches.iter().map(|b| b.chunk.len()).sum::<usize>()),
        })
    }

    fn should_warn(&self) -> bool {
        self.last_warning.lock().is_ok_and(|mut last| {
            if last.is_some_and(|last| last.elapsed() < Duration::from_secs(30)) {
                return false;
            }
            *last = Some(Instant::now());
            true
        })
    }

    async fn send_next(&self) -> bool {
        let started = Instant::now();
        let Ok(permit) = Arc::clone(&self.credit).acquire_owned().await else {
            return false;
        };
        let message = self
            .output
            .lock()
            .ok()
            .and_then(|mut output| output.pop_next());
        let Some(message) = message else { return false };
        let should_warn = started.elapsed() >= Duration::from_secs(1) && self.should_warn();
        if should_warn {
            crate::services::logging::warn_global("terminal:output", format!(
                "renderer backpressure window={} tab={} wait_ms={} in_flight_limit={IN_FLIGHT_BATCHES} progress={}",
                self.window_label, message.tab_id, started.elapsed().as_millis(), self.progress()
            ));
        }
        let delivery_id = self.next_delivery_id.fetch_add(1, Ordering::Relaxed);
        let Ok(mut pending) = self.pending.lock() else {
            return false;
        };
        if self.credit.is_closed() {
            return false;
        }
        pending.insert(delivery_id, permit);
        drop(pending);
        if self
            .channel
            .send(serde_json::json!({
                "tabId": message.tab_id, "chunk": message.chunk, "deliveryId": delivery_id, "skippedBytesBefore": message.skipped_bytes_before
            }))
            .is_err()
        {
            self.close();
            return false;
        }
        true
    }
}

impl WorkspaceState {
    pub fn forget_terminal_output_tab(&self, tab_id: &str) {
        if let Ok(channels) = self.terminal_output_channels.lock() {
            for subscription in channels.values() {
                if let Ok(mut output) = subscription.output.lock() {
                    output.forget_tab(tab_id);
                }
            }
        }
    }

    pub(super) fn terminal_output_health(&self) -> Option<serde_json::Value> {
        let channels = self.terminal_output_channels.try_lock().ok()?;
        let in_flight = channels.values().try_fold(0, |total, subscription| {
            Some(total + subscription.pending.try_lock().ok()?.len())
        });
        let queued =
            channels
                .values()
                .try_fold((0, 0, 0), |(batches, bytes, tabs), subscription| {
                    let output = subscription.output.try_lock().ok()?;
                    Some((
                        batches + output.batches.len(),
                        bytes + output.batches.iter().map(|b| b.chunk.len()).sum::<usize>(),
                        tabs + output.ready_tabs.len(),
                    ))
                });
        Some(serde_json::json!({
            "windows": channels.len(),
            "windowProgress": channels.values().take(16).map(|subscription| subscription.progress()).collect::<Vec<_>>(),
            "omittedWindows": channels.len().saturating_sub(16),
            "inFlight": in_flight,
            "queuedBatches": queued.map(|value| value.0),
            "queuedBytes": queued.map(|value| value.1),
            "queuedTabs": queued.map(|value| value.2),
        }))
    }

    pub fn register_terminal_output_channel(
        &self,
        window_label: &str,
        channel: StreamChannel<serde_json::Value>,
    ) {
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            // A reload replaces the renderer; its old credit can never be acknowledged.
            channels.retain(|_, subscription| {
                if subscription.window_label == window_label {
                    subscription.close();
                    false
                } else {
                    true
                }
            });
            let subscription = Arc::new(TerminalOutputSubscription {
                window_label: window_label.to_owned(),
                channel,
                output: Mutex::new(OutputQueue::default()),
                output_ready: Notify::new(),
                closed: CancellationToken::new(),
                credit: Arc::new(Semaphore::new(IN_FLIGHT_BATCHES)),
                next_delivery_id: AtomicU64::new(0),
                pending: Mutex::new(HashMap::new()),
                last_warning: Mutex::new(None),
                last_ack: Mutex::new(None),
            });
            channels.insert(subscription.channel.id(), Arc::clone(&subscription));
            crate::runtime::spawn(async move {
                loop {
                    tokio::select! {
                        biased;
                        _ = subscription.closed.cancelled() => break,
                        _ = subscription.output_ready.notified() => {},
                    }
                    while subscription.send_next().await {}
                }
            });
        }
    }

    pub fn ack_terminal_output(&self, window_label: &str, channel_id: u32, delivery_id: u64) {
        if let Ok(channels) = self.terminal_output_channels.lock() {
            if let Some(subscription) = channels
                .get(&channel_id)
                .filter(|s| s.window_label == window_label)
            {
                let acknowledged = subscription
                    .pending
                    .lock()
                    .is_ok_and(|mut pending| pending.remove(&delivery_id).is_some());
                if acknowledged {
                    if let Ok(mut last_ack) = subscription.last_ack.lock() {
                        *last_ack = Some((Instant::now(), delivery_id));
                    }
                }
            }
        }
    }

    pub fn publish_terminal_output(&self, tab_id: &str, chunk: &str) {
        let channels: Vec<_> = match self.terminal_output_channels.lock() {
            Ok(channels) => channels.values().cloned().collect(),
            Err(_) => return,
        };
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let mut end = remaining.len().min(BATCH_BYTES);
            while !remaining.is_char_boundary(end) {
                end -= 1;
            }
            let (batch, tail) = remaining.split_at(end);
            for subscription in &channels {
                subscription.enqueue(tab_id, batch);
            }
            remaining = tail;
        }
        if let Ok(mut channels) = self.terminal_output_channels.lock() {
            channels.retain(|_, subscription| !subscription.credit.is_closed());
        }
    }
}

mod queue;
use queue::OutputQueue;

#[cfg(test)]
use queue::{PENDING_BATCHES, PENDING_BATCHES_PER_TAB};
#[cfg(test)]
mod tests;
