//! Proof that signal-kit does not change how you emit telemetry.
//!
//! Every span and event in this binary comes from the plain [`tracing`]
//! crate — the code reads exactly as it would without signal-kit. The
//! `payment_service` module below stands in for a dependency crate that
//! knows nothing about signal-kit; it lights up anyway because signal-kit
//! installs a *global* subscriber underneath the unchanged `tracing`
//! facade.
//!
//! Run it:
//!
//! ```sh
//! cargo run -p signal-kit --example tracing-facade-unchanged
//! ```
//!
//! With no OTLP endpoint configured, the spans and events installed by
//! signal-kit's `fmt` layer print to stdout. Set
//! `OTEL_EXPORTER_OTLP_ENDPOINT` and the same untouched calls export via
//! OTLP instead.

/// A stand-in for a dependency crate: this module imports `tracing` only.
mod payment_service {
    use tracing::{info, instrument};

    /// Charges an order. The `#[instrument]` attribute is plain `tracing`.
    #[instrument(skip_all, fields(order_id = %order_id))]
    pub async fn charge(order_id: u64, amount_cents: u64) -> Result<(), String> {
        info!(amount_cents, "charging card");

        if amount_cents > 100_000 {
            let error = "amount exceeds gateway limit";
            info!(%error, "charge declined");
            return Err(error.to_owned());
        }

        gateway_call(amount_cents).await
    }

    /// Child span created implicitly by `#[instrument]`.
    #[instrument(skip_all)]
    async fn gateway_call(amount_cents: u64) -> Result<(), String> {
        let _ = amount_cents;
        info!("gateway approved");
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    // The only signal-kit code in this example: install the global
    // subscriber. Nothing else in this file references signal-kit.
    let guard = signal_kit::init("tracing-facade-unchanged");

    // Everything below is unchanged plain `tracing`: an explicit span
    // attached to a future with `Instrument`, `#[instrument]`ed calls in
    // `payment_service`, structured fields, and display-formatted errors.
    use tracing::{Instrument, info, info_span};

    async {
        info!("starting checkout");

        if let Err(error) = payment_service::charge(42, 2_500).await {
            info!(%error, "checkout failed");
        }

        if let Err(error) = payment_service::charge(7, 500_000).await {
            info!(%error, "checkout failed");
        }

        // Metrics, too, stay `tracing`-native: signal-kit's MetricsLayer
        // turns `counter.` / `gauge.` / `histogram.` /
        // `monotonic_counter.` fields into OpenTelemetry metrics.
        info!(monotonic_counter.checkouts = 1u64);
    }
    .instrument(info_span!("checkout", user = "u-123"))
    .await;

    // Dropping the guard flushes and shuts down the providers. Spans and
    // events emitted before this point were routed by signal-kit's
    // subscriber without a single change to the calls above.
    drop(guard);
}
