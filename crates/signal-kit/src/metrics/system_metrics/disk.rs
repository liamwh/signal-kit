use opentelemetry::{KeyValue, metrics::Meter};
use opentelemetry_semantic_conventions::{
    attribute::{SYSTEM_DEVICE, SYSTEM_FILESYSTEM_MOUNTPOINT, SYSTEM_FILESYSTEM_STATE},
    metric::{SYSTEM_FILESYSTEM_LIMIT, SYSTEM_FILESYSTEM_USAGE, SYSTEM_FILESYSTEM_UTILIZATION},
};

use super::DISKS;

/// Register disk metrics
pub fn register_disk_metrics(meter: &Meter) {
    // Disk usage
    let disks_clone = DISKS
        .get()
        .expect("DISKS Should be initialized in the initialize system metrics functions");

    let _disk_usage = meter
        .u64_observable_gauge(SYSTEM_FILESYSTEM_USAGE)
        .with_description("Reports a filesystem's space usage across different states")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut disks = disks_clone.write();
            disks.refresh(true);

            for disk in disks.list() {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);
                let mount_point = disk.mount_point().to_string_lossy().into_owned();
                let device_name = disk.name().to_string_lossy().into_owned();

                // Used space
                observer.observe(
                    used,
                    &[
                        KeyValue::new(SYSTEM_DEVICE, device_name.clone()),
                        KeyValue::new(SYSTEM_FILESYSTEM_MOUNTPOINT, mount_point.clone()),
                        KeyValue::new(SYSTEM_FILESYSTEM_STATE, "used"),
                    ],
                );

                // Free space
                observer.observe(
                    available,
                    &[
                        KeyValue::new(SYSTEM_DEVICE, device_name.clone()),
                        KeyValue::new(SYSTEM_FILESYSTEM_MOUNTPOINT, mount_point.clone()),
                        KeyValue::new(SYSTEM_FILESYSTEM_STATE, "free"),
                    ],
                );
            }
        })
        .build();

    // Filesystem utilization
    let disks_clone = DISKS
        .get()
        .expect("DISKS Should be initialized in the initialize system metrics functions");

    let _filesystem_utilization = meter
        .f64_observable_gauge(SYSTEM_FILESYSTEM_UTILIZATION)
        .with_description("Filesystem utilization as a ratio")
        .with_unit("1")
        .with_callback(move |observer| {
            let mut disks = disks_clone.write();
            disks.refresh(true);

            for disk in disks.list() {
                let total = disk.total_space() as f64;
                if total > 0.0 {
                    let available = disk.available_space() as f64;
                    let used = total - available;
                    let used_ratio = used / total;
                    let free_ratio = available / total;

                    let mount_point = disk.mount_point().to_string_lossy().into_owned();
                    let device_name = disk.name().to_string_lossy().into_owned();

                    // Used ratio
                    observer.observe(
                        used_ratio,
                        &[
                            KeyValue::new(SYSTEM_DEVICE, device_name.clone()),
                            KeyValue::new(SYSTEM_FILESYSTEM_MOUNTPOINT, mount_point.clone()),
                            KeyValue::new(SYSTEM_FILESYSTEM_STATE, "used"),
                        ],
                    );

                    // Free ratio
                    observer.observe(
                        free_ratio,
                        &[
                            KeyValue::new(SYSTEM_DEVICE, device_name.clone()),
                            KeyValue::new(SYSTEM_FILESYSTEM_MOUNTPOINT, mount_point.clone()),
                            KeyValue::new(SYSTEM_FILESYSTEM_STATE, "free"),
                        ],
                    );
                }
            }
        })
        .build();

    // Filesystem limit
    let disks_clone = DISKS
        .get()
        .expect("DISKS Should be initialized in the initialize system metrics functions");

    let _filesystem_limit = meter
        .u64_observable_gauge(SYSTEM_FILESYSTEM_LIMIT)
        .with_description("The total storage capacity of the filesystem")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut disks = disks_clone.write();
            disks.refresh(true);

            for disk in disks.list() {
                observer.observe(
                    disk.total_space(),
                    &[
                        KeyValue::new(SYSTEM_DEVICE, disk.name().to_string_lossy().into_owned()),
                        KeyValue::new(
                            SYSTEM_FILESYSTEM_MOUNTPOINT,
                            disk.mount_point().to_string_lossy().into_owned(),
                        ),
                    ],
                );
            }
        })
        .build();
}
