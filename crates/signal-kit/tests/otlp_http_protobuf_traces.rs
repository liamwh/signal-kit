//! Integration test: OTLP/HTTP with protobuf payloads exports real spans.
//!
//! Runs as its own test binary because `signal-kit` installs the process-wide
//! tracing subscriber. The receiver decodes the protobuf body into
//! `ExportTraceServiceRequest` and asserts the emitted span arrived — not
//! merely a 200 response.

#![cfg(all(feature = "http-proto", feature = "reqwest-rustls-webpki-roots"))]

use prost::Message;

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use tokio::sync::mpsc;

#[derive(Debug)]
struct ReceivedExport {
	headers: HeaderMap,
	body: Vec<u8>,
}

async fn record_export(
	State(exports): State<Arc<mpsc::UnboundedSender<ReceivedExport>>>,
	headers: HeaderMap,
	body: axum::body::Bytes,
) -> &'static str {
	let _ = exports.send(ReceivedExport {
		headers,
		body: body.to_vec(),
	});
	"ok"
}

#[tokio::test(flavor = "multi_thread")]
async fn http_protobuf_transport_exports_a_decoded_span() {
	let (export_sender, mut export_receiver) = mpsc::unbounded_channel::<ReceivedExport>();
	let app = Router::new()
		.route("/v1/traces", post(record_export))
		.with_state(Arc::new(export_sender));
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	tokio::spawn(async move {
		axum::serve(listener, app).await.unwrap();
	});

	// SAFETY: test-only environment mutation before initialisation.
	unsafe {
		std::env::set_var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT", format!("http://{address}/v1/traces"));
		std::env::set_var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "http/protobuf");
	}

	let guard = signal_kit::ObservabilityBuilder::new("signal-kit-http-proto-test")
		.init()
		.expect("initialisation must succeed with a configured HTTP exporter");

	let span = tracing::info_span!("signal_kit.transport.test", probe = "http-protobuf");
	let _guard = span.enter();
	tracing::info!("event inside the exported span");
	drop(_guard);
	drop(span);

	guard.shutdown().expect("shutdown must flush the batch exporter");

	let export = tokio::time::timeout(std::time::Duration::from_secs(10), export_receiver.recv())
		.await
		.expect("an export must arrive after shutdown")
		.expect("receiver must stay open");

	let content_type = export
		.headers
		.get("content-type")
		.and_then(|value| value.to_str().ok())
		.unwrap_or_default();
	assert_eq!(
		content_type, "application/x-protobuf",
		"OTLP/protobuf content type expected"
	);

	let request =
		opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest::decode(
			export.body.as_slice(),
		)
		.unwrap_or_else(|error| panic!("body must decode as OTLP protobuf: {error}"));

	let resource_spans = request.resource_spans;
	assert!(!resource_spans.is_empty(), "at least one resource span");

	let service_name = resource_spans[0]
		.resource
		.as_ref()
		.expect("resource present")
		.attributes
		.iter()
		.find(|attribute| attribute.key == "service.name")
		.and_then(|attribute| {
			attribute
				.value
				.as_ref()
				.and_then(|value| match &value.value {
					Some(
						opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue(name),
					) => Some(name.as_str()),
					_ => None,
				})
		})
		.expect("service.name resource attribute");
	assert_eq!(service_name, "signal-kit-http-proto-test");

	let names: Vec<&str> = resource_spans[0]
		.scope_spans
		.iter()
		.flat_map(|scope| scope.spans.iter().map(|span| span.name.as_str()))
		.collect();
	assert!(
		names.contains(&"signal_kit.transport.test"),
		"the emitted span must be present, got: {names:?}"
	);
}
