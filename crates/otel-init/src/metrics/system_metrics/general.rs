use std::{sync::OnceLock, time::SystemTime};

use opentelemetry::metrics::Meter;
use opentelemetry_semantic_conventions::metric::SYSTEM_UPTIME;

static START_TIME: OnceLock<SystemTime> = OnceLock::new();

/// Register system uptime metric
pub fn register_uptime_metric(meter: &Meter) {
    // Record system start time
    START_TIME.get_or_init(SystemTime::now);

    let _uptime = meter
        .i64_observable_gauge(SYSTEM_UPTIME)
        .with_description("The time the system has been running")
        .with_unit("s")
        .with_callback(move |observer| {
            if let Some(start_time) = START_TIME.get()
                && let Ok(duration) = SystemTime::now().duration_since(*start_time)
            {
                observer.observe(duration.as_secs() as i64, &[]);
            }
        })
        .build();
}
