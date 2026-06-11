use std::time::{SystemTime as StdSystemTime, UNIX_EPOCH};

use serde_json::{Map, Value, json};
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{
        FmtContext, FormatEvent, FormatFields,
        format::{Format, Full, Json, Writer},
        time::SystemTime,
    },
    registry::LookupSpan,
};

use super::current_trace_id;
use tracing::field::{Field, Visit};

const COLOR_PINK: &str = "\x1b[38;5;205m";
const COLOR_CYAN: &str = "\x1b[36m";
const COLOR_WHITE: &str = "\x1b[97m";
const COLOR_RESET: &str = "\x1b[0m";

/// A struct that allows us to dynamically choose JSON formatting without using
/// dynamic dispatch. This is just so we avoid any sort of possible slow down in
/// logging code
pub enum JsonOrNot {
    /// Text-based formatter for human readable output
    Not(Format<Full, SystemTime>),
    /// JSON formatter for structured logging suitable for machine parsing
    Json(Format<Json, SystemTime>),
}

impl<S, N> FormatEvent<S, N> for JsonOrNot
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        match self {
            JsonOrNot::Not(f) => {
                let trace_id = current_trace_id();

                // Prefix human-readable logs with a colored trace id.
                write!(
                    writer,
                    "[{pink}trace_id{reset}{white}={reset}{cyan}{trace_id}{reset}] ",
                    pink = COLOR_PINK,
                    white = COLOR_WHITE,
                    cyan = COLOR_CYAN,
                    reset = COLOR_RESET,
                    trace_id = trace_id,
                )?;

                f.format_event(ctx, writer, event)
            }
            JsonOrNot::Json(_f) => {
                let trace_id = current_trace_id();

                // Collect structured fields from the event.
                let mut visitor = JsonVisitor::default();
                event.record(&mut visitor);

                // Basic event metadata.
                let metadata = event.metadata();
                let level = metadata.level().to_string();
                let target = metadata.target();

                // Simple UNIX timestamp in seconds with fractional part.
                let timestamp = StdSystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64();

                let value = json!({
                    "timestamp": timestamp,
                    "level": level,
                    "target": target,
                    "trace_id": trace_id,
                    "fields": visitor.fields,
                });

                writeln!(writer, "{value}")
            }
        }
    }
}

#[derive(Default)]
struct JsonVisitor {
    fields: Map<String, Value>,
}

impl JsonVisitor {
    fn insert<T: Into<Value>>(&mut self, field: &Field, value: T) {
        self.fields.insert(field.name().to_string(), value.into());
    }
}

impl Visit for JsonVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, json!(value));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, json!(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, json!(value));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.insert(field, json!(value));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.insert(field, json!(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.insert(field, json!(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, json!(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.insert(field, json!(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.insert(field, json!(format!("{value:?}")));
    }
}
