use super::Manager;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc,
};
use std::thread::{self, ThreadId};
use std::time::Duration;

struct NativeGuardProbe {
    owner: ThreadId,
    released: mpsc::Sender<(ThreadId, ThreadId)>,
}

impl Drop for NativeGuardProbe {
    fn drop(&mut self) {
        let _ = self.released.send((self.owner, thread::current().id()));
    }
}

#[tokio::test]
async fn parallel_runs_release_only_after_last_run_on_the_owner_thread() {
    let manager = Manager::default();
    let created = Arc::new(AtomicUsize::new(0));
    let (released_tx, released_rx) = mpsc::channel();
    for _ in 0..2 {
        let created = Arc::clone(&created);
        let released = released_tx.clone();
        let first = manager
            .acquire_with(move || {
                created.fetch_add(1, Ordering::SeqCst);
                NativeGuardProbe {
                    owner: thread::current().id(),
                    released,
                }
            })
            .unwrap();
        let second = manager
            .acquire_with(|| -> NativeGuardProbe { panic!("must reuse the existing guard") })
            .unwrap();
        first.wait_ready(Duration::from_secs(3)).await;
        second.wait_ready(Duration::from_secs(3)).await;
        drop(first);
        assert!(matches!(
            released_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        drop(second);
        let (owner, released_on) = released_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(owner, released_on);
    }
    assert_eq!(created.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn unwinding_a_run_releases_its_claim() {
    let manager = Manager::default();
    let (released, receiver) = mpsc::channel();
    let lease = manager
        .acquire_with(move || NativeGuardProbe {
            owner: thread::current().id(),
            released,
        })
        .unwrap();
    lease.wait_ready(Duration::from_secs(3)).await;
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _lease = lease;
            panic!("simulated transfer failure");
        }))
        .is_err()
    );
    let (owner, released_on) = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(owner, released_on);
}

#[tokio::test]
async fn unavailable_backend_does_not_block_runs_and_can_retry_next_batch() {
    let manager = Manager::default();
    let unavailable = manager.acquire_with(|| None::<NativeGuardProbe>).unwrap();
    unavailable.wait_ready(Duration::from_secs(3)).await;
    drop(unavailable);
    let (released, receiver) = mpsc::channel();
    let next = manager
        .acquire_with(move || {
            Some(NativeGuardProbe {
                owner: thread::current().id(),
                released,
            })
        })
        .unwrap();
    next.wait_ready(Duration::from_secs(3)).await;
    drop(next);
    receiver.recv_timeout(Duration::from_secs(3)).unwrap();
}

#[tokio::test]
async fn cancellation_during_native_creation_does_not_orphan_the_guard() {
    let manager = Manager::default();
    let (proceed, waiting) = mpsc::channel();
    let (released, receiver) = mpsc::channel();
    let lease = manager
        .acquire_with(move || {
            waiting.recv().unwrap();
            NativeGuardProbe {
                owner: thread::current().id(),
                released,
            }
        })
        .unwrap();
    // Simulates dropping the acquisition future when the run is canceled.
    drop(lease);
    proceed.send(()).unwrap();
    let (owner, released_on) = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(owner, released_on);
}

#[tokio::test]
async fn slow_native_creation_times_out_and_releases_when_it_finishes() {
    let manager = Manager::default();
    let (proceed, waiting) = mpsc::channel();
    let (released, receiver) = mpsc::channel();
    let lease = manager
        .acquire_with(move || {
            waiting.recv().unwrap();
            NativeGuardProbe {
                owner: thread::current().id(),
                released,
            }
        })
        .unwrap();
    assert!(!lease.wait_ready(Duration::from_millis(10)).await);
    drop(lease);
    proceed.send(()).unwrap();
    let (owner, released_on) = receiver.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(owner, released_on);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_native_request_keeps_system_awake_and_releases_without_holding_display() {
    const CONTINUOUS: u32 = 0x8000_0000;
    const SYSTEM_REQUIRED: u32 = 1;
    const DISPLAY_REQUIRED: u32 = 2;
    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "SetThreadExecutionState"]
        fn set_thread_execution_state(flags: u32) -> u32;
    }
    thread::spawn(|| {
        let guard = keepawake::Builder::default().idle(true).create().unwrap();
        // Setting the same flags returns the thread's existing request.
        let held = unsafe { set_thread_execution_state(CONTINUOUS | SYSTEM_REQUIRED) };
        assert_ne!(held & SYSTEM_REQUIRED, 0);
        assert_eq!(held & DISPLAY_REQUIRED, 0);
        drop(guard);
        let released = unsafe { set_thread_execution_state(CONTINUOUS) };
        assert_ne!(released, 0);
        assert_eq!(released & (SYSTEM_REQUIRED | DISPLAY_REQUIRED), 0);
    })
    .join()
    .unwrap();
}
