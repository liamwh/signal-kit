use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use http::{Request, Response, header::CONTENT_LENGTH};
use http_body::Body;
use tokio::time::Instant;
use tower::{Layer, Service};
use tracing::instrument;

use crate::metrics::http_metrics::http_server::{
    record_http_server_active_requests, record_http_server_request_body_size,
    record_http_server_request_duration, record_http_server_response_body_size,
};

/// A tower Layer that logs each request's URI path and how long the inner
/// service took to respond.
#[derive(Debug, Clone)]
pub struct HTTPMetricLayer;

impl<S> Layer<S> for HTTPMetricLayer {
    type Service = HTTPMetricMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HTTPMetricMiddleware { inner }
    }
}

/// The actual middleware. Wraps any inner service `S` that takes
/// `http::Request<ReqBody>`.
#[derive(Debug, Clone)]
pub struct HTTPMetricMiddleware<S> {
    inner: S,
}

impl<S, ReqBody, ResBody> Service<Request<ReqBody>> for HTTPMetricMiddleware<S>
where
    // your wrapped
    // service must
    // accept the same
    // ReqBody…
    S: Service<Request<ReqBody>, Response = Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    // …and we require both ReqBody and ResBody to implement `http_body::Body`.
    ReqBody: Body + Send + 'static,
    ReqBody::Data: Send,
    ReqBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    ResBody: Body + Send + 'static,
    ResBody::Data: Send,
    ResBody::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[instrument(skip(self, req), level = "debug")]
    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let start = Instant::now();

        let route = req.uri().path().to_owned();

        let method = req.method().to_string();
        let scheme = req.uri().scheme_str().unwrap_or("http").to_string(); // Default to http if scheme is missing

        let protocol_version = format!("{:?}", req.version()); // e.g., "HTTP/1.1"

        let (server_addr, server_port): (Option<String>, Option<u16>) = (None, None);

        // Get request size from Content-Length header if available
        let request_size = req
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        // Record request start and size using new functions
        record_http_server_active_requests(
            1,
            &method,
            &scheme,
            server_addr.as_deref(),
            server_port.map(|p| p as i64),
        );

        let mut svc = self.inner.clone();

        // Clone necessary info for the async block
        let request_size_clone = request_size;
        let route_clone = route.clone();
        let method_clone = method.clone();
        let scheme_clone = scheme.clone();
        let protocol_version_clone = protocol_version.clone();
        let server_addr_clone = server_addr.clone();
        let server_port_clone = server_port;

        Box::pin(async move {
            let result = svc.call(req).await;
            let duration = start.elapsed().as_secs_f64();

            let (status_code, response_size, error_type) = match &result {
                Ok(response) => {
                    let status = response.status();
                    let size = response
                        .headers()
                        .get(CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let err_type = if status.is_client_error() {
                        Some("http.client_error")
                    } else if status.is_server_error() {
                        Some("http.server_error")
                    } else {
                        None
                    };

                    (Some(status.as_u16() as i64), size, err_type)
                }
                Err(_) => {
                    // In case of error, status code is typically not available from the Tower
                    // service error itself. We set response size to 0 and
                    // derive an error type string. The specific error type
                    // might need more sophisticated mapping depending on S::Error.
                    (None, 0, Some("service_error")) // Generic error type for service errors
                }
            };

            // Record request end metrics
            record_http_server_request_duration(
                duration,
                &method_clone,
                &scheme_clone,
                error_type,
                status_code,
                Some(&route_clone),
                Some("http"),
                Some(&protocol_version_clone),
                server_addr_clone.as_deref(),
                server_port_clone.map(|p| p as i64),
            );

            // Record request body size AFTER getting status and error
            record_http_server_request_body_size(
                request_size_clone,
                &method_clone,
                &scheme_clone,
                error_type,
                status_code,
                Some(&route_clone),
                Some("http"),
                Some(&protocol_version_clone),
                server_addr_clone.as_deref(),
                server_port_clone.map(|p| p as i64),
            );

            record_http_server_response_body_size(
                response_size,
                &method_clone,
                &scheme_clone,
                error_type,
                status_code,
                Some(&route_clone),
                Some("http"),
                Some(&protocol_version_clone),
                server_addr_clone.as_deref(),
                server_port_clone.map(|p| p as i64),
            );

            // Decrement active requests counter regardless of success or failure
            record_http_server_active_requests(
                -1,
                &method_clone,
                &scheme_clone,
                server_addr_clone.as_deref(),
                server_port_clone.map(|p| p as i64),
            );

            result // Return the original result
        })
    }
}
