<p align="center">
  <img src="branding/signal-kit-full.webp" alt="signal-kit — opinionated OpenTelemetry for production Rust services" width="560">
</p>

<h1 align="center">signal-kit</h1>

<p align="center">
  <strong>Opinionated OpenTelemetry for production Rust services.</strong>
</p>

<p align="center">
  <a href="#quick-start">Quick Start</a> · <a href="#configuration">Configuration</a> · <a href="#features">Features</a> · <a href="#examples">Examples</a>
</p>

---

`signal-kit` wires up traces, metrics, and logs in one builder call. It defaults to sensible choices (Tokio, `tracing`, OTLP/gRPC) and gets out of your way.

```rust
use signal_kit::ObservabilityBuilder;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = ObservabilityBuilder::new("my-service")
        .with_attribute("version", env!("CARGO_PKG_VERSION"))
        .init()?;

    // traces, metrics, and logs are all wired up
    tracing::info!("service started");
    Ok(())
}
```

Hold the returned [`OtelGuard`] — when it drops, providers flush and shut down cleanly.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
signal-kit = { path = "path/to/signal-kit" }
```

Then in `main`:

```rust
// Production — returns Result
let _guard = signal_kit::try_init("my-service")?;

// Examples and tools — panics on failure
let _guard = signal_kit::init("my-service");
```

Set the OTLP endpoint and you're exporting:

```sh
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
```

## Configuration

Everything is optional. Set `OTEL_EXPORTER_OTLP_ENDPOINT` to enable export; leave it unset for local development (stdout only).

### Environment variables

`signal-kit` follows OpenTelemetry conventions — signal-specific endpoints take precedence over the general one:

| Variable | Purpose |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | General OTLP endpoint |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | Traces-specific endpoint (overrides general) |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | Metrics-specific endpoint (overrides general) |
| `OTEL_EXPORTER_OTLP_LOGS_ENDPOINT` | Logs-specific endpoint (overrides general) |
| `RUST_LOG` | Log level filter (defaults to `info`) |

Crate-specific variables are namespaced under `SIGNAL_KIT_`:

| Variable | Default | Purpose |
|---|---|---|
| `SIGNAL_KIT_STRUCTURED_LOGGING` | `false` | JSON stdout output |
| `SIGNAL_KIT_FILE_ENABLED` | `false` | Enable file logging |
| `SIGNAL_KIT_FILE_PATH` | — | Log file path |
| `SIGNAL_KIT_FILE_ROTATION` | `daily` | `daily`, `hourly`, or `never` |
| `SIGNAL_KIT_FILE_RETENTION_DAYS` | `7` | Days to keep rotated logs |

### Resource attributes

Resource attributes are resolved in order: explicit builder attributes, `OTEL_RESOURCE_ATTRIBUTES`, crate convenience aliases (`ENVIRONMENT`, `CLOUD_*`, `KUBERNETES_*`), then compile-time metadata (`GIT_COMMIT`, `GIT_BRANCH`, etc.).

## Features

Default features: `traces`, `metrics`, `fmt`, `grpc-tonic`, `tls-roots`.

| Feature | Description |
|---|---|
| `traces` | OTLP trace export via `tracing-opentelemetry` |
| `metrics` | OTLP metric export with periodic reader |
| `fmt` | Stdout log output (pretty or JSON) |
| `json` | JSON-formatted stdout by default |
| `logs-otlp` | OTLP log export |
| `file-logging` | Rotating file log appender |
| `system` | System-level metrics (CPU, memory, disk, network) |
| `process` | Process-level metrics |
| `http` | HTTP request metrics with Tower middleware |
| `grpc-tonic` | gRPC transport via tonic (default) |
| `http-proto` | HTTP/protobuf transport |
| `tls` | TLS support |
| `tls-roots` | Native TLS root certificates (default) |
| `tls-webpki-roots` | WebPKI root certificates |

## Examples

### Custom resource attributes

```rust
let _guard = ObservabilityBuilder::new("my-service")
    .with_attribute("version", env!("CARGO_PKG_VERSION"))
    .with_attribute("environment", "production")
    .init()?;
```

### Custom metrics

```rust
let _guard = ObservabilityBuilder::new("my-service")
    .with_metrics_callback(|ctx| {
        let meter = ctx.meter();
        let counter = meter.u64_counter("requests.total").build();
        // store counter in your app state
        Ok(())
    })
    .init()?;
```

### File logging with rotation

```rust
use signal_kit::{FileLoggingConfig, ObservabilityBuilder, RotationConfig};

let file_config = FileLoggingConfig::builder()
    .enabled(true)
    .file_path("/var/log/myapp/service.log")
    .rotation(RotationConfig::daily())
    .build()?;

let _guard = ObservabilityBuilder::new("my-service")
    .with_file_logging(file_config)
    .init()?;
```

Rotation strategies: `Daily` (`service.2026-01-17`), `Hourly` (`service.2026-01-17.14`), `Never` (single file).

### Structured JSON stdout

```rust
let _guard = ObservabilityBuilder::new("my-service")
    .with_structured_stdout_logging()
    .init()?;
```

Or set `SIGNAL_KIT_STRUCTURED_LOGGING=true`.

## How it works

1. Builds an OpenTelemetry `Resource` with your service name and attributes
2. Creates trace, metric, and log providers with OTLP exporters (when an endpoint is configured)
3. Wires everything into a `tracing` subscriber — one layer per signal
4. Returns an `OtelGuard` that holds providers alive and flushes on drop

If no OTLP endpoint is set, exporters are skipped — you get local stdout logging with no network dependency. Perfect for development and tests.

## License

MIT OR Apache-2.0
