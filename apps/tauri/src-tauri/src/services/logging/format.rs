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
mod tests;
