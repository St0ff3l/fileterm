use super::{redact, sanitize_fragment};

#[test]
fn sanitization_preserves_unicode_and_control_escaping_at_byte_limits() {
    assert_eq!(
        sanitize_fragment("中🔒\n\r\t\0\x1b\u{85}", 64),
        "中🔒\\n\\r\\t\\u{0000}\\u{001b}\\u{0085}"
    );
    for (input, limit, expected) in [
        ("中🔒", 7, "中🔒"),
        ("中🔒", 6, "中…[truncated]"),
        ("\n", 2, "\\n"),
        ("\n", 1, "…[truncated]"),
        ("\0", 8, "\\u{0000}"),
        ("\0", 7, "…[truncated]"),
        ("a", 0, "…[truncated]"),
        ("", 0, ""),
    ] {
        assert_eq!(sanitize_fragment(input, limit), expected);
    }
}

#[test]
fn strips_common_secret_labels() {
    let line = redact(
        r##"password=hunter2 Authorization: Bearer very-secret Authorization=Basic encoded-secret proxyPassword:abc "passphrase":"private" token='opaque'"##,
    );
    assert!(!line.contains("hunter2"));
    assert!(!line.contains("BearerSecret"));
    assert!(!line.contains("very-secret"));
    assert!(!line.contains("encoded-secret"));
    assert!(!line.contains("abc"));
    assert!(!line.contains("private"));
    assert!(!line.contains("opaque"));
}

#[test]
fn preserves_non_secret_diagnostics() {
    let line = redact("session=tab-1 platform=windows cpu=12%");
    assert_eq!(line, "session=tab-1 platform=windows cpu=12%");
}

#[test]
fn redacts_otp_secret_field_names() {
    for name in ["otpSecret", "otp_secret", "otp-secret"] {
        assert_eq!(
            redact(&format!("{name}=GEZDGNBVGY3TQOJQ")),
            format!("{name}=[REDACTED]")
        );
    }
}

#[test]
fn removes_control_characters_and_redacts_quoted_values() {
    let line = redact("error=first\nsecond password=\"otp with spaces\" otp='123456'");
    assert_eq!(
        line,
        "error=first\\nsecond password=[REDACTED] otp=[REDACTED]"
    );
}

#[test]
fn truncates_unbounded_remote_diagnostics() {
    let line = redact(&"x".repeat(super::MAX_LOG_MESSAGE_BYTES + 128));
    assert!(line.len() <= super::MAX_LOG_MESSAGE_BYTES + "…[truncated]".len());
    assert!(line.ends_with("…[truncated]"));
}
