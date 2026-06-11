use crate::{OBSERVABILITY_EXPORT_INTERVAL, traces::resource};

/// System Metrics
#[cfg(feature = "system")]
pub mod system_metrics;
#[cfg(feature = "system")]
pub use system_metrics::*;

/// HTTP Metrics
#[cfg(feature = "http")]
pub mod http_metrics;
#[cfg(feature = "http")]
pub use http_metrics::*;

/// Process Metrics
#[cfg(feature = "process")]
pub mod process_metrics;
#[cfg(feature = "process")]
pub use process_metrics::*;

/// HTTP Metric Layer
#[cfg(feature = "http")]
pub mod http_metric_layer;
#[cfg(feature = "http")]
pub use http_metric_layer::*;

use opentelemetry::global;
use opentelemetry_sdk::metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider};

use super::observability::{MetricsInitCallback, MetricsInitContext, MetricsInitError};

/// Initialize the meter provider for the given service with metrics callbacks.
pub(crate) fn init_meter_provider_with_callbacks(
    service_name: &str,
    metrics_init_callbacks: Vec<MetricsInitCallback>,
) -> Result<SdkMeterProvider, MetricsInitError> {
    let mut meter_provider = MeterProviderBuilder::default().with_resource(resource(service_name));

    let export_enabled = crate::get_otlp_endpoint("metrics").is_some();

    // Check if OTLP endpoint is configured for metrics
    #[cfg(feature = "metrics")]
    if export_enabled {
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
            .build()
            .expect("Failed to build OTLP metric exporter");

        let reader = PeriodicReader::builder(exporter)
            .with_interval(OBSERVABILITY_EXPORT_INTERVAL)
            .build();

        meter_provider = meter_provider.with_reader(reader);
    }

    let meter_provider = meter_provider.build();

    global::set_meter_provider(meter_provider.clone());

    init_metrics();

    // Execute metrics initialization callbacks
    if !metrics_init_callbacks.is_empty() {
        tracing::info!(
            "Executing {} metrics initialization callbacks",
            metrics_init_callbacks.len()
        );

        let context =
            MetricsInitContext::new(global::meter(env!("CARGO_PKG_NAME")), export_enabled);

        for callback in metrics_init_callbacks {
            callback(&context)?;
        }

        tracing::info!("Metrics initialization callbacks completed");
    }

    Ok(meter_provider)
}

// Initialize all metrics
/// Initialize all metrics for the given service.
///
/// # Arguments
///
/// * `service_name` - The name of the service to initialize the metrics for.
fn init_metrics() {
    let _meter = global::meter(env!("CARGO_PKG_NAME"));

    // system
    #[cfg(feature = "system")]
    init_system_metrics(&_meter);
    #[cfg(feature = "process")]
    init_process_metrics(&_meter);
    #[cfg(feature = "http")]
    init_http_server_metrics(&_meter);
}
