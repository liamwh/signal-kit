use opentelemetry::{KeyValue, metrics::Meter};
use opentelemetry_semantic_conventions::{
    attribute::{NETWORK_INTERFACE_NAME, NETWORK_IO_DIRECTION},
    metric::{SYSTEM_NETWORK_ERRORS, SYSTEM_NETWORK_IO, SYSTEM_NETWORK_PACKETS},
};

use super::NETWORKS;

/// Register network metrics
pub fn register_network_metrics(meter: &Meter) {
    // Network IO
    let networks_clone = NETWORKS
        .get()
        .expect("NETWORKS Should be initialized in the initialize system metrics functions");
    let _network_io = meter
        .u64_observable_counter(SYSTEM_NETWORK_IO)
        .with_description("Network IO bytes")
        .with_unit("By")
        .with_callback(move |observer| {
            let mut networks = networks_clone.write();
            networks.refresh(true);

            for (interface_name, network) in networks.iter() {
                // Received bytes
                observer.observe(
                    network.received(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "receive"),
                    ],
                );

                // Transmitted bytes
                observer.observe(
                    network.transmitted(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "transmit"),
                    ],
                );
            }
        })
        .build();

    // Network packets
    let networks_clone = NETWORKS
        .get()
        .expect("NETWORKS Should be initialized in the initialize system metrics functions");
    let _network_packets = meter
        .u64_observable_gauge(SYSTEM_NETWORK_PACKETS)
        .with_description("Network packets")
        .with_unit("{packet}")
        .with_callback(move |observer| {
            let networks = networks_clone.read();

            for (interface_name, network) in networks.iter() {
                // Received packets
                observer.observe(
                    network.packets_received(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "receive"),
                    ],
                );

                // Transmitted packets
                observer.observe(
                    network.packets_transmitted(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "transmit"),
                    ],
                );
            }
        })
        .build();

    // Network errors
    let networks_clone = NETWORKS
        .get()
        .expect("NETWORKS Should be initialized in the initialize system metrics functions");
    let _network_errors = meter
        .u64_observable_gauge(SYSTEM_NETWORK_ERRORS)
        .with_description("Network errors")
        .with_unit("{error}")
        .with_callback(move |observer| {
            let networks = networks_clone.read();

            for (interface_name, network) in networks.iter() {
                // Received errors
                observer.observe(
                    network.errors_on_received(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "receive"),
                    ],
                );

                // Transmitted errors
                observer.observe(
                    network.errors_on_transmitted(),
                    &[
                        KeyValue::new(NETWORK_INTERFACE_NAME, interface_name.clone()),
                        KeyValue::new(NETWORK_IO_DIRECTION, "transmit"),
                    ],
                );
            }
        })
        .build();
}
