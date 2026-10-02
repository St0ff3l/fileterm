use std::path::PathBuf;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use super::{format::build_line, writer, LogCategory, LogLevel};

pub(super) struct FileTermLogLayer {
    directory: PathBuf,
}

impl FileTermLogLayer {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
}

#[derive(Default)]
struct EventFields {
    category: Option<String>,
    scope: Option<String>,
    message: Option<String>,
    attributes: Vec<Attribute>,
}

struct Attribute {
    name: String,
    value: String,
    is_string: bool,
}

impl EventFields {
    fn record(&mut self, field: &Field, value: String, is_string: bool) {
        match field.name() {
            "category" => self.category = Some(value),
            "scope" => self.scope = Some(value),
            "message" => self.message = Some(value),
            name => self.attributes.push(Attribute {
                name: name.to_owned(),
                value,
                is_string,
            }),
        }
    }

    fn message(&self, event_name: &str, span_path: Option<&str>) -> String {
        let mut message = self
            .message
            .clone()
            .unwrap_or_else(|| event_name.to_owned());

        if let Some(span_path) = span_path {
            message.push_str(" span=");
            message.push_str(&quoted(span_path));
        }

        for attribute in &self.attributes {
            message.push(' ');
            message.push_str(&attribute.name);
            message.push('=');
            if attribute.is_string {
                message.push_str(&quoted(&attribute.value));
            } else {
                message.push_str(&attribute.value);
            }
        }
        message
    }
}

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record(field, format!("{value:?}"), false);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record(field, value.to_owned(), true);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value.to_string(), false);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value.to_string(), false);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value.to_string(), false);
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.record(field, value.to_string(), false);
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"[unavailable]\"".to_owned())
}

fn level_from_tracing(level: &tracing::Level) -> Option<LogLevel> {
    if *level == tracing::Level::TRACE {
        Some(LogLevel::Trace)
    } else if *level == tracing::Level::DEBUG {
        Some(LogLevel::Debug)
    } else if *level == tracing::Level::INFO {
        Some(LogLevel::Info)
    } else if *level == tracing::Level::WARN {
        Some(LogLevel::Warn)
    } else if *level == tracing::Level::ERROR {
        Some(LogLevel::Error)
    } else {
        None
    }
}

impl<S> Layer<S> for FileTermLogLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let metadata = event.metadata();
        let Some(level) = level_from_tracing(metadata.level()) else {
            return;
        };

        let mut fields = EventFields::default();
        event.record(&mut fields);

        let scope = fields.scope.as_deref().unwrap_or_else(|| metadata.target());
        let category = fields
            .category
            .as_deref()
            .map(LogCategory::from_name)
            .unwrap_or_else(|| LogCategory::from_scope(scope));
        let span_path = context.event_scope(event).map(|spans| {
            spans
                .from_root()
                .map(|span| span.metadata().name())
                .collect::<Vec<_>>()
                .join("::")
        });
        let message = fields.message(metadata.name(), span_path.as_deref());
        let line = build_line(level, category, scope, &message);
        writer::dispatch(self.directory.clone(), level, line);
    }
}
