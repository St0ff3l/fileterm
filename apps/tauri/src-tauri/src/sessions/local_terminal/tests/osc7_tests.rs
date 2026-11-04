use super::{LocalOsc7CwdTracker, LOCAL_OSC7_BUFFER_LIMIT, LOCAL_OSC7_MARKER};

#[test]
fn oversized_unterminated_osc7_is_discarded_and_tracking_recovers() {
    let mut tracker = LocalOsc7CwdTracker::default();
    assert_eq!(
        tracker.observe(&format!(
            "{LOCAL_OSC7_MARKER}{}",
            "x".repeat(LOCAL_OSC7_BUFFER_LIMIT)
        )),
        None
    );
    assert!(tracker.buffer.len() <= 3);
    assert_eq!(
        tracker.observe("\x1b]7;file://host/recovered\x07"),
        Some("/recovered".to_string())
    );
}

#[test]
fn aborted_osc7_does_not_hide_a_later_directory() {
    for cancel in ["\x18", "\x1a", "\x1b"] {
        let mut tracker = LocalOsc7CwdTracker::default();
        let input = format!("\x1b]7;file://host/stale{cancel}\x1b]7;file://host/current\x07");
        assert_eq!(tracker.observe(&input), Some("/current".to_string()));
    }
}
