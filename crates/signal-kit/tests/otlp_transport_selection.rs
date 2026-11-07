//! Integration tests for OTLP transport selection from the environment.
//!
//! These live outside the library because `signal-kit` forbids `unsafe_code`
//! and Rust 2024 makes test-scoped environment mutation unsafe.

use signal_kit::{OtlpTransport, resolve_otlp_transport};

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_protocols(traces: Option<&str>, general: Option<&str>, test_body: impl FnOnce()) {
	let _guard = ENV_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
	let signal_original = std::env::var("OTEL_EXPORTER_OTLP_TRACES_PROTOCOL").ok();
	let general_original = std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL").ok();
	let apply = |value: Option<&str>, key: &str| {
		if let Some(value) = value {
			// SAFETY: test-only environment mutation under a lock.
			unsafe { std::env::set_var(key, value) };
		} else {
			// SAFETY: test-only environment mutation under a lock.
			unsafe { std::env::remove_var(key) };
		}
	};
	apply(traces, "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");
	apply(general, "OTEL_EXPORTER_OTLP_PROTOCOL");
	test_body();
	apply(signal_original.as_deref(), "OTEL_EXPORTER_OTLP_TRACES_PROTOCOL");
	apply(general_original.as_deref(), "OTEL_EXPORTER_OTLP_PROTOCOL");
}

#[test]
fn default_transport_is_grpc_when_tonic_compiled() {
	#[cfg(feature = "grpc-tonic")]
	with_protocols(None, None, || {
		assert_eq!(resolve_otlp_transport("traces"), OtlpTransport::Grpc);
	});
	#[cfg(not(feature = "grpc-tonic"))]
	with_protocols(None, None, || {
		assert_ne!(resolve_otlp_transport("traces"), OtlpTransport::Grpc);
	});
}

#[test]
fn signal_specific_protocol_takes_precedence() {
	with_protocols(Some("http/json"), Some("grpc"), || {
		assert_eq!(resolve_otlp_transport("traces"), OtlpTransport::HttpJson);
	});
}

#[test]
fn general_protocol_applies_without_signal_override() {
	with_protocols(None, Some("http/protobuf"), || {
		assert_eq!(resolve_otlp_transport("metrics"), OtlpTransport::HttpProtobuf);
	});
}

#[test]
fn surrounding_whitespace_is_ignored() {
	with_protocols(Some("  http/json  "), None, || {
		assert_eq!(resolve_otlp_transport("traces"), OtlpTransport::HttpJson);
	});
}

#[test]
#[should_panic(expected = "unsupported OTLP protocol 'avro'")]
fn unknown_protocol_token_fails_clearly() {
	with_protocols(Some("avro"), None, || {
		let _ = resolve_otlp_transport("traces");
	});
}
