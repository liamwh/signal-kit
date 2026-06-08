use opentelemetry::{KeyValue, metrics::Meter};
use opentelemetry_semantic_conventions::metric::{
    PROCESS_CPU_UTILIZATION, PROCESS_MEMORY_USAGE, PROCESS_MEMORY_VIRTUAL,
};
use sysinfo::{Pid, ProcessesToUpdate};

use super::SYSTEM;

/// Register process metrics
pub fn register_process_metrics(meter: &Meter) {
    let pid = std::process::id();
    let pid = Pid::from_u32(pid);

    // Process CPU utilization
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _process_cpu_utilisation = meter
        .f64_observable_gauge(PROCESS_CPU_UTILIZATION)
        .with_description(
            "Difference in process.cpu.time since the last measurement, divided by the elapsed \
             time and number of CPUs available to the process.",
        )
        .with_unit("1")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(process.cpu_usage() as f64, &[]);
            }
        })
        .build();

    // Process memory usage
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _process_memory_usage = meter
        .i64_observable_up_down_counter(PROCESS_MEMORY_USAGE)
        .with_description("The amount of physical memory in use.")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(
                    process.memory() as i64,
                    &[KeyValue::new("type", "physical")],
                );
            }
        })
        .build();

    // Process virtual memory
    let system_clone = SYSTEM
        .get()
        .expect("SYSTEM Should be initialized in the initialize system metrics functions");
    let _process_memory_limit = meter
        .i64_observable_up_down_counter(PROCESS_MEMORY_VIRTUAL)
        .with_description("The amount of committed virtual memory.")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(process.virtual_memory() as i64, &[]);
            }
        })
        .build();
}
