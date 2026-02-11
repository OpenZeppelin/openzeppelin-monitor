//! Metrics module for the application.
//!
//! - This module contains the global Prometheus registry.
//! - Defines specific metrics for the application.

pub mod server;
use lazy_static::lazy_static;
use prometheus::{
	CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder,
};
use sysinfo::{Disks, System};

lazy_static! {
	/// Global Prometheus registry.
	///
	/// This registry holds all metrics defined in this module and is used
	/// to gather metrics for exposure via the metrics endpoint.
	pub static ref REGISTRY: Registry = Registry::new();

	/// Gauge for CPU usage percentage.
	///
	/// Tracks the current CPU usage as a percentage (0-100) across all cores.
	pub static ref CPU_USAGE: Gauge = {
	  let gauge = Gauge::new("cpu_usage_percentage", "Current CPU usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	/// Gauge for memory usage percentage.
	///
	/// Tracks the percentage (0-100) of total system memory currently in use.
	pub static ref MEMORY_USAGE_PERCENT: Gauge = {
	  let gauge = Gauge::new("memory_usage_percentage", "Memory usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	/// Gauge for memory usage in bytes.
	///
	/// Tracks the absolute amount of memory currently in use by the system in bytes.
	pub static ref MEMORY_USAGE: Gauge = {
		let gauge = Gauge::new("memory_usage_bytes", "Memory usage in bytes").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for total memory in bytes.
	///
	/// Tracks the total amount of physical memory available on the system in bytes.
	pub static ref TOTAL_MEMORY: Gauge = {
	  let gauge = Gauge::new("total_memory_bytes", "Total memory in bytes").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	/// Gauge for available memory in bytes.
	///
	/// Tracks the amount of memory currently available for allocation in bytes.
	pub static ref AVAILABLE_MEMORY: Gauge = {
		let gauge = Gauge::new("available_memory_bytes", "Available memory in bytes").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for used disk space in bytes.
	///
	/// Tracks the total amount of disk space currently in use across all mounted filesystems in bytes.
	pub static ref DISK_USAGE: Gauge = {
	  let gauge = Gauge::new("disk_usage_bytes", "Used disk space in bytes").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	/// Gauge for disk usage percentage.
	///
	/// Tracks the percentage (0-100) of total disk space currently in use across all mounted filesystems.
	pub static ref DISK_USAGE_PERCENT: Gauge = {
	  let gauge = Gauge::new("disk_usage_percentage", "Disk usage percentage").unwrap();
	  REGISTRY.register(Box::new(gauge.clone())).unwrap();
	  gauge
	};

	/// Gauge for total number of monitors (active and paused).
	///
	/// Tracks the total count of all configured monitors in the system, regardless of their active state.
	pub static ref MONITORS_TOTAL: Gauge = {
		let gauge = Gauge::new("monitors_total", "Total number of configured monitors").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for number of active monitors (not paused).
	///
	/// Tracks the count of monitors that are currently active (not in paused state).
	pub static ref MONITORS_ACTIVE: Gauge = {
		let gauge = Gauge::new("monitors_active", "Number of active monitors").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for total number of triggers.
	///
	/// Tracks the total count of all configured triggers in the system.
	pub static ref TRIGGERS_TOTAL: Gauge = {
		let gauge = Gauge::new("triggers_total", "Total number of configured triggers").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for total number of contracts being monitored (across all monitors).
	///
	/// Tracks the total count of unique contracts (network + address combinations) being monitored.
	pub static ref CONTRACTS_MONITORED: Gauge = {
		let gauge = Gauge::new("contracts_monitored", "Total number of contracts being monitored").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge for total number of networks being monitored.
	///
	/// Tracks the count of unique blockchain networks that have at least one active monitor.
	pub static ref NETWORKS_MONITORED: Gauge = {
		let gauge = Gauge::new("networks_monitored", "Total number of networks being monitored").unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	/// Gauge Vector for per-network metrics.
	///
	/// Tracks the number of active monitors for each network, with the network name as a label.
	pub static ref NETWORK_MONITORS: GaugeVec = {
		let gauge = GaugeVec::new(
			Opts::new("network_monitors", "Number of monitors per network"),
			&["network"]
		).unwrap();
		REGISTRY.register(Box::new(gauge.clone())).unwrap();
		gauge
	};

	// ============================================================
	// RPC Operational Metrics
	// ============================================================

	/// Counter for total RPC requests.
	///
	/// Tracks the total number of RPC requests made, labeled by network and method.
	pub static ref RPC_REQUESTS_TOTAL: CounterVec = {
		let counter = CounterVec::new(
			Opts::new("rpc_requests_total", "Total number of RPC requests"),
			&["network", "method"]
		).unwrap();
		REGISTRY.register(Box::new(counter.clone())).unwrap();
		counter
	};

	/// Counter for RPC request errors.
	///
	/// Tracks the total number of failed RPC requests, labeled by network, HTTP status code, and error type.
	pub static ref RPC_REQUEST_ERRORS_TOTAL: CounterVec = {
		let counter = CounterVec::new(
			Opts::new("rpc_request_errors_total", "Total number of RPC request errors"),
			&["network", "status_code", "error_type"]
		).unwrap();
		REGISTRY.register(Box::new(counter.clone())).unwrap();
		counter
	};

	/// Histogram for RPC request duration.
	///
	/// Tracks the duration of successful RPC requests in seconds, labeled by network.
	pub static ref RPC_REQUEST_DURATION_SECONDS: HistogramVec = {
		let histogram = HistogramVec::new(
			HistogramOpts::new("rpc_request_duration_seconds", "RPC request duration in seconds")
				.buckets(vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
			&["network"]
		).unwrap();
		REGISTRY.register(Box::new(histogram.clone())).unwrap();
		histogram
	};

	/// Counter for RPC endpoint rotations.
	///
	/// Tracks the total number of endpoint rotations, labeled by network and reason.
	pub static ref RPC_ENDPOINT_ROTATIONS_TOTAL: CounterVec = {
		let counter = CounterVec::new(
			Opts::new("rpc_endpoint_rotations_total", "Total number of RPC endpoint rotations"),
			&["network", "reason"]
		).unwrap();
		REGISTRY.register(Box::new(counter.clone())).unwrap();
		counter
	};

	/// Counter for RPC rate limit responses (HTTP 429).
	///
	/// Tracks the total number of rate limit responses received, labeled by network and endpoint.
	pub static ref RPC_RATE_LIMITS_TOTAL: CounterVec = {
		let counter = CounterVec::new(
			Opts::new("rpc_rate_limits_total", "Total number of RPC rate limit responses (HTTP 429)"),
			&["network", "endpoint"]
		).unwrap();
		REGISTRY.register(Box::new(counter.clone())).unwrap();
		counter
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

// ============================================================
// RPC Metrics Helper Functions
// ============================================================

/// Records an RPC request.
///
/// # Arguments
/// * `network` - The network slug (e.g., "ethereum", "polygon")
/// * `method` - The RPC method name (e.g., "eth_getBlockByNumber")
pub fn record_rpc_request(network: &str, method: &str) {
	RPC_REQUESTS_TOTAL
		.with_label_values(&[network, method])
		.inc();
}

/// Records an RPC request error.
///
/// # Arguments
/// * `network` - The network slug
/// * `status_code` - The HTTP status code as a string (e.g., "429", "500", or "0" for network errors)
/// * `error_type` - The type of error (e.g., "http", "network", "timeout")
pub fn record_rpc_error(network: &str, status_code: &str, error_type: &str) {
	RPC_REQUEST_ERRORS_TOTAL
		.with_label_values(&[network, status_code, error_type])
		.inc();
}

/// Observes the duration of an RPC request.
///
/// # Arguments
/// * `network` - The network slug
/// * `duration_secs` - The request duration in seconds
pub fn observe_rpc_duration(network: &str, duration_secs: f64) {
	RPC_REQUEST_DURATION_SECONDS
		.with_label_values(&[network])
		.observe(duration_secs);
}

/// Records an RPC endpoint rotation event.
///
/// # Arguments
/// * `network` - The network slug
/// * `reason` - The reason for rotation (e.g., "rate_limit", "network_error")
pub fn record_endpoint_rotation(network: &str, reason: &str) {
	RPC_ENDPOINT_ROTATIONS_TOTAL
		.with_label_values(&[network, reason])
		.inc();
}

/// Records an RPC rate limit response (HTTP 429).
///
/// # Arguments
/// * `network` - The network slug
/// * `endpoint` - The endpoint URL that returned the rate limit
pub fn record_rate_limit(network: &str, endpoint: &str) {
	RPC_RATE_LIMITS_TOTAL
		.with_label_values(&[network, endpoint])
		.inc();
}

/// Initializes RPC metrics for a network so they appear in Prometheus output with 0 values.
///
/// This should be called when a transport client is created for a network.
/// Ensures metrics are visible in dashboards even before any errors occur.
///
/// # Arguments
/// * `network` - The network slug
pub fn init_rpc_metrics_for_network(network: &str) {
	// Initialize error counters with common status codes
	// These will show as 0 until actual errors occur
	RPC_REQUEST_ERRORS_TOTAL
		.with_label_values(&[network, "429", "http"])
		.inc_by(0.0);
	RPC_REQUEST_ERRORS_TOTAL
		.with_label_values(&[network, "500", "http"])
		.inc_by(0.0);
	RPC_REQUEST_ERRORS_TOTAL
		.with_label_values(&[network, "0", "network"])
		.inc_by(0.0);

	// Initialize rotation counters
	RPC_ENDPOINT_ROTATIONS_TOTAL
		.with_label_values(&[network, "rate_limit"])
		.inc_by(0.0);
	RPC_ENDPOINT_ROTATIONS_TOTAL
		.with_label_values(&[network, "network_error"])
		.inc_by(0.0);
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{BlockChainType, Monitor, Network, TransactionStatus, Trigger},
		utils::tests::builders::{
			evm::monitor::MonitorBuilder, network::NetworkBuilder, trigger::TriggerBuilder,
		},
	};
	use std::collections::HashMap;
	use std::sync::Mutex;

	// Use a mutex to ensure tests don't run in parallel when they modify shared state
	lazy_static! {
		static ref TEST_MUTEX: Mutex<()> = Mutex::new(());
	}

	// Reset all metrics to a known state
	fn reset_all_metrics() {
		// System metrics
		CPU_USAGE.set(0.0);
		MEMORY_USAGE.set(0.0);
		MEMORY_USAGE_PERCENT.set(0.0);
		TOTAL_MEMORY.set(0.0);
		AVAILABLE_MEMORY.set(0.0);
		DISK_USAGE.set(0.0);
		DISK_USAGE_PERCENT.set(0.0);

		// Monitoring metrics
		MONITORS_TOTAL.set(0.0);
		MONITORS_ACTIVE.set(0.0);
		TRIGGERS_TOTAL.set(0.0);
		CONTRACTS_MONITORED.set(0.0);
		NETWORKS_MONITORED.set(0.0);
		NETWORK_MONITORS.reset();

		// RPC metrics
		RPC_REQUESTS_TOTAL.reset();
		RPC_REQUEST_ERRORS_TOTAL.reset();
		RPC_REQUEST_DURATION_SECONDS.reset();
		RPC_ENDPOINT_ROTATIONS_TOTAL.reset();
		RPC_RATE_LIMITS_TOTAL.reset();
	}

	// Helper function to create a test network
	fn create_test_network(slug: &str, name: &str, chain_id: u64) -> Network {
		NetworkBuilder::new()
			.name(name)
			.slug(slug)
			.network_type(BlockChainType::EVM)
			.chain_id(chain_id)
			.rpc_url(&format!("https://{}.example.com", slug))
			.block_time_ms(15000)
			.confirmation_blocks(12)
			.cron_schedule("*/15 * * * * *")
			.max_past_blocks(1000)
			.store_blocks(true)
			.build()
	}

	// Helper function to create a test monitor
	fn create_test_monitor(
		name: &str,
		networks: Vec<String>,
		addresses: Vec<String>,
		paused: bool,
	) -> Monitor {
		MonitorBuilder::new()
			.name(name)
			.networks(networks)
			.paused(paused)
			.addresses(addresses)
			.function("transfer(address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.build()
	}

	fn create_test_trigger(name: &str) -> Trigger {
		TriggerBuilder::new()
			.name(name)
			.email(
				"smtp.example.com",
				"user@example.com",
				"password123",
				"alerts@example.com",
				vec!["user@example.com"],
			)
			.message("Alert", "Something happened!")
			.build()
	}

	#[test]
	fn test_gather_metrics_contains_expected_names() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Initialize all metrics with non-zero values to ensure they appear in output
		CPU_USAGE.set(50.0);
		MEMORY_USAGE_PERCENT.set(60.0);
		MEMORY_USAGE.set(1024.0);
		TOTAL_MEMORY.set(2048.0);
		AVAILABLE_MEMORY.set(1024.0);
		DISK_USAGE.set(512.0);
		DISK_USAGE_PERCENT.set(25.0);
		MONITORS_TOTAL.set(5.0);
		MONITORS_ACTIVE.set(3.0);
		TRIGGERS_TOTAL.set(2.0);
		CONTRACTS_MONITORED.set(4.0);
		NETWORKS_MONITORED.set(2.0);
		NETWORK_MONITORS.with_label_values(&["test"]).set(1.0);

		// Initialize RPC metrics
		RPC_REQUESTS_TOTAL
			.with_label_values(&["ethereum", "eth_getBlockByNumber"])
			.inc();
		RPC_REQUEST_ERRORS_TOTAL
			.with_label_values(&["ethereum", "429", "http"])
			.inc();
		RPC_REQUEST_DURATION_SECONDS
			.with_label_values(&["ethereum"])
			.observe(0.5);
		RPC_ENDPOINT_ROTATIONS_TOTAL
			.with_label_values(&["ethereum", "rate_limit"])
			.inc();
		RPC_RATE_LIMITS_TOTAL
			.with_label_values(&["ethereum", "https://rpc.example.com"])
			.inc();

		let metrics = gather_metrics().expect("failed to gather metrics");
		let output = String::from_utf8(metrics).expect("metrics output is not valid UTF-8");

		// Check for system metrics
		assert!(output.contains("cpu_usage_percentage"));
		assert!(output.contains("memory_usage_percentage"));
		assert!(output.contains("memory_usage_bytes"));
		assert!(output.contains("total_memory_bytes"));
		assert!(output.contains("available_memory_bytes"));
		assert!(output.contains("disk_usage_bytes"));
		assert!(output.contains("disk_usage_percentage"));

		// Check for monitoring metrics
		assert!(output.contains("monitors_total"));
		assert!(output.contains("monitors_active"));
		assert!(output.contains("triggers_total"));
		assert!(output.contains("contracts_monitored"));
		assert!(output.contains("networks_monitored"));
		assert!(output.contains("network_monitors"));

		// Check for RPC metrics
		assert!(output.contains("rpc_requests_total"));
		assert!(output.contains("rpc_request_errors_total"));
		assert!(output.contains("rpc_request_duration_seconds"));
		assert!(output.contains("rpc_endpoint_rotations_total"));
		assert!(output.contains("rpc_rate_limits_total"));
	}

	#[test]
	fn test_system_metrics_update() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Update metrics
		update_system_metrics();

		// Verify metrics were updated with reasonable values
		let cpu_usage = CPU_USAGE.get();
		assert!((0.0..=100.0).contains(&cpu_usage));

		let memory_usage = MEMORY_USAGE.get();
		assert!(memory_usage >= 0.0);

		let memory_percent = MEMORY_USAGE_PERCENT.get();
		assert!((0.0..=100.0).contains(&memory_percent));

		let total_memory = TOTAL_MEMORY.get();
		assert!(total_memory > 0.0);

		let expected_percentage = if total_memory > 0.0 {
			(memory_usage / total_memory) * 100.0
		} else {
			0.0
		};
		assert_eq!(memory_percent, expected_percentage);

		let available_memory = AVAILABLE_MEMORY.get();
		assert!(available_memory >= 0.0);

		let disk_usage = DISK_USAGE.get();
		assert!(disk_usage >= 0.0);

		let disk_percent = DISK_USAGE_PERCENT.get();
		assert!((0.0..=100.0).contains(&disk_percent));

		// Verify that memory usage doesn't exceed total memory
		assert!(memory_usage <= total_memory);

		// Verify that available memory doesn't exceed total memory
		assert!(available_memory <= total_memory);
	}

	#[test]
	fn test_monitoring_metrics_update() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Create test data
		let mut monitors = HashMap::new();
		let mut networks = HashMap::new();
		let triggers = HashMap::new();

		// Add test networks
		networks.insert(
			"ethereum".to_string(),
			create_test_network("ethereum", "Ethereum", 1),
		);
		networks.insert(
			"polygon".to_string(),
			create_test_network("polygon", "Polygon", 137),
		);
		networks.insert(
			"arbitrum".to_string(),
			create_test_network("arbitrum", "Arbitrum", 42161),
		);

		// Add test monitors
		monitors.insert(
			"monitor1".to_string(),
			create_test_monitor(
				"Test Monitor 1",
				vec!["ethereum".to_string()],
				vec!["0x1234567890123456789012345678901234567890".to_string()],
				false,
			),
		);

		monitors.insert(
			"monitor2".to_string(),
			create_test_monitor(
				"Test Monitor 2",
				vec!["polygon".to_string(), "ethereum".to_string()],
				vec!["0x0987654321098765432109876543210987654321".to_string()],
				true,
			),
		);

		monitors.insert(
			"monitor3".to_string(),
			create_test_monitor(
				"Test Monitor 3",
				vec!["arbitrum".to_string()],
				vec![
					"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
					"0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
				],
				false,
			),
		);

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify metrics
		assert_eq!(MONITORS_TOTAL.get(), 3.0);
		assert_eq!(MONITORS_ACTIVE.get(), 2.0);
		assert_eq!(TRIGGERS_TOTAL.get(), 0.0);
		assert_eq!(CONTRACTS_MONITORED.get(), 5.0);
		assert_eq!(NETWORKS_MONITORED.get(), 2.0);

		// Check network-specific metrics
		let ethereum_monitors = NETWORK_MONITORS
			.get_metric_with_label_values(&["ethereum"])
			.unwrap();
		assert_eq!(ethereum_monitors.get(), 1.0);

		let polygon_monitors = NETWORK_MONITORS
			.get_metric_with_label_values(&["polygon"])
			.unwrap();
		assert_eq!(polygon_monitors.get(), 0.0);

		let arbitrum_monitors = NETWORK_MONITORS
			.get_metric_with_label_values(&["arbitrum"])
			.unwrap();
		assert_eq!(arbitrum_monitors.get(), 1.0);
	}

	#[test]
	fn test_nonexistent_networks_are_ignored() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Create test data with a monitor referencing a non-existent network
		let mut monitors = HashMap::new();
		let mut networks = HashMap::new();
		let triggers = HashMap::new();

		networks.insert(
			"ethereum".to_string(),
			create_test_network("ethereum", "Ethereum", 1),
		);

		monitors.insert(
			"monitor1".to_string(),
			create_test_monitor(
				"Test Monitor 1",
				vec!["ethereum".to_string(), "nonexistent_network".to_string()],
				vec!["0x1234567890123456789012345678901234567890".to_string()],
				false,
			),
		);

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify metrics
		assert_eq!(NETWORKS_MONITORED.get(), 1.0);
		assert_eq!(CONTRACTS_MONITORED.get(), 1.0);

		// The nonexistent network should not have a metric
		let nonexistent = NETWORK_MONITORS.get_metric_with_label_values(&["nonexistent_network"]);
		assert!(nonexistent.is_err() || nonexistent.unwrap().get() == 0.0);
	}

	#[test]
	fn test_multiple_monitors_same_network() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Create test data with multiple monitors on the same network
		let mut monitors = HashMap::new();
		let mut networks = HashMap::new();
		let triggers = HashMap::new();

		networks.insert(
			"ethereum".to_string(),
			create_test_network("ethereum", "Ethereum", 1),
		);

		// Add three monitors all watching Ethereum
		monitors.insert(
			"monitor1".to_string(),
			create_test_monitor(
				"Test Monitor 1",
				vec!["ethereum".to_string()],
				vec!["0x1111111111111111111111111111111111111111".to_string()],
				false,
			),
		);

		monitors.insert(
			"monitor2".to_string(),
			create_test_monitor(
				"Test Monitor 2",
				vec!["ethereum".to_string()],
				vec!["0x2222222222222222222222222222222222222222".to_string()],
				false,
			),
		);

		monitors.insert(
			"monitor3".to_string(),
			create_test_monitor(
				"Test Monitor 3",
				vec!["ethereum".to_string()],
				vec!["0x3333333333333333333333333333333333333333".to_string()],
				true, // This one is paused
			),
		);

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify metrics
		assert_eq!(MONITORS_TOTAL.get(), 3.0);
		assert_eq!(MONITORS_ACTIVE.get(), 2.0);
		assert_eq!(NETWORKS_MONITORED.get(), 1.0);

		// Check network-specific metrics
		let ethereum_monitors = NETWORK_MONITORS
			.get_metric_with_label_values(&["ethereum"])
			.unwrap();
		assert_eq!(ethereum_monitors.get(), 2.0);
	}

	#[test]
	fn test_multiple_contracts_per_monitor() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Create test data with a monitor watching multiple contracts
		let mut monitors = HashMap::new();
		let mut networks = HashMap::new();
		let triggers = HashMap::new();

		networks.insert(
			"ethereum".to_string(),
			create_test_network("ethereum", "Ethereum", 1),
		);

		// Add a monitor watching multiple contracts
		monitors.insert(
			"monitor1".to_string(),
			create_test_monitor(
				"Test Monitor 1",
				vec!["ethereum".to_string()],
				vec![
					"0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
					"0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
					"0xcccccccccccccccccccccccccccccccccccccccc".to_string(),
				],
				false,
			),
		);

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify metrics
		assert_eq!(CONTRACTS_MONITORED.get(), 3.0);
	}

	#[test]
	fn test_triggers_count() {
		let _lock = TEST_MUTEX
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner());
		reset_all_metrics();

		// Create test data with triggers
		let monitors = HashMap::new();
		let networks = HashMap::new();
		let mut triggers = HashMap::new();

		// Add some triggers
		triggers.insert("trigger1".to_string(), create_test_trigger("trigger1"));
		triggers.insert("trigger2".to_string(), create_test_trigger("trigger2"));
		triggers.insert("trigger3".to_string(), create_test_trigger("trigger3"));

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify metrics
		let total_triggers = TRIGGERS_TOTAL.get();
		assert_eq!(total_triggers, 3.0);

		// Verify other metrics are zero since we have no monitors or networks
		assert_eq!(MONITORS_TOTAL.get(), 0.0);
		assert_eq!(MONITORS_ACTIVE.get(), 0.0);
		assert_eq!(CONTRACTS_MONITORED.get(), 0.0);
		assert_eq!(NETWORKS_MONITORED.get(), 0.0);
	}

	#[test]
	fn test_empty_collections() {
		let _lock = TEST_MUTEX.lock().unwrap();

		// Test with empty collections
		let monitors = HashMap::new();
		let networks = HashMap::new();
		let triggers = HashMap::new();

		// Reset metrics to non-zero values
		MONITORS_TOTAL.set(10.0);
		MONITORS_ACTIVE.set(5.0);
		TRIGGERS_TOTAL.set(3.0);
		CONTRACTS_MONITORED.set(7.0);
		NETWORKS_MONITORED.set(2.0);
		NETWORK_MONITORS.reset();

		// Set a value for a network that doesn't exist
		NETWORK_MONITORS.with_label_values(&["test"]).set(3.0);

		// Update metrics
		update_monitoring_metrics(&monitors, &triggers, &networks);

		// Verify all metrics are reset to zero
		assert_eq!(MONITORS_TOTAL.get(), 0.0);
		assert_eq!(MONITORS_ACTIVE.get(), 0.0);
		assert_eq!(TRIGGERS_TOTAL.get(), 0.0);
		assert_eq!(CONTRACTS_MONITORED.get(), 0.0);
		assert_eq!(NETWORKS_MONITORED.get(), 0.0);

		// The test network should have been reset
		let test_network = NETWORK_MONITORS
			.get_metric_with_label_values(&["test"])
			.unwrap();
		assert_eq!(test_network.get(), 0.0);
	}

	#[test]
	fn test_rpc_metrics_helper_functions() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Test record_rpc_request
		record_rpc_request("ethereum", "eth_getBlockByNumber");
		record_rpc_request("ethereum", "eth_getBlockByNumber");
		record_rpc_request("polygon", "eth_call");

		let eth_requests = RPC_REQUESTS_TOTAL
			.get_metric_with_label_values(&["ethereum", "eth_getBlockByNumber"])
			.unwrap();
		assert_eq!(eth_requests.get(), 2.0);

		let polygon_requests = RPC_REQUESTS_TOTAL
			.get_metric_with_label_values(&["polygon", "eth_call"])
			.unwrap();
		assert_eq!(polygon_requests.get(), 1.0);

		// Test record_rpc_error
		record_rpc_error("ethereum", "429", "http");
		record_rpc_error("ethereum", "500", "http");
		record_rpc_error("ethereum", "0", "network");

		let rate_limit_errors = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["ethereum", "429", "http"])
			.unwrap();
		assert_eq!(rate_limit_errors.get(), 1.0);

		let server_errors = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["ethereum", "500", "http"])
			.unwrap();
		assert_eq!(server_errors.get(), 1.0);

		let network_errors = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["ethereum", "0", "network"])
			.unwrap();
		assert_eq!(network_errors.get(), 1.0);

		// Test observe_rpc_duration
		observe_rpc_duration("ethereum", 0.5);
		observe_rpc_duration("ethereum", 1.5);

		let duration_histogram = RPC_REQUEST_DURATION_SECONDS
			.get_metric_with_label_values(&["ethereum"])
			.unwrap();
		assert_eq!(duration_histogram.get_sample_count(), 2);

		// Test record_endpoint_rotation
		record_endpoint_rotation("ethereum", "rate_limit");
		record_endpoint_rotation("ethereum", "network_error");
		record_endpoint_rotation("polygon", "rate_limit");

		let eth_rate_limit_rotations = RPC_ENDPOINT_ROTATIONS_TOTAL
			.get_metric_with_label_values(&["ethereum", "rate_limit"])
			.unwrap();
		assert_eq!(eth_rate_limit_rotations.get(), 1.0);

		let eth_network_rotations = RPC_ENDPOINT_ROTATIONS_TOTAL
			.get_metric_with_label_values(&["ethereum", "network_error"])
			.unwrap();
		assert_eq!(eth_network_rotations.get(), 1.0);

		// Test record_rate_limit
		record_rate_limit("ethereum", "https://rpc1.example.com");
		record_rate_limit("ethereum", "https://rpc1.example.com");
		record_rate_limit("ethereum", "https://rpc2.example.com");

		let rpc1_rate_limits = RPC_RATE_LIMITS_TOTAL
			.get_metric_with_label_values(&["ethereum", "https://rpc1.example.com"])
			.unwrap();
		assert_eq!(rpc1_rate_limits.get(), 2.0);

		let rpc2_rate_limits = RPC_RATE_LIMITS_TOTAL
			.get_metric_with_label_values(&["ethereum", "https://rpc2.example.com"])
			.unwrap();
		assert_eq!(rpc2_rate_limits.get(), 1.0);
	}

	#[test]
	fn test_init_rpc_metrics_for_network() {
		let _lock = TEST_MUTEX.lock().unwrap();
		reset_all_metrics();

		// Initialize metrics for a new network
		init_rpc_metrics_for_network("arbitrum");

		// Verify error counters are initialized with 0
		let http_429 = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["arbitrum", "429", "http"])
			.unwrap();
		assert_eq!(http_429.get(), 0.0);

		let http_500 = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["arbitrum", "500", "http"])
			.unwrap();
		assert_eq!(http_500.get(), 0.0);

		let network_error = RPC_REQUEST_ERRORS_TOTAL
			.get_metric_with_label_values(&["arbitrum", "0", "network"])
			.unwrap();
		assert_eq!(network_error.get(), 0.0);

		// Verify rotation counters are initialized with 0
		let rate_limit_rotation = RPC_ENDPOINT_ROTATIONS_TOTAL
			.get_metric_with_label_values(&["arbitrum", "rate_limit"])
			.unwrap();
		assert_eq!(rate_limit_rotation.get(), 0.0);

		let network_rotation = RPC_ENDPOINT_ROTATIONS_TOTAL
			.get_metric_with_label_values(&["arbitrum", "network_error"])
			.unwrap();
		assert_eq!(network_rotation.get(), 0.0);

		// Verify metrics appear in gathered output
		let metrics = gather_metrics().expect("failed to gather metrics");
		let output = String::from_utf8(metrics).expect("metrics output is not valid UTF-8");

		assert!(output.contains("rpc_request_errors_total"));
		assert!(output.contains("arbitrum"));
		assert!(output.contains("rpc_endpoint_rotations_total"));
	}
}
