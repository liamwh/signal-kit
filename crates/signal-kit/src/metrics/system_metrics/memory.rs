use opentelemetry::{KeyValue, metrics::Meter};
use opentelemetry_semantic_conventions::{
    attribute::SYSTEM_MEMORY_STATE,
    metric::{SYSTEM_MEMORY_LIMIT, SYSTEM_MEMORY_USAGE, SYSTEM_MEMORY_UTILIZATION},
};

use super::SYSTEM;

/// Register memory metrics
pub fn register_memory_metrics(meter: &Meter) {
    // Memory usage
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _memory_usage = meter
        .i64_observable_up_down_counter(SYSTEM_MEMORY_USAGE)
        .with_description("Reports memory in use by state")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            sys.refresh_memory();

            // Used memory
            observer.observe(
                sys.used_memory() as i64,
                &[KeyValue::new(SYSTEM_MEMORY_STATE, "used")],
            );

            // Free memory
            observer.observe(
                sys.free_memory() as i64,
                &[KeyValue::new(SYSTEM_MEMORY_STATE, "free")],
            );

            // Available memory (Linux specific, but we'll use it generally)
            observer.observe(
                sys.available_memory() as i64,
                &[KeyValue::new(SYSTEM_MEMORY_STATE, "available")],
            );
        })
        .build();

    // Memory limit/total
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _memory_limit = meter
        .u64_observable_gauge(SYSTEM_MEMORY_LIMIT)
        .with_description("Total memory available in the system")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            sys.refresh_memory();
            observer.observe(sys.total_memory(), &[]);
        })
        .build();

    // Memory utilization (as a ratio between 0-1)
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _memory_utilization = meter
        .i64_observable_up_down_counter(SYSTEM_MEMORY_UTILIZATION)
        .with_description("Reports memory in use by state.")
        .with_unit("1")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            sys.refresh_memory();

            let total = sys.total_memory() as i64;
            if total > 0 {
                // Used memory utilization
                let used_ratio = sys.used_memory() as i64 / total;
                observer.observe(used_ratio, &[KeyValue::new(SYSTEM_MEMORY_STATE, "used")]);

                // Free memory utilization
                let free_ratio = sys.free_memory() as i64 / total;
                observer.observe(free_ratio, &[KeyValue::new(SYSTEM_MEMORY_STATE, "free")]);

                let available_ratio = sys.available_memory() as i64 / total;
                observer.observe(
                    available_ratio,
                    &[KeyValue::new(SYSTEM_MEMORY_STATE, "available")],
                );
            }
        })
        .build();

    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _memory_limit = meter
        .i64_observable_up_down_counter(SYSTEM_MEMORY_LIMIT)
        .with_description("Total memory available in the system")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            sys.refresh_memory();
            observer.observe(sys.total_memory() as i64, &[]);
        })
        .build();
}
