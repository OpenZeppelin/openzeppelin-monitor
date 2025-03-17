//! Metrics module for the application.
//!
//! - This module contains the global Prometheus registry.
//! - Defines specific metrics for the application.

pub mod server;
use lazy_static::lazy_static;
use prometheus::{Encoder, Gauge, GaugeVec, Opts, Registry, TextEncoder};
use sysinfo::{Disks, System};

lazy_static! {
	// Global Prometheus registry.
	pub static ref REGISTRY: Registry = Registry::new();

	// Gauge for CPU usage percentage.
	pub static ref CPU_USAGE: Gauge = {
	  let gauge = Gauge::new("cpu_usage_percentage", "Current CPU usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	// Gauge for memory usage percentage.
	pub static ref MEMORY_USAGE_PERCENT: Gauge = {
	  let gauge = Gauge::new("memory_usage_percentage", "Memory usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	// Gauge for memory usage in bytes.
	pub static ref MEMORY_USAGE: Gauge = {
		let gauge = Gauge::new("memory_usage_bytes", "Memory usage in bytes").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for total memory in bytes.
	pub static ref TOTAL_MEMORY: Gauge = {
	  let gauge = Gauge::new("total_memory_bytes", "Total memory in bytes").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	// Gauge for available memory in bytes.
	pub static ref AVAILABLE_MEMORY: Gauge = {
		let gauge = Gauge::new("available_memory_bytes", "Available memory in bytes").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for used disk space in bytes.
	pub static ref DISK_USAGE: Gauge = {
	  let gauge = Gauge::new("disk_usage_bytes", "Used disk space in bytes").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	// Gauge for disk usage percentage.
	pub static ref DISK_USAGE_PERCENT: Gauge = {
	  let gauge = Gauge::new("disk_usage_percentage", "Disk usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	// Gauge for total number of monitors (active and paused)
	pub static ref MONITORS_TOTAL: Gauge = {
		let gauge = Gauge::new("monitors_total", "Total number of configured monitors").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for number of active monitors (not paused)
	pub static ref MONITORS_ACTIVE: Gauge = {
		let gauge = Gauge::new("monitors_active", "Number of active monitors").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for total number of triggers
	pub static ref TRIGGERS_TOTAL: Gauge = {
		let gauge = Gauge::new("triggers_total", "Total number of configured triggers").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for total number of contracts being monitored (across all monitors)
	pub static ref CONTRACTS_MONITORED: Gauge = {
		let gauge = Gauge::new("contracts_monitored", "Total number of contracts being monitored").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge for total number of networks being monitored
	pub static ref NETWORKS_MONITORED: Gauge = {
		let gauge = Gauge::new("networks_monitored", "Total number of networks being monitored").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// Gauge Vector for per-network metrics
	pub static ref NETWORK_MONITORS: GaugeVec = {
		let gauge = GaugeVec::new(
			Opts::new("network_monitors", "Number of monitors per network"),
			&["network"]
		).unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};
}

/// Gather all metrics and encode into the provided format.
pub fn gather_metrics() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
	let encoder = TextEncoder::new();
	let metric_families = REGISTRY.gather();
	let mut buffer = Vec::new();
	encoder.encode(&metric_families, &mut buffer)?;
	Ok(buffer)
}

/// Updates the system metrics for CPU and memory usage.
pub fn update_system_metrics() {
	let mut sys = System::new_all();
	sys.refresh_all();

	// Overall CPU usage.
	let cpu_usage = sys.global_cpu_usage();
	CPU_USAGE.set(cpu_usage as f64);

	// Total memory (in bytes).
	let total_memory = sys.total_memory();
	TOTAL_MEMORY.set(total_memory as f64);

	// Available memory (in bytes).
	let available_memory = sys.available_memory();
	AVAILABLE_MEMORY.set(available_memory as f64);

	// Used memory (in bytes).
	let memory_usage = sys.used_memory();
	MEMORY_USAGE.set(memory_usage as f64);

	// Calculate memory usage percentage
	let memory_percentage = if total_memory > 0 {
		(memory_usage as f64 / total_memory as f64) * 100.0
	} else {
		0.0
	};
	MEMORY_USAGE_PERCENT.set(memory_percentage);

	// Calculate disk usage:
	// Sum total space and available space across all disks.
	let disks = Disks::new_with_refreshed_list();
	let mut total_disk_space: u64 = 0;
	let mut total_disk_available: u64 = 0;
	for disk in disks.list() {
		total_disk_space += disk.total_space();
		total_disk_available += disk.available_space();
	}
	// Used disk space is total minus available ( in bytes).
	let used_disk_space = total_disk_space.saturating_sub(total_disk_available);
	DISK_USAGE.set(used_disk_space as f64);

	// Calculate disk usage percentage.
	let disk_percentage = if total_disk_space > 0 {
		(used_disk_space as f64 / total_disk_space as f64) * 100.0
	} else {
		0.0
	};
	DISK_USAGE_PERCENT.set(disk_percentage);
}

/// Updates metrics related to monitors, triggers, networks, and contracts.
pub fn update_monitoring_metrics(
	monitors: &std::collections::HashMap<String, crate::models::Monitor>,
	triggers: &std::collections::HashMap<String, crate::models::Trigger>,
	networks: &std::collections::HashMap<String, crate::models::Network>,
) {
	// Track total and active monitors
	let total_monitors = monitors.len();
	let active_monitors = monitors.values().filter(|m| !m.paused).count();

	MONITORS_TOTAL.set(total_monitors as f64);
	MONITORS_ACTIVE.set(active_monitors as f64);

	// Track total triggers
	TRIGGERS_TOTAL.set(triggers.len() as f64);

	// Count unique contracts across all monitors
	let mut unique_contracts = std::collections::HashSet::new();
	for monitor in monitors.values() {
		for address in &monitor.addresses {
			// Create a unique identifier for each contract (network + address)
			for network in &monitor.networks {
				// Verify the network exists in our network repository
				if networks.contains_key(network) {
					unique_contracts.insert(format!("{}:{}", network, address.address));
				}
			}
		}
	}
	CONTRACTS_MONITORED.set(unique_contracts.len() as f64);

	// Count networks being monitored (those with active monitors)
	let mut networks_with_monitors = std::collections::HashSet::new();
	for monitor in monitors.values().filter(|m| !m.paused) {
		for network in &monitor.networks {
			// Only count networks that exist in our repository
			if networks.contains_key(network) {
				networks_with_monitors.insert(network.clone());
			}
		}
	}
	NETWORKS_MONITORED.set(networks_with_monitors.len() as f64);

	// Reset all network-specific metrics
	NETWORK_MONITORS.reset();

	// Set per-network monitor counts (only for networks that exist)
	let mut network_monitor_counts = std::collections::HashMap::<String, usize>::new();
	for monitor in monitors.values().filter(|m| !m.paused) {
		for network in &monitor.networks {
			if networks.contains_key(network) {
				*network_monitor_counts.entry(network.clone()).or_insert(0) += 1;
			}
		}
	}

	for (network, count) in network_monitor_counts {
		NETWORK_MONITORS
			.with_label_values(&[&network])
			.set(count as f64);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_gather_metrics_contains_expected_names() {
		update_system_metrics();
		let metrics = gather_metrics().expect("failed to gather metrics");
		let output = String::from_utf8(metrics).expect("metrics output is not valid UTF-8");

		assert!(output.contains("cpu_usage_percentage"));
		assert!(output.contains("total_memory_bytes"));
		assert!(output.contains("disk_usage_bytes"));
	}
}
