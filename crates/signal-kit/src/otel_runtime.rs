//! Dedicated Tokio runtime for signal-kit's OTLP exporters.
//!
//! The batch/periodic processors shipped with `rt-tokio` run their worker on
//! a dedicated thread and drive export futures through ambient Tokio
//! handles, which do not exist inside that thread — exporters with async
//! HTTP or gRPC transports can never complete there regardless of the host
//! application's runtime. signal-kit instead drives the
//! `*_with_async_runtime` processor variants with this adapter, which always
//! dispatches onto one owned, single-threaded runtime retained for the
//! process lifetime by [`crate::OtelGuard`].

use std::sync::Arc;
use std::time::Duration;

use opentelemetry_sdk::runtime::{Runtime, RuntimeChannel};

/// OpenTelemetry runtime adapter backed by an owned Tokio runtime handle.
#[derive(Clone, Debug)]
pub(crate) struct SignalKitOtelRuntime {
	handle: tokio::runtime::Handle,
}

/// A dedicated Tokio runtime driving all OTLP export work for one process.
#[derive(Debug)]
pub(crate) struct SignalKitOtelExportRuntime {
	runtime: Option<Arc<tokio::runtime::Runtime>>,
}

impl SignalKitOtelExportRuntime {
	/// Build the dedicated export runtime.
	///
	/// # Errors
	/// Returns the io error when the runtime's worker thread cannot start.
	pub(crate) fn new() -> std::io::Result<Self> {
		let runtime = tokio::runtime::Builder::new_multi_thread()
			.worker_threads(1)
			.enable_all()
			.thread_name("signal-kit-otel")
			.build()?;
		Ok(Self {
			runtime: Some(Arc::new(runtime)),
		})
	}

	/// The runtime adapter handed to OpenTelemetry processors.
	pub(crate) fn adapter(&self) -> SignalKitOtelRuntime {
		let runtime = self
			.runtime
			.as_ref()
			.expect("export runtime cannot be dropped before its processors");
		SignalKitOtelRuntime {
			handle: runtime.handle().clone(),
		}
	}
}

impl Drop for SignalKitOtelExportRuntime {
	fn drop(&mut self) {
		// Dropping a Tokio runtime blocks until its tasks park, which panics
		// inside async contexts (guards are commonly dropped from async code).
		// Teardown runs on a detached thread instead; exporters have already
		// flushed by the time the guard drops.
		if let Some(runtime) = self.runtime.take() {
			std::thread::Builder::new()
				.name("signal-kit-otel-shutdown".to_string())
				.spawn(move || drop(runtime))
				.ok();
		}
	}
}

impl Runtime for SignalKitOtelRuntime {
	fn spawn<F>(&self, future: F)
	where
		F: Future<Output = ()> + Send + 'static,
	{
		#[allow(clippy::let_underscore_future)]
		let _ = self.handle.spawn(future);
	}

	fn delay(&self, duration: Duration) -> impl Future<Output = ()> + Send + 'static {
		// `async fn` cannot satisfy the trait's `+ 'static` return bound.
		#[allow(clippy::manual_async_fn)]
		{
			async move {
				tokio::time::sleep(duration).await;
			}
		}
	}
}

impl RuntimeChannel for SignalKitOtelRuntime {
	type Receiver<T: std::fmt::Debug + Send> = tokio_stream::wrappers::ReceiverStream<T>;
	type Sender<T: std::fmt::Debug + Send> = tokio::sync::mpsc::Sender<T>;

	fn batch_message_channel<T: std::fmt::Debug + Send>(
		&self,
		capacity: usize,
	) -> (Self::Sender<T>, Self::Receiver<T>) {
		let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
		(
			sender,
			tokio_stream::wrappers::ReceiverStream::new(receiver),
		)
	}
}
