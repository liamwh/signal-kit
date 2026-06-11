mod disk;
mod general;
mod memory;
mod network;
mod process;
mod processor;

use std::sync::{Arc, OnceLock};

use disk::*;
use general::*;
use memory::*;
use network::*;
use opentelemetry::metrics::Meter;
use parking_lot::RwLock;
use process::*;
use processor::*;
use sysinfo::{Disks, Networks, System};
use tokio::time;

use crate::OBSERVABILITY_EXPORT_INTERVAL;

/// Global cached system information
pub static SYSTEM: OnceLock<Arc<RwLock<System>>> = OnceLock::new();

/// Global cached disk information
pub static DISKS: OnceLock<Arc<RwLock<Disks>>> = OnceLock::new();

/// Global cached network information
pub static NETWORKS: OnceLock<Arc<RwLock<Networks>>> = OnceLock::new();

/// Initialize the system metrics collection service and register observable
/// callbacks
pub fn init_system_metrics(meter: &Meter) {
    let system = Arc::new(RwLock::new(System::new_all()));
    SYSTEM.get_or_init(|| system.clone());

    let disks = Arc::new(RwLock::new(Disks::new_with_refreshed_list()));
    DISKS.get_or_init(|| disks.clone());

    let networks = Arc::new(RwLock::new(Networks::new_with_refreshed_list()));
    NETWORKS.get_or_init(|| networks.clone());

    // General Metrics
    register_uptime_metric(meter);

    // Processor Metrics
    register_processor_metrics(meter);

    // Memory Metrics
    register_memory_metrics(meter);

    // Disk Metrics
    register_disk_metrics(meter);

    // Network Metrics
    register_network_metrics(meter);

    // Process Metrics
    register_process_metrics(meter);

    // Start a background task to periodically refresh system information
    start_refresh_task();
}

/// Start background task to periodically refresh system information
fn start_refresh_task() {
    tokio::spawn(async move {
        let mut interval = time::interval(OBSERVABILITY_EXPORT_INTERVAL);
        loop {
            interval.tick().await;
            if let Some(system) = SYSTEM.get() {
                let mut sys = system.write();
                sys.refresh_all();
            }
            if let Some(disks) = DISKS.get() {
                let mut d = disks.write();
                d.refresh(true);
            }
            if let Some(networks) = NETWORKS.get() {
                let mut n = networks.write();
                n.refresh(true);
            }
        }
    });
}
