use super::is_renderer_stall_warning;

#[test]
fn renderer_stall_warnings_trigger_scheduler_capture() {
    for event in [
        "event-loop-gap",
        "terminal-output-parse",
        "renderer-long-frame",
        "renderer-progress-slow",
    ] {
        assert!(is_renderer_stall_warning(
            "renderer:performance",
            &format!(r#"{{"event":"{event}"}}"#)
        ));
    }
    assert!(!is_renderer_stall_warning(
        "renderer:workspace",
        r#"{"event":"renderer-long-frame"}"#
    ));
    assert!(!is_renderer_stall_warning(
        "renderer:performance",
        r#"{"event":"renderer-health"}"#
    ));
    assert!(!is_renderer_stall_warning(
        "renderer:performance",
        "not json"
    ));
    assert!(!is_renderer_stall_warning(
        "renderer:performance",
        &" ".repeat(16 * 1024 + 1)
    ));
}
