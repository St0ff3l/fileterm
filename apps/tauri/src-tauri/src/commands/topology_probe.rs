use crate::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio::time::timeout;

const MAX_TOPOLOGY_PROBE_TARGETS: usize = 64;
const TOPOLOGY_PROBE_CONCURRENCY: usize = 8;
const TOPOLOGY_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyTcpProbeTarget {
    id: String,
    host: String,
    port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyTcpProbeResult {
    id: String,
    status: &'static str,
    latency_ms: Option<u64>,
    checked_at: u64,
}

fn valid_probe_target(target: &TopologyTcpProbeTarget) -> bool {
    !target.id.trim().is_empty()
        && target.id.len() <= 256
        && !target.host.trim().is_empty()
        && target.host.len() <= 253
        && !target
            .host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        && target.port > 0
}

async fn probe_topology_target(target: TopologyTcpProbeTarget, checked_at: u64) -> TopologyTcpProbeResult {
    let started = Instant::now();
    let result = timeout(
        TOPOLOGY_PROBE_TIMEOUT,
        TcpStream::connect((target.host.as_str(), target.port)),
    )
    .await;
    let latency_ms = result
        .ok()
        .and_then(Result::ok)
        .map(|stream| {
            drop(stream);
            started.elapsed().as_millis().min(u64::MAX as u128) as u64
        });

    TopologyTcpProbeResult {
        id: target.id,
        status: if latency_ms.is_some() {
            "reachable"
        } else {
            "unreachable"
        },
        latency_ms,
        checked_at,
    }
}

#[tauri::command]
pub async fn app_probe_topology_services(
    targets: Vec<TopologyTcpProbeTarget>,
) -> Result<Vec<TopologyTcpProbeResult>, AppError> {
    if targets.len() > MAX_TOPOLOGY_PROBE_TARGETS {
        return Err(AppError::Command(format!(
            "topology service probe limit is {MAX_TOPOLOGY_PROBE_TARGETS}"
        )));
    }

    let mut ids = HashSet::with_capacity(targets.len());
    if targets
        .iter()
        .any(|target| !valid_probe_target(target) || !ids.insert(target.id.clone()))
    {
        return Err(AppError::Command(
            "invalid or duplicate topology service probe target".to_string(),
        ));
    }

    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let mut pending = targets.into_iter().enumerate();
    let mut tasks = JoinSet::new();
    let mut results: Vec<Option<TopologyTcpProbeResult>> =
        std::iter::repeat_with(|| None).take(pending.len()).collect();

    for _ in 0..TOPOLOGY_PROBE_CONCURRENCY {
        if let Some((index, target)) = pending.next() {
            tasks.spawn(async move { (index, probe_topology_target(target, checked_at).await) });
        }
    }

    while let Some(joined) = tasks.join_next().await {
        let (index, result) = joined.map_err(|error| AppError::Command(error.to_string()))?;
        results[index] = Some(result);
        if let Some((index, target)) = pending.next() {
            tasks.spawn(async move { (index, probe_topology_target(target, checked_at).await) });
        }
    }

    Ok(results.into_iter().flatten().collect())
}
