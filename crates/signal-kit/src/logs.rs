#[cfg(feature = "logs-otlp")]
use opentelemetry_otlp::LogExporter;
use opentelemetry_sdk::logs::SdkLoggerProvider;

use crate::traces::otel_resource_attributes::*;

/// Initialize the logging provider for the given service.
///
/// # Arguments
///
/// * `service_name` - The name of the service to initialize the logging
///   provider for.
///
/// # Returns
pub(crate) fn init_logging_provider(service_name: &str) -> SdkLoggerProvider {
    #[cfg_attr(not(feature = "logs-otlp"), allow(unused_mut))]
    let mut builder = SdkLoggerProvider::builder().with_resource(resource(service_name));

    #[cfg(feature = "logs-otlp")]
    // Check if OTLP endpoint is configured for logs
    if let Some(_endpoint) = crate::get_otlp_endpoint("logs") {
        let exporter = LogExporter::builder()
            .with_tonic()
            .build()
            .expect("Failed to create log exporter");

        builder = builder.with_batch_exporter(exporter);
    }

    builder.build()
}
