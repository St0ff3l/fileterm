use super::{local_filename_timestamp, sanitize_filename, short_tab_id, timestamp_rfc3339};

#[test]
fn sanitizes_cross_platform_filename_characters() {
    assert_eq!(sanitize_filename("设备:/测试?"), "设备__测试_");
    assert_eq!(sanitize_filename("..."), "session");
    assert!(sanitize_filename(&"x".repeat(100)).len() <= 80);
}

#[test]
fn keeps_a_short_tab_suffix() {
    assert_eq!(short_tab_id("tab-1234567890abcdef"), "1234567890ab");
    assert_eq!(short_tab_id("local"), "local");
}

#[test]
fn formats_local_timestamp_with_an_explicit_offset() {
    let timestamp = timestamp_rfc3339();
    assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    assert!(timestamp.contains('T'));
    assert!(local_filename_timestamp().contains('-'));
}
