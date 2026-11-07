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
/// Bridges an SDK 0.31 gap: `BatchLogProcessor` (async-runtime flavour)
/// implements `shutdown` but not `shutdown_with_timeout`, whose trait default
/// is a no-op — so provider shutdowns would silently skip flushing queued
/// records. Forwarding the timeout variant to `shutdown` restores the drain.
#[derive(Debug)]
struct ShutdownFlushingLogProcessor {
    inner: opentelemetry_sdk::logs::log_processor_with_async_runtime::BatchLogProcessor<
        crate::otel_runtime::SignalKitOtelRuntime,
    >,
}

impl opentelemetry_sdk::logs::LogProcessor for ShutdownFlushingLogProcessor {
    fn emit(
        &self,
        data: &mut opentelemetry_sdk::logs::SdkLogRecord,
        instrumentation: &opentelemetry::InstrumentationScope,
    ) {
        self.inner.emit(data, instrumentation);
    }
    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.force_flush()
    }
    fn shutdown_with_timeout(
        &self,
        _timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        self.inner.shutdown()
    }
}

pub(crate) fn init_logging_provider(
    service_name: &str,
    otel_runtime: &crate::otel_runtime::SignalKitOtelRuntime,
) -> SdkLoggerProvider {
    #[cfg_attr(not(feature = "logs-otlp"), allow(unused_mut))]
    let mut builder = SdkLoggerProvider::builder().with_resource(resource(service_name));

    #[cfg(all(
        feature = "logs-otlp",
        any(feature = "grpc-tonic", feature = "http-proto", feature = "http-json")
    ))]
    // Check if OTLP endpoint is configured for logs
    if crate::get_otlp_endpoint("logs").is_some() {
        let exporter =
            crate::transport::log_exporter::build(crate::resolve_otlp_transport("logs"));
        let batch_processor =
            opentelemetry_sdk::logs::log_processor_with_async_runtime::BatchLogProcessor::builder(
                exporter,
                otel_runtime.clone(),
            )
            .build();
        let flushing = ShutdownFlushingLogProcessor {
            inner: batch_processor,
        };
        builder = builder.with_log_processor(flushing);
    }

    builder.build()
}
