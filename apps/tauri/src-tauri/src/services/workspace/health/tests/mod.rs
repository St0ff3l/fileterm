use super::*;

#[test]
fn only_minute_health_reports_collect_runtime_counts() {
    let state = WorkspaceState::default();
    assert!(augment_renderer_health(&state, "not json").is_none());
    assert!(augment_renderer_health(&state, r#"{"event":"event-loop-gap"}"#).is_none());
    assert!(augment_renderer_health(&state, &" ".repeat(16385)).is_none());
    let _busy = state.transfers.try_write().unwrap();
    let report =
        augment_renderer_health(&state, r#"{"event":"renderer-health","uptimeMs":60000}"#).unwrap();
    let report: Value = serde_json::from_str(&report).unwrap();
    assert_eq!(report["uptimeMs"], 60000);
    assert_eq!(report["native"]["sessions"], 0);
    assert_eq!(report["native"]["workerQueuedCommands"], 0);
    assert!(
        report["native"]["transferTasks"].is_null(),
        "busy locks must not wait or report zero"
    );
    assert!(report["native"]["tokioAliveTasks"].is_number());
}
