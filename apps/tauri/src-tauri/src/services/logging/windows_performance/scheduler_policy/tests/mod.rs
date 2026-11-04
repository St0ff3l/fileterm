use super::*;

#[test]
fn end_of_capture_budget_catches_small_batches_and_unknown_cpu() {
    assert!(!exceeds_cpu_budget(Some(500_000), CONSUMER_CPU_LIMIT_100NS));
    assert!(exceeds_cpu_budget(Some(500_001), CONSUMER_CPU_LIMIT_100NS));
    assert!(exceeds_cpu_budget(None, CONSUMER_CPU_LIMIT_100NS));
    assert!(exceeds_cpu_budget(Some(2_000_001), HELPER_CPU_LIMIT_100NS));
}
