//! OTLP transport selection for signal-kit exporters.
//!
//! signal-kit derives the transport for each signal (traces, metrics, logs)
//! from the standard OpenTelemetry protocol environment variables:
//!
//! - `OTEL_EXPORTER_OTLP_<SIGNAL>_PROTOCOL` (signal-specific, takes precedence)
//! - `OTEL_EXPORTER_OTLP_PROTOCOL`
//!
//! Supported values are `grpc`, `http/protobuf` and `http/json`. When neither
//! variable is set the transport defaults to gRPC whenever the `grpc-tonic`
//! cargo feature is compiled (the historical signal-kit behaviour), otherwise
//! whichever single HTTP protocol feature is available.
//!
//! Endpoints, headers and timeouts are resolved by the underlying
//! `opentelemetry-otlp` builders from `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT`
//! (used as-is, full URL) or `OTEL_EXPORTER_OTLP_ENDPOINT` (base URL, the
//! signal path is appended automatically), so signal-kit adds no endpoint
//! handling of its own.
//!
//! HTTP transports additionally require an HTTP client feature
//! (`reqwest-client`, `reqwest-blocking-client` or
//! `reqwest-rustls-webpki-roots`); unsupported combinations fail at
//! initialisation with an explicit remediation message instead of silently
//! exporting nothing.

/// The OTLP transport used to export one signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtlpTransport {
	/// gRPC via tonic (`grpc`).
	Grpc,
	/// OTLP/HTTP with protobuf payloads (`http/protobuf`).
	HttpProtobuf,
	/// OTLP/HTTP with JSON payloads (`http/json`).
	HttpJson,
}

impl OtlpTransport {
	/// The transport used when no protocol environment variable is set.
	fn default_for_compiled_features(signal: &str) -> Self {
		if cfg!(feature = "grpc-tonic") {
			return Self::Grpc;
		}
		if cfg!(feature = "http-proto") {
			return Self::HttpProtobuf;
		}
		if cfg!(feature = "http-json") {
			return Self::HttpJson;
		}
		panic!("signal-kit was compiled without any OTLP transport feature; enable one of grpc-tonic, http-proto or http-json to export signal '{signal}'");
	}

	/// The protocol token as it appears in the environment variables.
	fn protocol_token(self) -> &'static str {
		match self {
			Self::Grpc => "grpc",
			Self::HttpProtobuf => "http/protobuf",
			Self::HttpJson => "http/json",
		}
	}
}

/// Resolve the OTLP transport for a signal (`"traces"`, `"metrics"`, `"logs"`).
///
/// Signal-specific `OTEL_EXPORTER_OTLP_<SIGNAL>_PROTOCOL` takes precedence
/// over `OTEL_EXPORTER_OTLP_PROTOCOL`. An unsupported value fails at
/// initialisation with the valid alternatives.
///
/// # Panics
/// Panics when the configured protocol token is not one of `grpc`,
/// `http/protobuf` or `http/json`.
pub fn resolve_otlp_transport(signal: &str) -> OtlpTransport {
	let signal_var = format!("OTEL_EXPORTER_OTLP_{}_PROTOCOL", signal.to_uppercase());
	let configured = std::env::var(&signal_var)
		.or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL"))
		.unwrap_or_default();
	let trimmed = configured.trim();
	if trimmed.is_empty() {
		return OtlpTransport::default_for_compiled_features(signal);
	}
	match trimmed {
		"grpc" => OtlpTransport::Grpc,
		"http/protobuf" => OtlpTransport::HttpProtobuf,
		"http/json" => OtlpTransport::HttpJson,
		other => panic!(
			"unsupported OTLP protocol '{other}' for signal '{signal}' (set via {signal_var} or OTEL_EXPORTER_OTLP_PROTOCOL); expected one of: grpc, http/protobuf, http/json"
		),
	}
}

/// Fail at initialisation when the selected transport was not compiled in.
pub(crate) fn assert_transport_supported(transport: OtlpTransport, signal: &str) {
	let missing_feature = match transport {
		OtlpTransport::Grpc => {
			if cfg!(feature = "grpc-tonic") {
				None
			} else {
				Some("grpc-tonic")
			}
		}
		OtlpTransport::HttpProtobuf => {
			if cfg!(feature = "http-proto") {
				None
			} else {
				Some("http-proto")
			}
		}
		OtlpTransport::HttpJson => {
			if cfg!(feature = "http-json") {
				None
			} else {
				Some("http-json")
			}
		}
	};
	if let Some(feature) = missing_feature {
		panic!(
			"OTLP protocol '{}' selected for signal '{signal}' but signal-kit was compiled without the '{feature}' cargo feature; enable it (or choose a compiled protocol)",
			transport.protocol_token()
		);
	}
	if matches!(transport, OtlpTransport::HttpProtobuf | OtlpTransport::HttpJson)
		&& !cfg!(any(
			feature = "reqwest-client",
			feature = "reqwest-blocking-client",
			feature = "reqwest-rustls-webpki-roots",
			feature = "hyper-client"
		))
	{
		panic!(
			"OTLP protocol '{}' selected for signal '{signal}' but no HTTP client is compiled; enable one of signal-kit's client features: reqwest-client, reqwest-blocking-client, reqwest-rustls-webpki-roots",
			transport.protocol_token()
		);
	}
}

/// Describe a transport build failure with the transport context.
pub(crate) fn build_failure(transport: OtlpTransport, signal: &str, error: String) -> String {
	format!(
		"failed to build OTLP {} exporter for protocol '{}': {error}",
		signal,
		transport.protocol_token()
	)
}

use opentelemetry_otlp::Protocol;

fn http_protocol(transport: OtlpTransport) -> Protocol {
	match transport {
		OtlpTransport::HttpJson => Protocol::HttpJson,
		OtlpTransport::HttpProtobuf => Protocol::HttpBinary,
		OtlpTransport::Grpc => unreachable!("gRPC transport does not use an HTTP protocol"),
	}
}

#[cfg(all(
	feature = "traces",
	any(feature = "grpc-tonic", feature = "http-proto", feature = "http-json")
))]
pub(crate) mod span_exporter {
	use super::{OtlpTransport, Protocol, assert_transport_supported, build_failure, http_protocol};

	/// Build the OTLP span exporter for the selected transport.
	pub(crate) fn build(transport: OtlpTransport) -> opentelemetry_otlp::SpanExporter {
		assert_transport_supported(transport, "traces");
		let result = build_inner(transport);
		result.unwrap_or_else(|error| panic!("{}", build_failure(transport, "trace", error)))
	}

	fn build_inner(
		transport: OtlpTransport,
	) -> Result<opentelemetry_otlp::SpanExporter, String> {
		match transport {
			OtlpTransport::Grpc => grpc(transport),
			OtlpTransport::HttpProtobuf | OtlpTransport::HttpJson => http(transport, http_protocol(transport)),
		}
	}

	#[cfg(feature = "grpc-tonic")]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::SpanExporter, String> {
		opentelemetry_otlp::SpanExporter::builder()
			.with_tonic()
			.build()
			.map_err(|error| error.to_string())
	}

	#[cfg(not(feature = "grpc-tonic"))]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::SpanExporter, String> {
		Err("grpc transport requires the grpc-tonic feature".to_string())
	}

	#[cfg(any(feature = "http-proto", feature = "http-json"))]
	fn http(
		_transport: OtlpTransport,
		protocol: Protocol,
	) -> Result<opentelemetry_otlp::SpanExporter, String> {
		use opentelemetry_otlp::WithExportConfig;
		opentelemetry_otlp::SpanExporter::builder()
			.with_http()
			.with_protocol(protocol)
			.build()
			.map_err(|error| error.to_string())
	}

	#[cfg(not(any(feature = "http-proto", feature = "http-json")))]
	fn http(
		_transport: OtlpTransport,
		_protocol: Protocol,
	) -> Result<opentelemetry_otlp::SpanExporter, String> {
		Err("HTTP transport requires the http-proto or http-json feature".to_string())
	}
}

#[cfg(all(
	feature = "logs-otlp",
	any(feature = "grpc-tonic", feature = "http-proto", feature = "http-json")
))]
pub(crate) mod log_exporter {
	use super::{OtlpTransport, Protocol, assert_transport_supported, build_failure, http_protocol};

	/// Build the OTLP log exporter for the selected transport.
	pub(crate) fn build(transport: OtlpTransport) -> opentelemetry_otlp::LogExporter {
		assert_transport_supported(transport, "logs");
		let result = build_inner(transport);
		result.unwrap_or_else(|error| panic!("{}", build_failure(transport, "log", error)))
	}

	fn build_inner(transport: OtlpTransport) -> Result<opentelemetry_otlp::LogExporter, String> {
		match transport {
			OtlpTransport::Grpc => grpc(transport),
			OtlpTransport::HttpProtobuf | OtlpTransport::HttpJson => http(transport, http_protocol(transport)),
		}
	}

	#[cfg(feature = "grpc-tonic")]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::LogExporter, String> {
		opentelemetry_otlp::LogExporter::builder()
			.with_tonic()
			.build()
			.map_err(|error| error.to_string())
	}

	#[cfg(not(feature = "grpc-tonic"))]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::LogExporter, String> {
		Err("grpc transport requires the grpc-tonic feature".to_string())
	}

	#[cfg(any(feature = "http-proto", feature = "http-json"))]
	fn http(
		_transport: OtlpTransport,
		protocol: Protocol,
	) -> Result<opentelemetry_otlp::LogExporter, String> {
		use opentelemetry_otlp::WithExportConfig;
		opentelemetry_otlp::LogExporter::builder()
			.with_http()
			.with_protocol(protocol)
			.build()
			.map_err(|error| error.to_string())
	}


	#[cfg(not(any(feature = "http-proto", feature = "http-json")))]
	fn http(
		_transport: OtlpTransport,
		_protocol: Protocol,
	) -> Result<opentelemetry_otlp::LogExporter, String> {
		Err("HTTP transport requires the http-proto or http-json feature".to_string())
	}
}

#[cfg(all(
	feature = "metrics",
	any(feature = "grpc-tonic", feature = "http-proto", feature = "http-json")
))]
pub(crate) mod metric_exporter {
	use super::{OtlpTransport, Protocol, assert_transport_supported, build_failure, http_protocol};

	/// Build the OTLP metric exporter for the selected transport.
	pub(crate) fn build(transport: OtlpTransport) -> opentelemetry_otlp::MetricExporter {
		assert_transport_supported(transport, "metrics");
		let result = build_inner(transport);
		result.unwrap_or_else(|error| panic!("{}", build_failure(transport, "metric", error)))
	}

	fn build_inner(transport: OtlpTransport) -> Result<opentelemetry_otlp::MetricExporter, String> {
		match transport {
			OtlpTransport::Grpc => grpc(transport),
			OtlpTransport::HttpProtobuf | OtlpTransport::HttpJson => http(transport, http_protocol(transport)),
		}
	}

	#[cfg(feature = "grpc-tonic")]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::MetricExporter, String> {
		opentelemetry_otlp::MetricExporter::builder()
			.with_tonic()
			.with_temporality(opentelemetry_sdk::metrics::Temporality::default())
			.build()
			.map_err(|error| error.to_string())
	}

	#[cfg(not(feature = "grpc-tonic"))]
	fn grpc(_transport: OtlpTransport) -> Result<opentelemetry_otlp::MetricExporter, String> {
		Err("grpc transport requires the grpc-tonic feature".to_string())
	}

	#[cfg(any(feature = "http-proto", feature = "http-json"))]
	fn http(
		_transport: OtlpTransport,
		protocol: Protocol,
	) -> Result<opentelemetry_otlp::MetricExporter, String> {
		use opentelemetry_otlp::WithExportConfig;
		opentelemetry_otlp::MetricExporter::builder()
			.with_http()
			.with_protocol(protocol)
			.with_temporality(opentelemetry_sdk::metrics::Temporality::default())
			.build()
			.map_err(|error| error.to_string())
	}

	#[cfg(not(any(feature = "http-proto", feature = "http-json")))]
	fn http(
		_transport: OtlpTransport,
		_protocol: Protocol,
	) -> Result<opentelemetry_otlp::MetricExporter, String> {
		Err("HTTP transport requires the http-proto or http-json feature".to_string())
	}
}
