//! Integration test: OTLP/HTTP with JSON payloads exports real spans.
//!
//! Runs as its own test binary because `signal-kit` installs the process-wide
//! tracing subscriber. The receiver decodes the OTLP/JSON body and asserts the
//! emitted span arrived with its resource attributes — not merely a 200.

#![cfg(all(feature = "http-json", feature = "reqwest-rustls-webpki-roots"))]

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use tokio::sync::mpsc;

#[derive(Debug)]
struct ReceivedExport {
	headers: HeaderMap,
	body: String,
}

async fn record_export(
	State(exports): State<Arc<mpsc::UnboundedSender<ReceivedExport>>>,
	headers: HeaderMap,
	body: String,
) -> &'static str {
	let _ = exports.send(ReceivedExport { headers, body });
	"ok"
}

#[tokio::test(flavor = "multi_thread")]
async fn http_json_transport_exports_a_decoded_span() {
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
		std::env::set_var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL", "http/json");
	}

	let guard = signal_kit::ObservabilityBuilder::new("signal-kit-http-json-test")
		.init()
		.expect("initialisation must succeed with a configured HTTP exporter");

	let span = tracing::info_span!("signal_kit.transport.test", probe = "http-json");
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
	assert_eq!(content_type, "application/json", "OTLP/JSON content type expected");

	let payload: serde_json::Value = serde_json::from_str(&export.body)
		.unwrap_or_else(|error| panic!("body must decode as OTLP/JSON: {error}: {}", export.body));

	let resource_spans = payload["resourceSpans"]
		.as_array()
		.expect("resourceSpans array");
	assert!(!resource_spans.is_empty(), "at least one resource span");

	let attributes = &resource_spans[0]["resource"]["attributes"];
	let service_name = attributes
		.as_array()
		.into_iter()
		.flatten()
		.find(|attribute| attribute["key"] == "service.name")
		.and_then(|attribute| attribute["value"]["stringValue"].as_str())
		.expect("service.name resource attribute");
	assert_eq!(service_name, "signal-kit-http-json-test");

	let spans = &resource_spans[0]["scopeSpans"][0]["spans"];
	let names = spans
		.as_array()
		.into_iter()
		.flatten()
		.filter_map(|span| span["name"].as_str())
		.collect::<Vec<_>>();
	assert!(
		names.contains(&"signal_kit.transport.test"),
		"the emitted span must be present, got: {names:?}"
	);
}
