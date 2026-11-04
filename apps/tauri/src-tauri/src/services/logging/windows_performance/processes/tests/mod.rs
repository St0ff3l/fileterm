use super::*;

#[test]
fn cpu_delta_handles_first_sample_pid_reuse_and_single_core_percentage() {
    let now = Instant::now();
    let previous = (42, 10_000_000, now - std::time::Duration::from_secs(60));
    assert_eq!(cpu_percent(None, 42, 20_000_000, now), None);
    assert_eq!(cpu_percent(Some(&previous), 43, 20_000_000, now), None);
    assert_eq!(cpu_percent(Some(&previous), 42, 1, now), None);
    assert_eq!(
        cpu_percent(Some(&previous), 42, 610_000_000, now),
        Some(100.0)
    );
}

#[test]
fn inaccessible_process_memory_is_not_reported_as_zero() {
    let report = Sampler::default().process(u32::MAX);
    assert!(report.get("openErrorCode").is_some());
    assert!(report.get("privateBytes").is_none());
    assert!(report.get("workingSetBytes").is_none());
}

#[test]
fn native_sampler_reads_current_process_and_releases_handles() {
    let mut sampler = Sampler::default();
    let first = sampler.process(std::process::id());
    assert!(first["workingSetBytes"].as_u64().unwrap() > 0, "{first}");
    assert!(first["cpuPercentOneCore"].is_null());
    let second = sampler.process(std::process::id());
    assert!(second["cpuPercentOneCore"].is_number(), "{second}");
    assert!(sampler.sample()["count"].as_u64().unwrap() >= 1);
}
