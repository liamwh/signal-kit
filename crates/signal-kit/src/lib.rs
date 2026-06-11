//! # signal-kit
//!
//! Opinionated OpenTelemetry initialisation for async Rust services.
//!
//! Most applications should use [`ObservabilityBuilder`] or the convenience
//! functions [`init`] and [`try_init`].
//!
//! ```no_run
//! use signal_kit::ObservabilityBuilder;
//!
//! # fn main() -> Result<(), signal_kit::OtelInitError> {
//! let _guard = ObservabilityBuilder::new("my-service")
//!     .with_attribute("version", env!("CARGO_PKG_VERSION"))
//!     .init()?;
//! # Ok(())
//! # }
//! ```
//!
//! Use [`try_init`] for production applications and tests. Use [`init`] for
//! examples, prototypes, and binaries where panicking on startup failure is
//! acceptable. The crate installs the global tracing subscriber and should
//! usually be initialized once near the start of `main`.
//!
//! ## Environment Variables
//!
//! Compile-time build metadata is read with `option_env!`: `GIT_COMMIT`,
//! `GIT_BRANCH`, `GIT_REPOSITORY_URL`, and `BUILD_NUMBER`.
//!
//! Runtime exporter variables follow OpenTelemetry conventions:
//! `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`,
//! `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT`, and
//! `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT`. Signal-specific endpoints take
//! precedence over the general endpoint.
//!
//! Crate-specific runtime variables are namespaced under `SIGNAL_KIT_`:
//! `SIGNAL_KIT_STRUCTURED_LOGGING`, `SIGNAL_KIT_FILE_ENABLED`,
//! `SIGNAL_KIT_FILE_PATH`, `SIGNAL_KIT_FILE_ROTATION`, and
//! `SIGNAL_KIT_FILE_RETENTION_DAYS`.
//!
//! Resource attributes are resolved in this order: explicit builder
//! attributes, `OTEL_RESOURCE_ATTRIBUTES`, crate convenience aliases such as
//! `ENVIRONMENT`, `CLOUD_*`, and `KUBERNETES_*`, then compile-time metadata.
#![forbid(unsafe_code)]

use std::time::Duration;

mod observability;
pub use observability::{
    InitSummary, InitWarning, MetricsInitCallback, MetricsInitContext, MetricsInitError,
    ObservabilityBuilder, OtelGuard, OtelInitError, OtelShutdownError, OtelShutdownFailure,
    SignalKind, init, try_init,
};

/// Metrics initialization and configuration.
pub mod metrics;

mod traces;
pub use traces::otel_resource_attributes::build_resource;

mod logs;

pub mod file_logging;
pub use file_logging::{FileLoggingConfig, FileLoggingError, RotationConfig, RotationStrategy};

/// The interval at which metrics and traces are exported to the otlp collector.
pub const OBSERVABILITY_EXPORT_INTERVAL: Duration = Duration::from_secs(15);

/// Check if OTLP endpoint is configured for a specific signal.
///
/// According to OpenTelemetry specification, signal-specific endpoints take precedence
/// over the general endpoint.
///
/// # Arguments
/// * `signal` - The signal type ("traces", "metrics", "logs")
///
/// # Returns
/// * `Some(endpoint)` if an endpoint is configured, `None` otherwise
fn get_otlp_endpoint(signal: &str) -> Option<String> {
    // First check for signal-specific endpoint
    let signal_env = format!("OTEL_EXPORTER_OTLP_{}_ENDPOINT", signal.to_uppercase());
    if let Ok(endpoint) = std::env::var(&signal_env)
        && !endpoint.trim().is_empty()
    {
        return Some(endpoint);
    }

    // Fall back to general endpoint
    if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        && !endpoint.trim().is_empty()
    {
        return Some(endpoint);
    }

    None
}
