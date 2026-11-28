//! Shared idle-sleep inhibition for running file and directory transfers.
//!
//! The native guard stays on one dedicated thread: Windows execution-state
//! requests must be cleared on the same thread that created them. Each run
//! owns a lease; the last drop disconnects the thread and releases the guard.

use std::sync::{mpsc, Arc, Mutex, Weak};
use std::time::Duration;

use crate::runtime::AppHandle;
use tokio::sync::watch;

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
#[derive(Default)]
struct Manager {
    current: Mutex<Weak<Lease>>,
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub(super) struct Lease {
    _release: mpsc::Sender<()>,
    ready: watch::Receiver<bool>,
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
impl Lease {
    async fn wait_ready(&self, limit: Duration) -> bool {
        let mut ready = self.ready.clone();
        tokio::time::timeout(limit, ready.wait_for(|ready| *ready))
            .await
            .is_ok_and(|result| result.is_ok())
    }
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
impl Manager {
    fn acquire_with<F, G>(&self, create: F) -> std::io::Result<Arc<Lease>>
    where
        F: FnOnce() -> G + Send + 'static,
        G: 'static,
    {
        let mut current = self
            .current
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(lease) = current.upgrade() {
            return Ok(lease);
        }

        let (release, released) = mpsc::channel();
        let (ready_tx, ready) = watch::channel(false);
        std::thread::Builder::new()
            .name("fileterm-transfer-power".to_string())
            .spawn(move || {
                let guard = create();
                let _ = ready_tx.send(true);
                // No messages are sent. Disconnect means every run has ended.
                let _ = released.recv();
                drop(guard);
            })?;
        let lease = Arc::new(Lease {
            _release: release,
            ready,
        });
        *current = Arc::downgrade(&lease);
        Ok(lease)
    }
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
static MANAGER: Manager = Manager {
    current: Mutex::new(Weak::new()),
};

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
struct NativeGuard {
    #[cfg(feature = "desktop")]
    native: Option<keepawake::KeepAwake>,
    app: AppHandle,
}

impl Drop for NativeGuard {
    fn drop(&mut self) {
        #[cfg(feature = "desktop")]
        if let Some(native) = self.native.take() {
            drop(native);
            crate::services::logging::info(
                &self.app,
                "transfer:power",
                "idle sleep inhibition released; all transfer leases ended",
            );
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = &self.app;
        }
    }
}

pub(super) async fn acquire(app: AppHandle) -> Option<Arc<Lease>> {
    if app.is_web() {
        return None;
    }
    #[cfg(not(feature = "desktop"))]
    {
        return None;
    }
    #[cfg(feature = "desktop")]
    {
        let power_app = app.clone();
        let lease = match MANAGER.acquire_with(move || {
            let native = match keepawake::Builder::default()
                .idle(true)
                .display(false)
                .sleep(false)
                .app_name("FileTerm")
                .app_reverse_domain("com.fileterm.desktop")
                .reason("FileTerm file transfer in progress")
                .create()
            {
                Ok(guard) => {
                    crate::services::logging::info(
                        &power_app,
                        "transfer:power",
                        "idle sleep inhibition acquired; display sleep allowed",
                    );
                    Some(guard)
                }
                Err(error) => {
                    crate::services::logging::warn(
                        &power_app,
                        "transfer:power",
                        format!("unable to inhibit idle sleep; transfer continues: {error}"),
                    );
                    None
                }
            };
            NativeGuard {
                native,
                app: power_app,
            }
        }) {
            Ok(lease) => lease,
            Err(error) => {
                crate::services::logging::warn(
                    &app,
                    "transfer:power",
                    format!("unable to start power owner thread; transfer continues: {error}"),
                );
                return None;
            }
        };
        if !lease.wait_ready(Duration::from_secs(3)).await {
            crate::services::logging::warn(
                &app,
                "transfer:power",
                "idle sleep inhibition unavailable or initialization timed out; transfer continues",
            );
            return None;
        }
        Some(lease)
    }
}

#[cfg(test)]
mod tests;
