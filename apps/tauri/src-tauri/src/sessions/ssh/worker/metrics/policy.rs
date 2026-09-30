#[derive(Clone, Copy)]
struct MetricsPolicy {
    interval: u64,
    pause: Duration,
    rebuild: Duration,
    first_sample: Duration,
}

impl MetricsPolicy {
    fn new(interval: u64) -> Self {
        Self {
            interval,
            pause: Duration::from_secs((3 * interval).max(15)),
            rebuild: Duration::from_secs((5 * interval).max(60)),
            first_sample: Duration::from_secs((3 * interval).max(30)),
        }
    }

    fn deadline(
        self,
        started: tokio::time::Instant,
        last: Option<tokio::time::Instant>,
        paused: bool,
    ) -> tokio::time::Instant {
        match last {
            None => started + self.first_sample,
            Some(last) => last + if paused { self.rebuild } else { self.pause },
        }
    }

    fn retry_delay(attempt: u8) -> Option<Duration> {
        [2, 5, 10, 20, 30]
            .get(usize::from(attempt))
            .map(|s| Duration::from_secs(*s))
    }
}

#[derive(Clone, Default)]
struct MetricsHealth {
    last_sample: Option<tokio::time::Instant>,
    healthy_since: Option<tokio::time::Instant>,
    healthy_samples: u64,
}

impl MetricsHealth {
    fn interrupt(&mut self) {
        self.healthy_since = None;
        self.healthy_samples = 0;
    }

    fn sample(&mut self, now: tokio::time::Instant) -> bool {
        self.last_sample = Some(now);
        let since = *self.healthy_since.get_or_insert(now);
        self.healthy_samples += 1;
        self.healthy_samples >= 3 && now.duration_since(since) >= Duration::from_secs(60)
    }
}

/// The parser supplies zero defaults for absent fields; validate the raw block
/// first so shell errors or marker-only output cannot masquerade as healthy.
fn metrics_block_has_sample(block: &str) -> bool {
    block.split(['\r', '\n']).any(|line| {
        let number = line.trim().strip_prefix("__CPU__").or_else(|| {
            line.trim()
                .strip_prefix("__MEM__")
                .and_then(|value| value.split('|').nth(2))
        });
        number
            .and_then(|s| s.trim().parse::<f64>().ok())
            .is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n))
    })
}

async fn wait_for_metrics_retry(
    cancellation: &CancellationToken,
    retry_rx: &mut mpsc::Receiver<()>,
    delay: Option<Duration>,
) -> bool {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        request = retry_rx.recv() => request.is_some(),
        _ = async {
            match delay {
                Some(delay) => tokio::time::sleep(delay).await,
                None => std::future::pending::<()>().await,
            }
        } => true,
    }
}
