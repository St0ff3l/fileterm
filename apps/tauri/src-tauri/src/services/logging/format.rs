use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{LogCategory, LogLevel};

const MAX_LOG_MESSAGE_BYTES: usize = 16 * 1024;

static AUTHORIZATION_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?i)(authorization["']?\s*[:=]\s*["']?(?:bearer|basic)\s+)[^\s,;"'}]+"#)
        .expect("static authorization redaction regex")
});
static SECRET_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r##"(?i)(password|passphrase|authorization|proxy[_-]?password|otp(?:[_-]?secret)?|mfa|one[-_ ]?time[-_ ]?password|verification[-_ ]?code|private[-_ ]?key|secret|api[-_ ]?key|token)["']?\s*([:=])\s*(?:"[^"]*"|'[^']*'|[^\s,;"'}\]]+)"##,
    )
        .expect("static redaction regex")
});

/// Keep diagnostics one physical line and bounded even when a remote server
/// returns a banner/error containing newlines or an unexpectedly large blob.
/// This prevents log injection and keeps a malformed SSH response from
/// turning the local log into an unbounded append workload.
fn sanitize_fragment(value: &str, max_bytes: usize) -> String {
    let mut sanitized = String::with_capacity(value.len().min(max_bytes));
    let mut used = 0usize;
    for character in value.chars() {
        let mut utf8 = [0; 4];
        let control;
        let escaped = match character {
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            character if character.is_control() => {
                control = format!("\\u{{{:04x}}}", character as u32);
                &control
            }
            character => character.encode_utf8(&mut utf8),
        };
        if used.saturating_add(escaped.len()) > max_bytes {
            sanitized.push_str("…[truncated]");
            break;
        }
        used += escaped.len();
        sanitized.push_str(escaped);
    }
    sanitized
}

fn redact(message: &str) -> String {
    // Redact before truncating. If a quoted secret is cut in half first, the
    // closing quote may disappear and the redaction pattern could leave a
    // prefix of that value in the bounded diagnostic line.
    let message = AUTHORIZATION_PATTERN.replace_all(message, "$1[REDACTED]");
    let message = SECRET_PATTERN.replace_all(&message, "$1$2[REDACTED]");
    sanitize_fragment(&message, MAX_LOG_MESSAGE_BYTES)
}

pub(super) fn build_line(
    level: LogLevel,
    category: LogCategory,
    scope: &str,
    message: &str,
) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!(
        "{timestamp} [{}] [{}] category={} {}\n",
        level.as_str(),
        sanitize_fragment(scope, 256),
        category.as_str(),
        redact(message)
    )
}

#[cfg(test)]
mod tests {
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
}
