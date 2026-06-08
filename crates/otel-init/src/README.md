# Observability

## Metrics

Each metric as specified by the [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/specs/semconv/) should have it's own function to record it.

Each metric category should have it's own file which includes the functions to record the metrics. For observables these are init functions, and for non-observables these are the `record_{metric_name}` functions. We define a metric category as the heading item in the table of contents in the docs page. So for example, for the [http](https://opentelemetry.io/docs/specs/semconv/http/http-metrics/#http-server) metrics, there should be two files for HTTP Server and HTTP Client.

Each metric category should have it's own init function, for example `init_http_server_metrics`. This should also call the init functions for each metric. The init function for observables should begin recording the observable, and for non-observables it should set up a oncelocked static variable, which is formatted as `{METRIC_NAME}_{INSTRUMENT_TYPE}`.
