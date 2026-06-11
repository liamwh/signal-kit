use std::io::IsTerminal;

use opentelemetry::trace::TracerProvider as _;
#[cfg(feature = "logs-otlp")]
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    logs::SdkLoggerProvider, metrics::SdkMeterProvider, trace::SdkTracerProvider,
};
use tracing_core::LevelFilter;
use tracing_error::ErrorLayer;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::{
    EnvFilter, fmt,
    fmt::format::{DefaultFields, Format, JsonFields},
    layer::SubscriberExt,
};

use super::{metrics::*, traces::json_or_not::JsonOrNot, traces::*};

/// Fallible observability initialization for applications, libraries, and tests.
pub fn try_init(service_name: impl Into<String>) -> Result<OtelGuard, OtelInitError> {
    ObservabilityBuilder::new(service_name).init()
}

/// Panicking observability initialization for examples, prototypes, and binaries
/// where startup failure should abort the process.
pub fn init(service_name: impl Into<String>) -> OtelGuard {
    try_init(service_name).expect("failed to initialise observability")
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceName(String);

impl ServiceName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServiceName {
    type Error = OtelInitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let value = value.trim().to_owned();
        if value.is_empty() {
            return Err(OtelInitError::MissingServiceName);
        }
        Ok(Self(value))
    }
}

/// Callback type for registering custom metrics after the meter provider is set up.
pub type MetricsInitCallback =
    Box<dyn Fn(&MetricsInitContext) -> Result<(), MetricsInitError> + Send + Sync + 'static>;

/// Context passed to metrics initialization callbacks.
#[derive(Clone, Debug)]
pub struct MetricsInitContext {
    meter: opentelemetry::metrics::Meter,
    export_enabled: bool,
}

impl MetricsInitContext {
    pub(crate) fn new(meter: opentelemetry::metrics::Meter, export_enabled: bool) -> Self {
        Self {
            meter,
            export_enabled,
        }
    }

    /// Returns the meter consumers should use to register custom metrics.
    pub fn meter(&self) -> &opentelemetry::metrics::Meter {
        &self.meter
    }

    /// Returns whether metric export was configured.
    pub fn export_enabled(&self) -> bool {
        self.export_enabled
    }
}

/// Errors that can occur during metrics initialization.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MetricsInitError {
    /// General metrics initialization error.
    #[error("metrics initialization failed: {message}")]
    Initialization {
        /// Description of the initialization failure.
        message: String,
    },
}

/// Errors that can occur during observability initialization.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OtelInitError {
    /// No service name was provided.
    #[error("service name must be provided")]
    MissingServiceName,
    /// The global tracing subscriber has already been set.
    #[error("global tracing subscriber is already initialised")]
    SubscriberAlreadyInitialised,
    /// Failed to install the global tracing subscriber.
    #[error("failed to install global tracing subscriber")]
    SetGlobalSubscriber(#[from] tracing::subscriber::SetGlobalDefaultError),
    /// File logging configuration or setup failed.
    #[error("file logging configuration failed")]
    FileLogging(#[from] crate::file_logging::FileLoggingError),
    /// Metrics initialization failed.
    #[error("metrics initialisation failed")]
    Metrics(#[from] MetricsInitError),
}

/// Aggregate shutdown error across OpenTelemetry providers.
#[derive(Debug, thiserror::Error)]
#[error("one or more OpenTelemetry providers failed to shut down")]
pub struct OtelShutdownError {
    failures: Vec<OtelShutdownFailure>,
}

impl OtelShutdownError {
    pub(crate) fn new(failures: Vec<OtelShutdownFailure>) -> Self {
        Self { failures }
    }

    /// Returns the individual shutdown failures.
    pub fn failures(&self) -> &[OtelShutdownFailure] {
        &self.failures
    }

    /// Returns `true` if there are no failures.
    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Individual provider shutdown failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OtelShutdownFailure {
    /// Tracer provider failed to shut down.
    #[error("tracer provider shutdown failed: {0}")]
    TracerProvider(Box<dyn std::error::Error + Send + Sync>),
    /// Meter provider failed to shut down.
    #[error("meter provider shutdown failed: {0}")]
    MeterProvider(Box<dyn std::error::Error + Send + Sync>),
    /// Logger provider failed to shut down.
    #[error("logger provider shutdown failed: {0}")]
    LoggerProvider(Box<dyn std::error::Error + Send + Sync>),
}

/// Summary of what was initialized during observability setup.
#[derive(Debug, Clone)]
pub struct InitSummary {
    service_name: String,
    traces_enabled: bool,
    metrics_enabled: bool,
    otlp_logs_enabled: bool,
    fmt_layer_enabled: bool,
    file_logging_enabled: bool,
    structured_stdout_logging_enabled: bool,
    otlp_traces_endpoint: Option<String>,
    otlp_metrics_endpoint: Option<String>,
    otlp_logs_endpoint: Option<String>,
    warnings: Vec<InitWarning>,
}

impl InitSummary {
    /// Returns the service name used during initialization.
    pub fn service_name(&self) -> &str {
        &self.service_name
    }
    /// Returns whether trace export was enabled.
    pub fn traces_enabled(&self) -> bool {
        self.traces_enabled
    }
    /// Returns whether metric export was enabled.
    pub fn metrics_enabled(&self) -> bool {
        self.metrics_enabled
    }
    /// Returns whether OTLP log export was enabled.
    pub fn otlp_logs_enabled(&self) -> bool {
        self.otlp_logs_enabled
    }
    /// Returns whether the fmt (stdout) layer was enabled.
    pub fn fmt_layer_enabled(&self) -> bool {
        self.fmt_layer_enabled
    }
    /// Returns whether file logging was enabled.
    pub fn file_logging_enabled(&self) -> bool {
        self.file_logging_enabled
    }
    /// Returns whether structured JSON stdout logging was enabled.
    pub fn structured_stdout_logging_enabled(&self) -> bool {
        self.structured_stdout_logging_enabled
    }
    /// Returns the configured OTLP traces endpoint, if any.
    pub fn otlp_traces_endpoint(&self) -> Option<&str> {
        self.otlp_traces_endpoint.as_deref()
    }
    /// Returns the configured OTLP metrics endpoint, if any.
    pub fn otlp_metrics_endpoint(&self) -> Option<&str> {
        self.otlp_metrics_endpoint.as_deref()
    }
    /// Returns the configured OTLP logs endpoint, if any.
    pub fn otlp_logs_endpoint(&self) -> Option<&str> {
        self.otlp_logs_endpoint.as_deref()
    }
    /// Returns any warnings generated during initialization.
    pub fn warnings(&self) -> &[InitWarning] {
        &self.warnings
    }
}

/// Warnings generated during observability initialization.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum InitWarning {
    /// An OTLP endpoint was not configured for the given signal.
    MissingEndpoint {
        /// The signal kind that is missing an endpoint.
        signal: SignalKind,
    },
}

/// Kind of OpenTelemetry signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// Distributed traces.
    Traces,
    /// Metrics.
    Metrics,
    /// Logs.
    Logs,
}

/// Builder for configuring and initializing observability.
#[derive(Default)]
pub struct ObservabilityBuilder {
    service_name: Option<String>,
    additional_attributes: Vec<opentelemetry::KeyValue>,
    metrics_callbacks: Vec<MetricsInitCallback>,
    file_logging_config: Option<crate::FileLoggingConfig>,
    structured_stdout_logging: Option<bool>,
}

impl ObservabilityBuilder {
    /// Creates a new builder with the given service name.
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            service_name: Some(service_name.into()),
            ..Self::default()
        }
    }

    /// Overrides the service name.
    pub fn service_name(mut self, service_name: impl Into<String>) -> Self {
        self.service_name = Some(service_name.into());
        self
    }

    /// Sets additional resource attributes, replacing any previously set.
    pub fn with_attributes(mut self, attributes: Vec<opentelemetry::KeyValue>) -> Self {
        self.additional_attributes = attributes;
        self
    }

    /// Sets additional resource attributes from an iterator, replacing any previously set.
    pub fn with_attributes_iter<I>(mut self, attributes: I) -> Self
    where
        I: IntoIterator<Item = opentelemetry::KeyValue>,
    {
        self.additional_attributes = attributes.into_iter().collect();
        self
    }

    /// Appends a single resource attribute.
    pub fn with_attribute<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<opentelemetry::Key>,
        V: Into<opentelemetry::Value>,
    {
        self.additional_attributes
            .push(opentelemetry::KeyValue::new(key, value));
        self
    }

    /// Registers a callback to run after the meter provider is initialized.
    pub fn with_metrics_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&MetricsInitContext) -> Result<(), MetricsInitError> + Send + Sync + 'static,
    {
        self.metrics_callbacks.push(Box::new(callback));
        self
    }

    /// Enables file logging with the given configuration.
    pub fn with_file_logging(mut self, config: crate::FileLoggingConfig) -> Self {
        self.file_logging_config = Some(config);
        self
    }

    /// Enables structured JSON output to stdout.
    pub fn with_structured_stdout_logging(mut self) -> Self {
        self.structured_stdout_logging = Some(true);
        self
    }

    /// Initializes all configured observability providers and returns a guard.
    pub fn init(self) -> Result<OtelGuard, OtelInitError> {
        if tracing_core::dispatcher::has_been_set() {
            return Err(OtelInitError::SubscriberAlreadyInitialised);
        }

        let service_name = ServiceName::try_from(self.service_name.unwrap_or_default())?;
        let traces_endpoint = crate::get_otlp_endpoint("traces");
        let metrics_endpoint = crate::get_otlp_endpoint("metrics");
        let logs_endpoint = crate::get_otlp_endpoint("logs");
        let structured_stdout_logging = self
            .structured_stdout_logging
            .unwrap_or_else(structured_logging_enabled);

        let mut warnings = Vec::new();
        if traces_endpoint.is_none() {
            warnings.push(InitWarning::MissingEndpoint {
                signal: SignalKind::Traces,
            });
        }
        if metrics_endpoint.is_none() {
            warnings.push(InitWarning::MissingEndpoint {
                signal: SignalKind::Metrics,
            });
        }
        #[cfg(feature = "logs-otlp")]
        if logs_endpoint.is_none() {
            warnings.push(InitWarning::MissingEndpoint {
                signal: SignalKind::Logs,
            });
        }

        let summary = InitSummary {
            service_name: service_name.as_str().to_owned(),
            traces_enabled: cfg!(feature = "traces"),
            metrics_enabled: cfg!(feature = "metrics"),
            otlp_logs_enabled: cfg!(feature = "logs-otlp"),
            fmt_layer_enabled: cfg!(feature = "fmt"),
            file_logging_enabled: self
                .file_logging_config
                .as_ref()
                .is_some_and(|config| config.enabled),
            structured_stdout_logging_enabled: structured_stdout_logging,
            otlp_traces_endpoint: traces_endpoint,
            otlp_metrics_endpoint: metrics_endpoint,
            otlp_logs_endpoint: logs_endpoint,
            warnings,
        };

        let tracer_provider = init_tracer_provider_with_attributes(
            service_name.as_str(),
            &self.additional_attributes,
        );
        let meter_provider =
            init_meter_provider_with_callbacks(service_name.as_str(), self.metrics_callbacks)?;
        let logger_provider = crate::logs::init_logging_provider(service_name.as_str());

        let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));
        let tracing_otlp_layer = OpenTelemetryLayer::new(tracer);
        let metrics_otlp_layer = MetricsLayer::new(meter_provider.clone());

        let stdout = std::io::stdout();
        let ansi = stdout.is_terminal();

        let registry = tracing_subscriber::registry()
            .with(get_stdout_filter())
            .with(metrics_otlp_layer)
            .with(tracing_otlp_layer)
            .with(ErrorLayer::default());

        #[cfg(feature = "logs-otlp")]
        let registry = registry.with(OpenTelemetryTracingBridge::new(&logger_provider));

        #[cfg(feature = "file-logging")]
        let (file_layer, file_guard) = if let Some(ref file_config) = self.file_logging_config
            && file_config.enabled
        {
            let Some((file_writer, file_guard)) =
                crate::file_logging::build_file_writer(file_config)?
            else {
                return Err(crate::file_logging::FileLoggingError::InvalidPath(
                    "file logging returned None when enabled".to_string(),
                )
                .into());
            };
            let file_layer = fmt::layer()
                .with_writer(file_writer)
                .json()
                .with_current_span(true)
                .with_span_list(true);
            (Some(file_layer), Some(file_guard))
        } else {
            (None, None)
        };
        #[cfg(feature = "file-logging")]
        let registry = registry.with(file_layer);

        #[cfg(not(feature = "file-logging"))]
        let file_guard = None::<()>;
        #[cfg(not(feature = "file-logging"))]
        let registry = registry.with(None::<fmt::Layer<_>>);

        let dispatch: tracing::Dispatch = if structured_stdout_logging {
            registry
                .with(
                    fmt::layer()
                        .with_ansi(ansi)
                        .event_format(JsonOrNot::Json(Format::default().json()))
                        .fmt_fields(JsonFields::new()),
                )
                .into()
        } else {
            registry
                .with(
                    fmt::layer()
                        .with_ansi(ansi)
                        .event_format(JsonOrNot::Not(Format::default()))
                        .fmt_fields(DefaultFields::new()),
                )
                .into()
        };

        tracing::dispatcher::set_global_default(dispatch)
            .map_err(OtelInitError::SetGlobalSubscriber)?;

        Ok(OtelGuard {
            inner: Some(OtelGuardInner {
                tracer_provider,
                meter_provider,
                logger_provider,
                _file_guard: file_guard,
            }),
            summary,
        })
    }
}

const USE_STRUCTURED_LOGGING_ENV_VAR: &str = "SIGNAL_KIT_STRUCTURED_LOGGING";
const USE_STRUCTURED_LOGGING_DEFAULT: bool = false;

fn structured_logging_enabled() -> bool {
    std::env::var(USE_STRUCTURED_LOGGING_ENV_VAR)
        .unwrap_or(USE_STRUCTURED_LOGGING_DEFAULT.to_string())
        .parse::<bool>()
        .unwrap_or(false)
}

fn get_stdout_filter() -> EnvFilter {
    EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy()
        .add_directive("tonic=off".parse().unwrap())
        .add_directive("h2=off".parse().unwrap())
        .add_directive("reqwest=off".parse().unwrap())
        .add_directive("opentelemetry=warn".parse().unwrap())
        .add_directive("opentelemetry_sdk=warn".parse().unwrap())
        .add_directive("opentelemetry-otlp=warn".parse().unwrap())
}

/// RAII guard that holds OpenTelemetry providers alive and shuts them down on drop.
pub struct OtelGuard {
    inner: Option<OtelGuardInner>,
    summary: InitSummary,
}

impl OtelGuard {
    /// Flush and shut down all OpenTelemetry providers.
    pub fn shutdown(mut self) -> Result<(), OtelShutdownError> {
        let Some(inner) = self.inner.take() else {
            return Ok(());
        };
        inner.shutdown()
    }

    /// Returns a summary of what was initialized.
    pub fn summary(&self) -> &InitSummary {
        &self.summary
    }
}

struct OtelGuardInner {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
    logger_provider: SdkLoggerProvider,
    #[cfg(feature = "file-logging")]
    _file_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    #[cfg(not(feature = "file-logging"))]
    _file_guard: Option<()>,
}

impl OtelGuardInner {
    fn shutdown(self) -> Result<(), OtelShutdownError> {
        let mut failures = Vec::new();
        if let Err(err) = self.tracer_provider.shutdown() {
            failures.push(OtelShutdownFailure::TracerProvider(Box::new(err)));
        }
        if let Err(err) = self.meter_provider.shutdown() {
            failures.push(OtelShutdownFailure::MeterProvider(Box::new(err)));
        }
        if let Err(err) = self.logger_provider.shutdown() {
            failures.push(OtelShutdownFailure::LoggerProvider(Box::new(err)));
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(OtelShutdownError::new(failures))
        }
    }
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            let _ = inner.shutdown();
        }
    }
}

#[cfg(test)]
mod test {
    use opentelemetry::KeyValue;

    use super::{ObservabilityBuilder, ServiceName};

    #[test]
    fn service_name_is_trimmed_and_validated() {
        assert_eq!(
            ServiceName::try_from(" test-service ".to_string())
                .unwrap()
                .as_str(),
            "test-service"
        );
        assert!(ServiceName::try_from(" ".to_string()).is_err());
    }

    #[test]
    fn test_observability_builder_creation() {
        let builder = ObservabilityBuilder::new("test-service");
        assert_eq!(builder.service_name, Some("test-service".to_string()));
        assert!(builder.additional_attributes.is_empty());
        assert!(builder.metrics_callbacks.is_empty());
    }

    #[test]
    fn test_observability_builder_with_attributes() {
        let attr1 = KeyValue::new("version", "1.0.0");
        let attr2 = KeyValue::new("environment", "test");

        let builder = ObservabilityBuilder::new("test-service")
            .with_attributes(vec![attr1.clone(), attr2.clone()]);

        assert_eq!(builder.additional_attributes.len(), 2);
        assert_eq!(builder.additional_attributes[0], attr1);
        assert_eq!(builder.additional_attributes[1], attr2);
    }

    #[test]
    fn test_observability_builder_with_attribute() {
        let builder = ObservabilityBuilder::new("test-service")
            .with_attribute("version", "1.0.0")
            .with_attribute("environment", "test");

        assert_eq!(builder.additional_attributes.len(), 2);
        assert_eq!(builder.additional_attributes[0].key.as_str(), "version");
        assert_eq!(builder.additional_attributes[1].key.as_str(), "environment");
    }
}
