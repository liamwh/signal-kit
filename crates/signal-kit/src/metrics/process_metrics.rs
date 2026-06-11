use std::{
    sync::{Arc, OnceLock},
    time::SystemTime,
};

use opentelemetry::{KeyValue, metrics::Meter};
use parking_lot::RwLock;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::time;

use crate::OBSERVABILITY_EXPORT_INTERVAL;

/// Global cached system information for process metrics
static SYSTEM: OnceLock<Arc<RwLock<System>>> = OnceLock::new();
static START_TIME: OnceLock<SystemTime> = OnceLock::new();

/// Initialize the process metrics collection service and register observable
/// callbacks
pub fn init_process_metrics(meter: &Meter) {
    // Record process start time
    START_TIME.get_or_init(SystemTime::now);

    // Create and initialize the system info
    let system = Arc::new(RwLock::new(System::new_all()));
    SYSTEM.get_or_init(|| system.clone());

    // Get the current process ID
    let pid = std::process::id();
    let pid = Pid::from_u32(pid);

    // Process CPU time metric
    let system_clone = system.clone();
    let _cpu_time = meter
        .f64_observable_counter("process.cpu.time")
        .with_description("Total CPU seconds broken down by different states")
        .with_unit("s")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                // CPU time in user mode
                observer.observe(
                    process.cpu_usage() as f64,
                    &[KeyValue::new("cpu.mode", "user")],
                );

                // Since sysinfo 0.34.2 doesn't provide separate user/system time,
                // we'll just use the same CPU usage for both for compatibility
                observer.observe(
                    process.cpu_usage() as f64,
                    &[KeyValue::new("cpu.mode", "system")],
                );
            }
        })
        .build();

    // Process CPU utilization
    let system_clone = system.clone();
    let _cpu_utilization = meter
        .f64_observable_gauge("process.cpu.utilization")
        .with_description(
            "Difference in process.cpu.time since the last measurement, divided by the elapsed \
             time and number of CPUs available to the process",
        )
        .with_unit("1")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(
                    process.cpu_usage() as f64 / 100.0, // Convert percentage to ratio [0-1]
                    &[],
                );
            }
        })
        .build();

    // Process memory usage
    let system_clone = system.clone();
    let _memory_usage = meter
        .u64_observable_counter("process.memory.usage")
        .with_description("The amount of physical memory in use")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(process.memory(), &[]);
            }
        })
        .build();

    // Process virtual memory
    let system_clone = system.clone();
    let _virtual_memory = meter
        .u64_observable_counter("process.memory.virtual")
        .with_description("The amount of committed virtual memory")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                observer.observe(process.virtual_memory(), &[]);
            }
        })
        .build();

    // Process disk I/O
    let system_clone = system.clone();
    let _disk_io = meter
        .u64_observable_counter("process.disk.io")
        .with_description("Disk bytes transferred")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut sys = system_clone.write();
            let binding = [pid];
            let processes_to_update = ProcessesToUpdate::Some(&binding);
            sys.refresh_processes(processes_to_update, true);

            if let Some(process) = sys.process(pid) {
                // In sysinfo 0.34.2, disk_usage is a method returning a struct, not an Option
                let disk_usage = process.disk_usage();

                // Read operations
                observer.observe(
                    disk_usage.read_bytes,
                    &[KeyValue::new("disk.io.direction", "read")],
                );

                // Write operations
                observer.observe(
                    disk_usage.written_bytes,
                    &[KeyValue::new("disk.io.direction", "write")],
                );
            }
        })
        .build();

    #[cfg(target_os = "linux")]
    {
        // Process network I/O
        let system_clone = system.clone();
        let _network_io = meter
            .u64_observable_counter("process.network.io")
            .with_description("Network bytes transferred")
            .with_unit("By")
            .with_callback(move |observer| {
                let mut sys = system_clone.write();
                let binding = [pid];
                let processes_to_update = ProcessesToUpdate::Some(&binding);
                sys.refresh_processes(processes_to_update, true);

                if let Some(process) = sys.process(pid) {
                    // On Linux, try to get network info from /proc
                    if let Ok(proc_net) =
                        std::fs::read_to_string(format!("/proc/{}/net/dev", process.pid()))
                    {
                        // Parse network statistics - this is a simplified example
                        let mut received_bytes = 0;
                        let mut transmitted_bytes = 0;

                        // Skip header lines and process interface lines
                        for line in proc_net.lines().skip(2) {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 10 {
                                if let Ok(rx) = parts[1].parse::<u64>() {
                                    received_bytes += rx;
                                }
                                if let Ok(tx) = parts[9].parse::<u64>() {
                                    transmitted_bytes += tx;
                                }
                            }
                        }

                        observer.observe(
                            received_bytes,
                            &[KeyValue::new("network.io.direction", "receive")],
                        );

                        observer.observe(
                            transmitted_bytes,
                            &[KeyValue::new("network.io.direction", "transmit")],
                        );
                    }
                }
            })
            .build();
    }

    // Process thread count
    // let system_clone = system.clone();
    // let _thread_count = meter
    //     .i64_observable_up_down_counter("process.thread.count")
    //     .with_description("Process threads count")
    //     .with_unit("{thread}")
    //     .with_callback(move |observer| {
    //         let mut sys = system_clone.write();
    //         let binding = [pid];
    //         let processes_to_update = ProcessesToUpdate::Some(&binding);
    //         sys.refresh_processes(processes_to_update, true);

    //         if let Some(process) = sys.process(pid) {
    //             // In sysinfo 0.34.2, thread count is accessed differently
    //             observer.observe(process.thread_count() as i64, &[]);
    //         }
    //     })
    //     .build();

    // Process open file descriptor count
    #[cfg(target_os = "linux")]
    {
        let system_clone = system.clone();
        let _open_file_descriptor_count = meter
            .i64_observable_up_down_counter("process.open_file_descriptor.count")
            .with_description("Number of file descriptors in use by the process")
            .with_unit("{file_descriptor}")
            .with_callback(move |observer| {
                let mut sys = system_clone.write();
                let binding = [pid];
                let processes_to_update = ProcessesToUpdate::Some(&binding);
                sys.refresh_processes(processes_to_update, true);

                if let Some(process) = sys.process(pid) {
                    // For portability, we'll try to get the count from /proc if available
                    #[cfg(target_os = "linux")]
                    {
                        if let Ok(count) = std::fs::read_dir(format!("/proc/{}/fd", process.pid()))
                            && let Ok(count) = count.count().try_into()
                        {
                            observer.observe(count, &[]);
                        }
                    }

                    // For other platforms, we might not have direct access to
                    // file descriptor counts
                    // This is a placeholder for cross-platform implementation
                }
            })
            .build();
    }

    // Process context switches
    #[cfg(target_os = "linux")]
    {
        let system_clone = system.clone();
        let _context_switches = meter
            .u64_observable_counter("process.context_switches")
            .with_description("Number of times the process has been context switched")
            .with_unit("{context_switch}")
            .with_callback(move |observer| {
                let mut sys = system_clone.write();
                let binding = [pid];
                let processes_to_update = ProcessesToUpdate::Some(&binding);
                sys.refresh_processes(processes_to_update, true);

                if let Some(process) = sys.process(pid) {
                    // On Linux, we can try to get this information from /proc
                    // This is a simplified implementation
                    if let Ok(stat) =
                        std::fs::read_to_string(format!("/proc/{}/stat", process.pid()))
                    {
                        let parts: Vec<&str> = stat.split_whitespace().collect();

                        // Voluntary context switches is at index 16 (nvcsw)
                        if parts.len() > 16
                            && let Ok(voluntary) = parts[16].parse::<u64>()
                        {
                            observer.observe(
                                voluntary,
                                &[KeyValue::new("process.context_switch_type", "voluntary")],
                            );
                        }

                        // Involuntary context switches is at index 17 (nivcsw)
                        if parts.len() > 17
                            && let Ok(involuntary) = parts[17].parse::<u64>()
                        {
                            observer.observe(
                                involuntary,
                                &[KeyValue::new("process.context_switch_type", "involuntary")],
                            );
                        }
                    }
                }
            })
            .build();
    }

    // Process page faults
    #[cfg(target_os = "linux")]
    {
        let system_clone = system.clone();
        let _page_faults = meter
            .u64_observable_counter("process.paging.faults")
            .with_description("Number of page faults the process has made")
            .with_unit("{fault}")
            .with_callback(move |observer| {
                let mut sys = system_clone.write();
                let binding = [pid];
                let processes_to_update = ProcessesToUpdate::Some(&binding);
                sys.refresh_processes(processes_to_update, true);

                if let Some(process) = sys.process(pid) {
                    // On Linux, we can try to get this information from /proc
                    if let Ok(stat) =
                        std::fs::read_to_string(format!("/proc/{}/stat", process.pid()))
                    {
                        let parts: Vec<&str> = stat.split_whitespace().collect();

                        // Minor page faults is at index 10
                        if parts.len() > 10
                            && let Ok(minor) = parts[10].parse::<u64>()
                        {
                            observer.observe(
                                minor,
                                &[KeyValue::new("process.paging.fault_type", "minor")],
                            );
                        }

                        // Major page faults is at index 12
                        if parts.len() > 12
                            && let Ok(major) = parts[12].parse::<u64>()
                        {
                            observer.observe(
                                major,
                                &[KeyValue::new("process.paging.fault_type", "major")],
                            );
                        }
                    }
                }
            })
            .build();
    }

    // Process uptime
    let _uptime = meter
        .f64_observable_gauge("process.uptime")
        .with_description("The time the process has been running")
        .with_unit("s")
        .with_callback(move |observer| {
            if let Some(start_time) = START_TIME.get()
                && let Ok(duration) = SystemTime::now().duration_since(*start_time)
            {
                observer.observe(duration.as_secs_f64(), &[]);
            }
        })
        .build();

    // Start a background task to periodically refresh system information
    tokio::spawn(async move {
        let mut interval = time::interval(OBSERVABILITY_EXPORT_INTERVAL);
        loop {
            interval.tick().await;
            if let Some(system) = SYSTEM.get() {
                let mut sys = system.write();
                // In sysinfo 0.34.2, use refresh_all to refresh everything
                sys.refresh_all();
            }
        }
    });
}

/// Simple, synchronous access to system information for process metrics
pub fn get_system() -> Arc<RwLock<System>> {
    SYSTEM
        .get()
        .expect("Process metrics not initialized")
        .clone()
}
