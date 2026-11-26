static SSH_ROUTE_DIAL_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
const ROUTE_HISTORY_TTL: Duration = Duration::from_secs(15 * 60);
const ROUTE_HISTORY_LIMIT: usize = 1024;
type SshRouteHealthKey = (SshRouteTarget, SshRouteCandidate);
static SSH_ROUTE_HEALTH: LazyLock<StdMutex<HashMap<SshRouteHealthKey, SshRouteHealth>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

struct SshRouteHealth {
    samples: u16,
    success_rate: f64,
    latency_ms: Option<f64>,
    variation_ms: f64,
    failures: u8,
    updated: Instant,
    authenticated: Option<Instant>,
}

impl SshRouteHealth {
    fn new(now: Instant) -> Self {
        Self {
            samples: 0,
            success_rate: 0.5,
            latency_ms: None,
            variation_ms: 0.0,
            failures: 0,
            updated: now,
            authenticated: None,
        }
    }

    fn observe(&mut self, now: Instant, elapsed: Duration, success: bool) {
        self.updated = now;
        self.samples = self.samples.saturating_add(1);
        self.success_rate = self.success_rate * 0.75 + if success { 0.25 } else { 0.0 };
        if success {
            self.failures = 0;
            let sample = elapsed.as_secs_f64() * 1000.0;
            let average = self.latency_ms.unwrap_or(sample);
            self.latency_ms = Some(average * 0.75 + sample * 0.25);
            self.variation_ms = self.variation_ms * 0.75 + (sample - average).abs() * 0.25;
        } else {
            self.failures = self.failures.saturating_add(1);
        }
    }

    fn cooling(&self, now: Instant) -> bool {
        self.failures > 0
            && now.duration_since(self.updated)
                < Duration::from_secs(5 * 2_u64.pow(u32::from(self.failures.min(4) - 1)))
    }

    fn score(&self, now: Instant) -> f64 {
        let recent_auth = self
            .authenticated
            .is_some_and(|time| now.duration_since(time) < Duration::from_secs(300));
        (1.0 - self.success_rate) * 1200.0
            + self.latency_ms.unwrap_or(250.0).min(1500.0)
            + self.variation_ms.min(500.0)
            - if recent_auth { 150.0 } else { 0.0 }
    }
}

fn with_ssh_route_health(
    target: &SshRouteTarget,
    route: &SshRouteCandidate,
    update: impl FnOnce(&mut SshRouteHealth, Instant),
) {
    let now = Instant::now();
    let key = (target.clone(), route.clone());
    let mut health = SSH_ROUTE_HEALTH.lock().expect("SSH route health lock");
    health.retain(|_, entry| now.saturating_duration_since(entry.updated) < ROUTE_HISTORY_TTL);
    if health.len() >= ROUTE_HISTORY_LIMIT && !health.contains_key(&key) {
        if let Some(oldest) = health
            .iter()
            .min_by_key(|(_, entry)| entry.updated)
            .map(|(key, _)| key.clone())
        {
            health.remove(&oldest);
        }
    }
    update(
        health
            .entry(key)
            .or_insert_with(|| SshRouteHealth::new(now)),
        now,
    );
}

fn observe_ssh_route(
    target: &SshRouteTarget,
    route: &SshRouteCandidate,
    elapsed: Duration,
    failure: Option<SshRouteFailure>,
) {
    with_ssh_route_health(target, route, |entry, now| {
        entry.observe(now, elapsed, failure.is_none())
    });
}

fn record_authenticated_ssh_route(target: SshRouteTarget, route: SshRouteCandidate) {
    with_ssh_route_health(&target, &route, |entry, now| {
        entry.authenticated = Some(now)
    });
}

fn schedule_ssh_routes(
    target: &SshRouteTarget,
    routes: &[SshRouteCandidate],
    budget: Duration,
) -> Vec<(SshRouteCandidate, Duration)> {
    let now = Instant::now();
    let health = SSH_ROUTE_HEALTH.lock().expect("SSH route health lock");
    let ranked: Vec<_> = routes
        .iter()
        .map(|route| {
            let entry = health
                .get(&(target.clone(), route.clone()))
                .filter(|entry| now.saturating_duration_since(entry.updated) < ROUTE_HISTORY_TTL);
            (
                route,
                entry.is_some_and(|entry| entry.cooling(now)),
                entry.map_or(850.0, |entry| entry.score(now)),
            )
        })
        .collect();
    let best = ranked
        .iter()
        .filter(|(_, cooling, _)| !cooling)
        .map(|(_, _, score)| *score)
        .reduce(f64::min);
    ranked
        .into_iter()
        .map(|(route, cooling, score)| {
            let delay = match best {
                None => 0,
                Some(_) if cooling => 1000,
                Some(best) => ((score - best) as u64).min(450),
            };
            (route.clone(), Duration::from_millis(delay).min(budget / 2))
        })
        .collect()
}
