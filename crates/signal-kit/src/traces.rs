use opentelemetry::trace::TraceContextExt;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, SdkTracerProvider};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Tracing Resource Attributes
pub mod otel_resource_attributes;
pub use otel_resource_attributes::*;

/// Tracing JSON or Not
pub mod json_or_not;

/// Returns the current OpenTelemetry trace ID from the active span, or
/// `"unknown"` if no valid span context is available.
pub(crate) fn current_trace_id() -> String {
    let span = Span::current();
    let context = span.context();
    let span_ref = context.span();
    let span_context = span_ref.span_context();

    if span_context.is_valid() {
        span_context.trace_id().to_string()
    } else {
        "unknown".to_string()
    }
}

// Construct TracerProvider for OpenTelemetryLayer with additional resource attributes
pub(crate) fn init_tracer_provider_with_attributes(
    service_name: &str,
    additional_attributes: &[opentelemetry::KeyValue],
    otel_runtime: &crate::otel_runtime::SignalKitOtelRuntime,
) -> SdkTracerProvider {
    let mut builder = SdkTracerProvider::builder()
        .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(build_resource(
            service_name,
            additional_attributes.iter().cloned(),
        ));

    // Check if OTLP endpoint is configured for traces
    #[cfg(all(
        feature = "traces",
        any(feature = "grpc-tonic", feature = "http-proto", feature = "http-json")
    ))]
    if crate::get_otlp_endpoint("traces").is_some() {
        let exporter =
            crate::transport::span_exporter::build(crate::resolve_otlp_transport("traces"));
        let batch_processor =
            opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor::builder(
                exporter,
                otel_runtime.clone(),
            )
            .build();
        builder = builder.with_span_processor(batch_processor);
    }

    builder.build()
}
