use std::sync::OnceLock;

use opentelemetry::{
    KeyValue,
    metrics::{Histogram, Meter, UpDownCounter},
};
use opentelemetry_semantic_conventions::{
    attribute::*,
    metric::{
        HTTP_SERVER_ACTIVE_REQUESTS, HTTP_SERVER_REQUEST_BODY_SIZE, HTTP_SERVER_REQUEST_DURATION,
        HTTP_SERVER_RESPONSE_BODY_SIZE,
    },
};

// Metrics
static HTTP_SERVER_REQUEST_DURATION_HISTOGRAM: OnceLock<Histogram<f64>> = OnceLock::new();
static HTTP_SERVER_ACTIVE_REQUESTS_UP_DOWN_COUNTER: OnceLock<UpDownCounter<i64>> = OnceLock::new();
static HTTP_SERVER_REQUEST_BODY_SIZE_HISTOGRAM: OnceLock<Histogram<u64>> = OnceLock::new();
static HTTP_SERVER_RESPONSE_BODY_SIZE_HISTOGRAM: OnceLock<Histogram<u64>> = OnceLock::new();

/// Initialize the HTTP server metrics.
///
/// # Parameters
/// - `meter`: the OpenTelemetry meter to use for recording metrics
pub fn init_http_server_metrics(meter: &Meter) {
    HTTP_SERVER_REQUEST_DURATION_HISTOGRAM.get_or_init(|| {
        meter
            .f64_histogram(HTTP_SERVER_REQUEST_DURATION)
            .with_description("Duration of HTTP server requests.")
            .with_unit("s")
            .build()
    });

    HTTP_SERVER_ACTIVE_REQUESTS_UP_DOWN_COUNTER.get_or_init(|| {
        meter
            .i64_up_down_counter(HTTP_SERVER_ACTIVE_REQUESTS)
            .with_description("Number of active HTTP server requests.")
            .with_unit("{request}")
            .build()
    });

    HTTP_SERVER_REQUEST_BODY_SIZE_HISTOGRAM.get_or_init(|| {
        meter
            .u64_histogram(HTTP_SERVER_REQUEST_BODY_SIZE)
            .with_description("Size of HTTP server request bodies.")
            .with_unit("By")
            .build()
    });

    HTTP_SERVER_RESPONSE_BODY_SIZE_HISTOGRAM.get_or_init(|| {
        meter
            .u64_histogram(HTTP_SERVER_RESPONSE_BODY_SIZE)
            .with_description("Size of HTTP server response bodies.")
            .with_unit("By")
            .build()
    });
}

/// Records an HTTP server request duration metric with semantic attributes.
///
/// # Parameters
/// - `duration_seconds`: duration of the HTTP request in seconds
/// - `method`: HTTP method (e.g. `"GET"`, `"POST"`)
/// - `url_scheme`: URI scheme (e.g. `"http"`, `"https"`)
/// - `error_type`: optional error type if the request failed
/// - `status_code`: optional HTTP status code if one was returned
/// - `route`: optional route template (e.g. `"/users/:id"`)
/// - `network_protocol_name`: optional network protocol (e.g. `"http"`,
///   `"spdy"`)
/// - `network_protocol_version`: optional protocol version (e.g. `"1.1"`,
///   `"2"`)
/// - `server_address`: optional local server address that received the request
/// - `server_port`: optional local server port that received the request
#[allow(clippy::too_many_arguments)]
pub fn record_http_server_request_duration(
    duration_seconds: f64,
    method: &str,
    url_scheme: &str,
    error_type: Option<&str>,
    status_code: Option<i64>,
    route: Option<&str>,
    network_protocol_name: Option<&str>,
    network_protocol_version: Option<&str>,
    server_address: Option<&str>,
    server_port: Option<i64>,
) {
    // guard: if the histogram hasn’t been init’d, skip recording
    let histogram = match HTTP_SERVER_REQUEST_DURATION_HISTOGRAM.get() {
        Some(h) => h,
        None => return,
    };

    let mut attrs = Vec::with_capacity(8);
    attrs.push(KeyValue::new(HTTP_REQUEST_METHOD, method.to_string()));
    attrs.push(KeyValue::new(URL_SCHEME, url_scheme.to_string()));

    if let Some(err) = error_type {
        attrs.push(KeyValue::new(ERROR_TYPE, err.to_string()));
    }

    if let Some(code) = status_code {
        attrs.push(KeyValue::new(HTTP_RESPONSE_STATUS_CODE, code));
    }

    if let Some(r) = route {
        attrs.push(KeyValue::new(HTTP_ROUTE, r.to_string()));
    }

    if let Some(name) = network_protocol_name {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_NAME, name.to_string()));
    }

    if let Some(version) = network_protocol_version {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_VERSION, version.to_string()));
    }

    if let Some(addr) = server_address {
        attrs.push(KeyValue::new(SERVER_ADDRESS, addr.to_string()));
    }

    if let Some(port) = server_port {
        attrs.push(KeyValue::new(SERVER_PORT, port));
    }

    histogram.record(duration_seconds, &attrs);
}

/// Records a change in the number of active HTTP server requests with semantic
/// attributes.
///
/// # Parameters
/// - `delta`: the change in active requests (e.g. +1 when a request starts, -1
///   when it ends)
/// - `method`: HTTP method (e.g. `"GET"`, `"POST"`)
/// - `url_scheme`: URI scheme (e.g. `"http"`, `"https"`)
/// - `server_address`: optional local server address that received the request
/// - `server_port`: optional local server port that received the request
pub fn record_http_server_active_requests(
    delta: i64,
    method: &str,
    url_scheme: &str,
    server_address: Option<&str>,
    server_port: Option<i64>,
) {
    let counter = match HTTP_SERVER_ACTIVE_REQUESTS_UP_DOWN_COUNTER.get() {
        Some(c) => c,
        None => return,
    };

    let mut attrs = Vec::with_capacity(4);
    attrs.push(KeyValue::new(HTTP_REQUEST_METHOD, method.to_string()));
    attrs.push(KeyValue::new(URL_SCHEME, url_scheme.to_string()));
    if let Some(addr) = server_address {
        attrs.push(KeyValue::new(SERVER_ADDRESS, addr.to_string()));
    }
    if let Some(port) = server_port {
        attrs.push(KeyValue::new(SERVER_PORT, port));
    }

    counter.add(delta, &attrs);
}

/// Records the size of an HTTP server request body with semantic attributes.
///
/// # Parameters
/// - `size_bytes`: size of the request body in bytes
/// - `method`: HTTP method (e.g. `"GET"`, `"POST"`)
/// - `url_scheme`: URI scheme (e.g. `"http"`, `"https"`)
/// - `error_type`: optional error type if the request failed
/// - `status_code`: optional HTTP status code if one was returned
/// - `route`: optional route template (e.g. `"/users/:id"`)
/// - `network_protocol_name`: optional network protocol (e.g. `"http"`,
///   `"spdy"`)
/// - `network_protocol_version`: optional protocol version (e.g. `"1.1"`,
///   `"2"`)
/// - `server_address`: optional local server address that received the request
/// - `server_port`: optional local server port that received the request
#[allow(clippy::too_many_arguments)]
pub fn record_http_server_request_body_size(
    size_bytes: u64,
    method: &str,
    url_scheme: &str,
    error_type: Option<&str>,
    status_code: Option<i64>,
    route: Option<&str>,
    network_protocol_name: Option<&str>,
    network_protocol_version: Option<&str>,
    server_address: Option<&str>,
    server_port: Option<i64>,
) {
    let histogram = match HTTP_SERVER_REQUEST_BODY_SIZE_HISTOGRAM.get() {
        Some(h) => h,
        None => return,
    };

    let mut attrs = Vec::with_capacity(8);
    attrs.push(KeyValue::new(HTTP_REQUEST_METHOD, method.to_string()));
    attrs.push(KeyValue::new(URL_SCHEME, url_scheme.to_string()));
    if let Some(err) = error_type {
        attrs.push(KeyValue::new(ERROR_TYPE, err.to_string()));
    }
    if let Some(code) = status_code {
        attrs.push(KeyValue::new(HTTP_RESPONSE_STATUS_CODE, code));
    }
    if let Some(r) = route {
        attrs.push(KeyValue::new(HTTP_ROUTE, r.to_string()));
    }
    if let Some(name) = network_protocol_name {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_NAME, name.to_string()));
    }
    if let Some(version) = network_protocol_version {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_VERSION, version.to_string()));
    }
    if let Some(addr) = server_address {
        attrs.push(KeyValue::new(SERVER_ADDRESS, addr.to_string()));
    }
    if let Some(port) = server_port {
        attrs.push(KeyValue::new(SERVER_PORT, port));
    }

    histogram.record(size_bytes, &attrs);
}

/// Records the size of an HTTP server response body with semantic attributes.
///
/// # Parameters
/// - `size_bytes`: size of the response body in bytes
/// - `method`: HTTP method (e.g. `"GET"`, `"POST"`)
/// - `url_scheme`: URI scheme (e.g. `"http"`, `"https"`)
/// - `error_type`: optional error type if the request failed
/// - `status_code`: optional HTTP status code if one was returned
/// - `route`: optional route template (e.g. `"/users/:id"`)
/// - `network_protocol_name`: optional network protocol (e.g. `"http"`,
///   `"spdy"`)
/// - `network_protocol_version`: optional protocol version (e.g. `"1.1"`,
///   `"2"`)
/// - `server_address`: optional local server address that received the request
/// - `server_port`: optional local server port that received the request
#[allow(clippy::too_many_arguments)]
pub fn record_http_server_response_body_size(
    size_bytes: u64,
    method: &str,
    url_scheme: &str,
    error_type: Option<&str>,
    status_code: Option<i64>,
    route: Option<&str>,
    network_protocol_name: Option<&str>,
    network_protocol_version: Option<&str>,
    server_address: Option<&str>,
    server_port: Option<i64>,
) {
    let histogram = match HTTP_SERVER_RESPONSE_BODY_SIZE_HISTOGRAM.get() {
        Some(h) => h,
        None => return,
    };

    let mut attrs = Vec::with_capacity(8);
    attrs.push(KeyValue::new(HTTP_REQUEST_METHOD, method.to_string()));
    attrs.push(KeyValue::new(URL_SCHEME, url_scheme.to_string()));
    if let Some(err) = error_type {
        attrs.push(KeyValue::new(ERROR_TYPE, err.to_string()));
    }
    if let Some(code) = status_code {
        attrs.push(KeyValue::new(HTTP_RESPONSE_STATUS_CODE, code));
    }
    if let Some(r) = route {
        attrs.push(KeyValue::new(HTTP_ROUTE, r.to_string()));
    }
    if let Some(name) = network_protocol_name {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_NAME, name.to_string()));
    }
    if let Some(version) = network_protocol_version {
        attrs.push(KeyValue::new(NETWORK_PROTOCOL_VERSION, version.to_string()));
    }
    if let Some(addr) = server_address {
        attrs.push(KeyValue::new(SERVER_ADDRESS, addr.to_string()));
    }
    if let Some(port) = server_port {
        attrs.push(KeyValue::new(SERVER_PORT, port));
    }

    histogram.record(size_bytes, &attrs);
}
