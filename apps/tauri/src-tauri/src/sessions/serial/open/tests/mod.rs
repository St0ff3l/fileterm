use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::oneshot;

struct Opened(Arc<AtomicUsize>);
impl Drop for Opened {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn timed_out_and_canceled_opens_release_late_handles_once() {
    for cancel in [false, true] {
        let released = Arc::new(AtomicUsize::new(0));
        let probe = released.clone();
        let cancellation = CancellationToken::new();
        let token = cancellation.clone();
        let (started, ready) = oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            wait_for_open(
                move || {
                    started.send(()).unwrap();
                    blocked.recv().unwrap();
                    Ok(Opened(probe))
                },
                &token,
                if cancel {
                    Duration::from_secs(10)
                } else {
                    Duration::from_millis(50)
                },
            )
            .await
        });
        ready.await.unwrap();
        if cancel {
            cancellation.cancel();
        }
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result
            .err()
            .unwrap()
            .contains(if cancel { "取消" } else { "超时" }));
        assert_eq!(released.load(Ordering::SeqCst), 0);
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while released.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(released.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn an_already_canceled_open_does_not_touch_the_device() {
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(wait_for_open(
        || -> Result<(), String> { panic!("canceled open ran") },
        &cancellation,
        Duration::from_secs(1)
    )
    .await
    .unwrap_err()
    .contains("取消"));
}
