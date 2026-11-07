//! Integration test: OTLP/HTTP carries logs and metrics, not just traces.
//!
//! Runs as its own test binary because `signal-kit` installs the process-wide
//! tracing subscriber. Both signals flush on shutdown; the receiver decodes
//! each payload and asserts the emitted records arrived.

#![cfg(all(
	feature = "http-json",
	feature = "reqwest-rustls-webpki-roots",
	feature = "logs-otlp",
	feature = "metrics"
))]

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::post;
use axum::Router;
use tokio::sync::mpsc;

#[derive(Debug)]
struct ReceivedExport {
	path: String,
	headers: HeaderMap,
	body: String,
}

async fn record_traces(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	body: String,
) -> &'static str {
	let _ = state.exports.send(ReceivedExport {
		path: "/v1/traces".to_string(),
		headers,
		body,
	});
	"ok"
}

async fn record_logs(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	body: String,
) -> &'static str {
	let _ = state.exports.send(ReceivedExport {
		path: "/v1/logs".to_string(),
		headers,
		body,
	});
	"ok"
}

async fn record_metrics(
	State(state): State<Arc<AppState>>,
	headers: HeaderMap,
	body: String,
) -> &'static str {
	let _ = state.exports.send(ReceivedExport {
		path: "/v1/metrics".to_string(),
		headers,
		body,
	});
	"ok"
}

struct AppState {
	exports: mpsc::UnboundedSender<ReceivedExport>,
}

#[tokio::test(flavor = "multi_thread")]
async fn http_json_transport_exports_logs_and_metrics() {
	let (export_sender, mut export_receiver) = mpsc::unbounded_channel::<ReceivedExport>();
	let state = Arc::new(AppState {
		exports: export_sender,
	});
	let app = Router::new()
		.route("/v1/traces", post(record_traces))
		.route("/v1/logs", post(record_logs))
		.route("/v1/metrics", post(record_metrics))
		.with_state(state);
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	tokio::spawn(async move {
		axum::serve(listener, app).await.unwrap();
	});

	let base = format!("http://{address}");
	// SAFETY: test-only environment mutation before initialisation.
	unsafe {
		std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", &base);
		std::env::set_var("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json");
	}

	let guard = signal_kit::ObservabilityBuilder::new("signal-kit-http-logs-test")
		.init()
		.expect("initialisation must succeed with HTTP exporters");

	tracing::warn!(probe = "http-logs", "log record that must be exported");
	let meter = opentelemetry::global::meter("signal-kit-transport-test");
	let counter = meter.u64_counter("signal_kit.transport.counter").build();
	counter.add(1, &[]);

	guard.shutdown().expect("shutdown must flush all exporters");

	let mut saw_log = false;
	let mut saw_metric = false;
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
	while std::time::Instant::now() < deadline && (!saw_log || !saw_metric) {
		let export = tokio::time::timeout(std::time::Duration::from_secs(15), export_receiver.recv())
			.await
			.expect("exports must keep arriving after shutdown")
			.expect("receiver must stay open");
		let content_type = export
			.headers
			.get("content-type")
			.and_then(|value| value.to_str().ok())
			.unwrap_or_default();
		assert_eq!(content_type, "application/json");

		match export.path.as_str() {
			"/v1/logs" => {
				let body: serde_json::Value = serde_json::from_str(&export.body)
					.unwrap_or_else(|error| panic!("logs body must decode: {error}"));
				let bodies = body["logRecords"]
					.as_array()
					.or_else(|| {
						body["resourceLogs"]
							.as_array()
							.and_then(|resource_logs| resource_logs.first())
							.and_then(|first| first["scopeLogs"].as_array())
							.and_then(|scopes| scopes.first())
							.and_then(|scope| scope["logRecords"].as_array())
					})
					.expect("logRecords array");
				saw_log |= bodies
					.iter()
					.any(|record| record["body"]["stringValue"] == "log record that must be exported");
			}
			"/v1/metrics" => {
				let body: serde_json::Value = serde_json::from_str(&export.body)
					.unwrap_or_else(|error| panic!("metrics body must decode: {error}"));
				let text = body.to_string();
				saw_metric |= text.contains("signal_kit.transport.counter");
			}
			_ => {}
		}
	}
	assert!(saw_log, "the emitted log record must be exported over HTTP");
	assert!(
		saw_metric,
		"the recorded counter must be exported over HTTP after shutdown"
	);
}
