use std::sync::OnceLock;

/// Ordered severity shared by the backend and renderer IPC validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "TRACE" => Some(Self::Trace),
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    pub(super) fn enabled(self) -> bool {
        static MINIMUM_LEVEL: OnceLock<LogLevel> = OnceLock::new();
        // Preserve existing diagnostics by default; an invalid override also falls back to DEBUG.
        let minimum = MINIMUM_LEVEL.get_or_init(|| {
            std::env::var("FILETERM_LOG_LEVEL")
                .ok()
                .as_deref()
                .and_then(Self::parse)
                .unwrap_or(Self::Debug)
        });
        self >= *minimum
    }
}
