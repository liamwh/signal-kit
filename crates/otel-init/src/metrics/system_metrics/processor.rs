use opentelemetry::{KeyValue, metrics::Meter};
use opentelemetry_semantic_conventions::metric::{
    SYSTEM_CPU_LOGICAL_COUNT, SYSTEM_CPU_PHYSICAL_COUNT, SYSTEM_CPU_UTILIZATION,
};

use super::SYSTEM;

/// Register CPU count metrics
pub fn register_processor_metrics(meter: &Meter) {
    let _cpu_count = meter
        .i64_observable_up_down_counter(SYSTEM_CPU_LOGICAL_COUNT)
        .with_description("Reports the number of logical (virtual) processor cores")
        .with_unit("{cpu}")
        .with_callback(move |observer| {
            let system = SYSTEM
                .get()
                .expect("SYSTEM Should be initialized in the initialize system metrics functions");
            let sys = system.read();
            observer.observe(sys.cpus().len() as i64, &[]);
        })
        .build();

    let _physical_cpu_count = meter
        .i64_observable_up_down_counter(SYSTEM_CPU_PHYSICAL_COUNT)
        .with_description("Reports the number of actual physical processor cores on the hardware")
        .with_unit("{cpu}")
        .with_callback(move |observer| {
            let physical_count = sysinfo::System::physical_core_count().unwrap_or(1) as i64;
            observer.observe(physical_count, &[]);
        })
        .build();

    let _cpu_usage = meter
        .f64_observable_gauge(SYSTEM_CPU_UTILIZATION)
        .with_description("CPU usage percentage")
        .with_unit("%")
        .with_callback(move |observer| {
            let mut sys = SYSTEM
                .get()
                .expect("SYSTEM Should be initialized in the initialize system metrics functions")
                .write();

            // Global CPU usage
            sys.refresh_cpu_usage();
            let global_usage = sys.global_cpu_usage();
            observer.observe(global_usage as f64, &[KeyValue::new("cpu", "global")]);

            // Per-CPU usage
            for (i, cpu) in sys.cpus().iter().enumerate() {
                observer.observe(
                    cpu.cpu_usage() as f64,
                    &[KeyValue::new("cpu", format!("cpu{i}"))],
                );
            }
        })
        .build();
}
