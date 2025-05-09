//! Blockchain monitoring service entry point.
//!
//! This binary provides the main entry point for the blockchain monitoring service.
//! It initializes all required services, sets up blockchain watchers for configured
//! networks, and handles graceful shutdown on interrupt signals.
//!
//! # Architecture
//! The service is built around several key components:
//! - Monitors: Define what to watch for in the blockchain
//! - Networks: Supported blockchain networks
//! - Triggers: Actions to take when monitored conditions are met
//! - Services: Core functionality including block watching, filtering, and notifications
//!
//! # Flow
//! 1. Loads configurations from the default directory
//! 2. Initializes core services (monitoring, filtering, notifications)
//! 3. Sets up blockchain watchers for networks with active monitors
//! 4. Processes blocks and triggers notifications based on configured conditions
//! 5. Handles graceful shutdown on Ctrl+C

pub mod bootstrap;
pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

use crate::{
	bootstrap::{
		create_block_handler, create_trigger_handler, get_contract_specs, has_active_monitors,
		initialize_services, Result,
	},
	models::{BlockChainType, Network, ScriptLanguage},
	repositories::{
		MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
	},
	services::{
		blockchain::{ClientPool, ClientPoolTrait},
		blockwatcher::{BlockTracker, BlockTrackerTrait, BlockWatcherService, FileBlockStorage},
		filter::FilterService,
		trigger::{TriggerExecutionService, TriggerExecutionServiceTrait},
	},
	utils::{
		constants::DOCUMENTATION_URL,
		logging::setup_logging,
		metrics::server::create_metrics_server,
		monitor::{
			execution::{execute_monitor, MonitorExecutionConfig},
			MonitorExecutionError,
		},
		parse_string_to_bytes_size,
	},
};

use clap::Parser;
use dotenvy::dotenv_override;
use std::collections::HashMap;
use std::env::{set_var, var};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};
use tokio_cron_scheduler::JobScheduler;
use tracing::{error, info, instrument};

type MonitorServiceType = MonitorService<
	MonitorRepository<NetworkRepository, TriggerRepository>,
	NetworkRepository,
	TriggerRepository,
>;
/// Configuration for testing monitor execution
/// Fields:
/// * `path` - Path to the monitor configuration file
/// * `network_slug` - Optional network identifier to run the monitor against
/// * `block_number` - Optional specific block number to test the monitor against
/// * `monitor_service` - Service handling monitor operations
/// * `network_service` - Service handling network operations
/// * `filter_service` - Service handling filter operations
/// * `trigger_execution_service` - Service handling trigger execution
/// * `active_monitors_trigger_scripts` - Map of active monitors and their trigger scripts
/// * `raw_output` - Whether to print the raw output of the monitor execution
/// * `client_pool` - Client pool of blockchain clients
struct MonitorExecutionTestConfig {
	pub path: String,
	pub network_slug: Option<String>,
	pub block_number: Option<u64>,
	pub monitor_service: Arc<Mutex<MonitorServiceType>>,
	pub network_service: Arc<Mutex<NetworkService<NetworkRepository>>>,
	pub filter_service: Arc<FilterService>,
	pub trigger_execution_service: Arc<TriggerExecutionService<TriggerRepository>>,
	pub active_monitors_trigger_scripts: HashMap<String, (ScriptLanguage, String)>,
	pub raw_output: bool,
	pub client_pool: Arc<ClientPool>,
}

#[derive(Parser)]
#[command(
	name = "openzeppelin-monitor",
	about = "A blockchain monitoring service that watches for specific on-chain activities and triggers notifications based on configurable conditions.",
	version
)]
struct Cli {
	/// Write logs to file instead of stdout
	#[arg(long)]
	log_file: bool,

	/// Set log level (trace, debug, info, warn, error)
	#[arg(long, value_name = "LEVEL")]
	log_level: Option<String>,

	/// Path to store log files (default: logs/)
	#[arg(long, value_name = "PATH")]
	log_path: Option<String>,

	/// Maximum log file size before rolling (e.g., "1GB", "500MB", "1024KB")
	#[arg(long, value_name = "SIZE", value_parser = parse_string_to_bytes_size)]
	log_max_size: Option<u64>,

	/// Address to start the metrics server on (default: 127.0.0.1:8081)
	#[arg(long, value_name = "HOST:PORT")]
	metrics_address: Option<String>,

	/// Enable metrics server
	#[arg(long)]
	metrics: bool,

	/// Path to the monitor to execute
	#[arg(long, value_name = "MONITOR_PATH")]
	monitor_path: Option<String>,

	/// Network to execute the monitor for
	#[arg(long, value_name = "NETWORK_SLUG")]
	network: Option<String>,

	/// Block number to execute the monitor for
	#[arg(long, value_name = "BLOCK_NUMBER")]
	block: Option<u64>,

	/// Validate configuration files without starting the service
	#[arg(long)]
	check: bool,
}

impl Cli {
	/// Apply CLI options to environment variables, overriding any existing values
	fn apply_to_env(&self) {
		// Reload environment variables from .env file
		// Override any existing environment variables
		dotenv_override().ok();

		// Log file mode - override if CLI flag is set
		if self.log_file {
			set_var("LOG_MODE", "file");
		}

		// Set log level from RUST_LOG if it exists
		if let Ok(level) = var("RUST_LOG") {
			set_var("LOG_LEVEL", level);
		}

		// Log level - override if CLI flag is set
		if let Some(level) = &self.log_level {
			set_var("LOG_LEVEL", level);
			set_var("RUST_LOG", level);
		}

		// Log path - override if CLI flag is set
		if let Some(path) = &self.log_path {
			set_var("LOG_DATA_DIR", path);
		}

		// Log max size - override if CLI flag is set
		if let Some(max_size) = &self.log_max_size {
			set_var("LOG_MAX_SIZE", max_size.to_string());
		}

		// Metrics server - override if CLI flag is set
		if self.metrics {
			set_var("METRICS_ENABLED", "true");
		}

		// Metrics address - override if CLI flag is set
		if let Some(address) = &self.metrics_address {
			// Extract port from address if it's in HOST:PORT format
			if let Some(port) = address.split(':').nth(1) {
				set_var("METRICS_PORT", port);
			}
		}
	}
}

/// Main entry point for the blockchain monitoring service.
///
/// # Errors
/// Returns an error if service initialization fails or if there's an error during shutdown.
#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();

	// Apply CLI options to environment
	cli.apply_to_env();

	// Setup logging to stdout
	setup_logging().unwrap_or_else(|e| {
		error!("Failed to setup logging: {}", e);
	});

	// If --check flag is provided, only validate configuration and exit
	if cli.check {
		validate_configuration().await;
		return Ok(());
	}

	let (
		filter_service,
		trigger_execution_service,
		active_monitors,
		networks,
		monitor_service,
		network_service,
		trigger_service,
	) = initialize_services::<
		MonitorRepository<NetworkRepository, TriggerRepository>,
		NetworkRepository,
		TriggerRepository,
	>(None, None, None)
	.await
	.map_err(|e| anyhow::anyhow!("Failed to initialize services: {}. Please refer to the documentation quickstart ({}) on how to configure the service.", e, DOCUMENTATION_URL))?;

	// Pre-load all trigger scripts into memory at startup to reduce file I/O operations.
	// This prevents repeated file descriptor usage during script execution and improves performance
	// by keeping scripts readily available in memory.
	let active_monitors_trigger_scripts = trigger_execution_service
		.load_scripts(&active_monitors)
		.await?;
	// Read CLI arguments to determine if we should test monitor execution
	let monitor_path = cli.monitor_path.clone();
	let network_slug = cli.network.clone();
	let block_number = cli.block;

	let client_pool = Arc::new(ClientPool::new());

	let should_test_monitor_execution = monitor_path.is_some();
	// If monitor path is provided, test monitor execution else start the service
	if should_test_monitor_execution {
		let monitor_path = monitor_path.ok_or(anyhow::anyhow!(
			"monitor_path must be defined when testing monitor execution"
		))?;
		return test_monitor_execution(MonitorExecutionTestConfig {
			path: monitor_path,
			network_slug,
			block_number,
			monitor_service: monitor_service.clone(),
			network_service: network_service.clone(),
			filter_service: filter_service.clone(),
			trigger_execution_service: trigger_execution_service.clone(),
			active_monitors_trigger_scripts,
			raw_output: false,
			client_pool,
		})
		.await;
	}

	// Check if metrics should be enabled from either CLI flag or env var
	let metrics_enabled =
		cli.metrics || var("METRICS_ENABLED").map(|v| v == "true").unwrap_or(false);

	// Extract metrics address as a String to avoid borrowing issues
	let metrics_address = if var("IN_DOCKER").unwrap_or_default() == "true" {
		// For Docker, use METRICS_PORT env var if available
		var("METRICS_PORT")
			.map(|port| format!("0.0.0.0:{}", port))
			.unwrap_or_else(|_| "0.0.0.0:8081".to_string())
	} else {
		// For CLI, use the command line arg or default
		cli.metrics_address
			.map(|s| s.to_string())
			.unwrap_or_else(|| "127.0.0.1:8081".to_string())
	};

	// Start the metrics server if successful
	let metrics_server = if metrics_enabled {
		info!("Metrics server enabled, starting on {}", metrics_address);

		// Create the metrics server future
		match create_metrics_server(
			metrics_address,
			monitor_service.clone(),
			network_service.clone(),
			trigger_service.clone(),
		) {
			Ok(server) => Some(server),
			Err(e) => {
				error!("Failed to create metrics server: {}", e);
				None
			}
		}
	} else {
		info!("Metrics server disabled. Use --metrics flag or METRICS_ENABLED=true to enable");
		None
	};

	let networks_with_monitors: Vec<Network> = networks
		.values()
		.filter(|network| has_active_monitors(&active_monitors.clone(), &network.slug))
		.cloned()
		.collect();

	if networks_with_monitors.is_empty() {
		info!("No networks with active monitors found. Exiting...");
		return Ok(());
	}

	// Create a vector of networks with their associated monitors
	let network_monitors = networks_with_monitors
		.iter()
		.map(|network| {
			(
				network.clone(),
				active_monitors
					.iter()
					.filter(|m| m.networks.contains(&network.slug))
					.cloned()
					.collect::<Vec<_>>(),
			)
		})
		.collect::<Vec<_>>();

	// Fetch all contract specs for all active monitors
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	let (shutdown_tx, _) = watch::channel(false);
	let block_handler = create_block_handler(
		shutdown_tx.clone(),
		filter_service,
		active_monitors,
		client_pool.clone(),
		contract_specs,
	);
	let trigger_handler = create_trigger_handler(
		shutdown_tx.clone(),
		trigger_execution_service,
		active_monitors_trigger_scripts,
	);

	let file_block_storage = Arc::new(FileBlockStorage::default());
	let block_watcher = BlockWatcherService::<FileBlockStorage, _, _, JobScheduler>::new(
		file_block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(BlockTracker::new(1000, Some(file_block_storage.clone()))),
	)
	.await?;

	for network in networks_with_monitors {
		match network.network_type {
			BlockChainType::EVM => {
				if let Ok(client) = client_pool.get_evm_client(&network).await {
					let _ = block_watcher
						.start_network_watcher(&network, (*client).clone())
						.await
						.inspect_err(|e| {
							error!("Failed to start EVM network watcher: {}", e);
						});
				} else {
					error!("Failed to get EVM client for network: {}", network.slug);
				}
			}
			BlockChainType::Stellar => {
				if let Ok(client) = client_pool.get_stellar_client(&network).await {
					let _ = block_watcher
						.start_network_watcher(&network, (*client).clone())
						.await
						.inspect_err(|e| {
							error!("Failed to start Stellar network watcher: {}", e);
						});
				} else {
					error!("Failed to get Stellar client for network: {}", network.slug);
				}
			}
			BlockChainType::Midnight => unimplemented!("Midnight not implemented"),
			BlockChainType::Solana => unimplemented!("Solana not implemented"),
		}
	}

	info!("Service started. Press Ctrl+C to shutdown");

	let ctrl_c = tokio::signal::ctrl_c();

	if let Some(metrics_future) = metrics_server {
		tokio::select! {
				result = ctrl_c => {
					if let Err(e) = result {
			  error!("Error waiting for Ctrl+C: {}", e);
			}
			info!("Shutdown signal received, stopping services...");
		  }
		  result = metrics_future => {
			if let Err(e) = result {
			  error!("Metrics server error: {}", e);
			}
			info!("Metrics server stopped, shutting down services...");
		  }
		}
	} else {
		let _ = ctrl_c.await;
		info!("Shutdown signal received, stopping services...");
	}

	// Common shutdown logic
	let _ = shutdown_tx.send(true);

	// Future for all network shutdown operations
	let shutdown_futures = networks
		.values()
		.map(|network| block_watcher.stop_network_watcher(&network.slug));

	for result in futures::future::join_all(shutdown_futures).await {
		if let Err(e) = result {
			error!("Error during shutdown: {}", e);
		}
	}

	tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

	info!("Shutdown complete");
	Ok(())
}

/// Tests the execution of a blockchain monitor configuration file.
///
/// This function loads and executes a monitor configuration from the specified path,
/// allowing for optional network and block number specifications. It's primarily used
/// for testing and debugging monitor configurations before deploying them.
///
/// # Arguments
/// * `config` - Configuration for monitor execution
///
/// # Returns
/// * `Result<()>` - Ok(()) if execution succeeds, or an error if execution fails
///
/// # Errors
/// * Returns an error if network slug is missing when block number is specified
/// * Returns an error if monitor execution fails for any reason (invalid path, network issues, etc.)
#[instrument(skip_all)]
async fn test_monitor_execution(config: MonitorExecutionTestConfig) -> Result<()> {
	// Validate inputs first
	if config.block_number.is_some() && config.network_slug.is_none() {
		return Err(Box::new(MonitorExecutionError::execution_error(
			"Network name is required when executing a monitor for a specific block",
			None,
			None,
		)));
	}

	info!(
		message = "Starting monitor execution",
		path = config.path,
		network = config.network_slug,
		block = config.block_number,
	);

	let result = execute_monitor(MonitorExecutionConfig {
		path: config.path.clone(),
		network_slug: config.network_slug.clone(),
		block_number: config.block_number,
		monitor_service: config.monitor_service.clone(),
		network_service: config.network_service.clone(),
		filter_service: config.filter_service.clone(),
		trigger_execution_service: config.trigger_execution_service.clone(),
		active_monitors_trigger_scripts: config.active_monitors_trigger_scripts.clone(),
		client_pool: config.client_pool.clone(),
	})
	.await;

	match result {
		Ok(matches) => {
			info!("Monitor execution completed successfully");

			if matches.is_empty() {
				info!("No matches found");
				return Ok(());
			}

			info!("=========== Execution Results ===========");

			if config.raw_output {
				info!(matches = %matches, "Raw execution results");
			} else {
				// Parse and extract relevant information
				match serde_json::from_str::<serde_json::Value>(&matches) {
					Ok(json) => {
						if let Some(matches_array) = json.as_array() {
							info!(total = matches_array.len(), "Found matches");

							for (idx, match_result) in matches_array.iter().enumerate() {
								info!("Match #{}", idx + 1);
								info!("-------------");

								// Handle any network type (EVM, Stellar, etc.)
								for (network_type, details) in
									match_result.as_object().unwrap_or(&serde_json::Map::new())
								{
									// Get monitor name
									if let Some(monitor) = details.get("monitor") {
										if let Some(name) =
											monitor.get("name").and_then(|n| n.as_str())
										{
											info!("Monitor: {}", name);
										}
									}

									info!(
										"Network: {}",
										details
											.get("network_slug")
											.unwrap_or(&serde_json::Value::Null)
									);

									// Get transaction details based on network type
									match network_type.as_str() {
										"EVM" => {
											if let Some(receipt) = details.get("receipt") {
												// Get block number (handle hex format)
												if let Some(block) = receipt.get("blockNumber") {
													let block_num = match block.as_str() {
														Some(hex) if hex.starts_with("0x") => {
															u64::from_str_radix(
																hex.trim_start_matches("0x"),
																16,
															)
															.map(|n| n.to_string())
															.unwrap_or_else(|_| hex.to_string())
														}
														_ => block
															.as_str()
															.unwrap_or_default()
															.to_string(),
													};
													info!("Block: {}", block_num);
												}

												// Get transaction hash
												if let Some(hash) = receipt
													.get("transactionHash")
													.and_then(|h| h.as_str())
												{
													info!("Transaction: {}", hash);
												}
											}
										}
										"Stellar" => {
											// Get block number from ledger
											if let Some(ledger) = details.get("ledger") {
												if let Some(sequence) =
													ledger.get("sequence").and_then(|s| s.as_u64())
												{
													info!("Ledger: {}", sequence);
												}
											}

											// Get transaction hash
											if let Some(transaction) = details.get("transaction") {
												if let Some(hash) = transaction
													.get("txHash")
													.and_then(|h| h.as_str())
												{
													info!("Transaction: {}", hash);
												}
											}
										}
										_ => {}
									}

									// Get matched conditions (common across networks)
									if let Some(matched_on) = details.get("matched_on") {
										info!("Matched Conditions:");

										// Check events
										if let Some(events) =
											matched_on.get("events").and_then(|e| e.as_array())
										{
											for event in events {
												let mut condition = String::new();
												if let Some(sig) =
													event.get("signature").and_then(|s| s.as_str())
												{
													condition.push_str(sig);
												}
												if let Some(expr) =
													event.get("expression").and_then(|e| e.as_str())
												{
													if !expr.is_empty() {
														condition
															.push_str(&format!(" where {}", expr));
													}
												}
												if !condition.is_empty() {
													info!("  - Event: {}", condition);
												}
											}
										}

										// Check functions
										if let Some(functions) =
											matched_on.get("functions").and_then(|f| f.as_array())
										{
											for function in functions {
												let mut condition = String::new();
												if let Some(sig) = function
													.get("signature")
													.and_then(|s| s.as_str())
												{
													condition.push_str(sig);
												}
												if let Some(expr) = function
													.get("expression")
													.and_then(|e| e.as_str())
												{
													if !expr.is_empty() {
														condition
															.push_str(&format!(" where {}", expr));
													}
												}
												if !condition.is_empty() {
													info!("  - Function: {}", condition);
												}
											}
										}

										// Check transaction conditions
										if let Some(txs) = matched_on
											.get("transactions")
											.and_then(|t| t.as_array())
										{
											for tx in txs {
												if let Some(status) =
													tx.get("status").and_then(|s| s.as_str())
												{
													info!("  - Transaction Status: {}", status);
												}
											}
										}
									}
								}
								info!("-------------\n");
							}
						}
					}
					Err(e) => {
						tracing::warn!(
							error = %e,
							"Failed to parse JSON output, falling back to raw output"
						);
						info!(matches = %matches, "Raw execution results");
					}
				}
			}

			info!("=========================================");
			Ok(())
		}
		Err(e) => {
			// Convert to domain-specific error with proper context
			Err(MonitorExecutionError::execution_error(
				"Monitor execution failed",
				Some(e.into()),
				Some(std::collections::HashMap::from([
					("path".to_string(), config.path),
					(
						"network".to_string(),
						config.network_slug.unwrap_or_default(),
					),
					(
						"block".to_string(),
						config
							.block_number
							.map(|b| b.to_string())
							.unwrap_or_default(),
					),
				])),
			)
			.into())
		}
	}
}

/// Validates configuration files and their structure
async fn validate_configuration() {
	info!("Validating configuration files...");

	// Initialize services in validation mode to check configurations
	match initialize_services::<
		MonitorRepository<NetworkRepository, TriggerRepository>,
		NetworkRepository,
		TriggerRepository,
	>(None, None, None)
	.await
	{
		Ok((_, _, active_monitors, networks, _, _, _)) => {
			info!("✓ Core services initialized successfully");

			// Check if we have any monitors configured
			if active_monitors.is_empty() {
				error!("No active monitors found. Please refer to the documentation quickstart ({}) for configuration setup.", DOCUMENTATION_URL);
				return;
			}
			info!("✓ Found {} active monitor(s)", active_monitors.len());

			// Check if we have any networks with active monitors
			let networks_with_monitors: Vec<&Network> = networks
				.values()
				.filter(|network| has_active_monitors(&active_monitors, &network.slug))
				.collect();

			if networks_with_monitors.is_empty() {
				error!("No networks with active monitors found. Please refer to the documentation quickstart ({}) for network configuration.", DOCUMENTATION_URL);
				return;
			}
			info!(
				"✓ Found {} network(s) with active monitors",
				networks_with_monitors.len()
			);

			info!("Configuration validation completed successfully!");
		}
		Err(e) => {
			error!("{}.\nPlease refer to the documentation quickstart ({}) for proper configuration setup.", e, DOCUMENTATION_URL);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_monitor_execution_without_network_slug_with_block_number() {
		// Initialize services
		let (filter_service, trigger_execution_service, _, _, monitor_service, network_service, _) =
			initialize_services::<
				MonitorRepository<NetworkRepository, TriggerRepository>,
				NetworkRepository,
				TriggerRepository,
			>(None, None, None)
			.await
			.unwrap();

		let path = "test_monitor.json".to_string();
		let block_number = Some(12345);
		let client_pool = Arc::new(ClientPool::new());
		// Execute test
		let result = test_monitor_execution(MonitorExecutionTestConfig {
			path,
			network_slug: None,
			block_number,
			monitor_service: monitor_service.clone(),
			network_service: network_service.clone(),
			filter_service: filter_service.clone(),
			trigger_execution_service: trigger_execution_service.clone(),
			active_monitors_trigger_scripts: HashMap::new(),
			raw_output: false,
			client_pool: client_pool.clone(),
		})
		.await;

		// Verify result and error logging
		assert!(result.is_err());
		assert!(result
			.err()
			.unwrap()
			.to_string()
			.contains("Network name is required when executing a monitor for a specific block"));
	}

	#[tokio::test]
	async fn test_monitor_execution_with_invalid_path() {
		// Initialize services
		let (filter_service, trigger_execution_service, _, _, monitor_service, network_service, _) =
			initialize_services::<
				MonitorRepository<NetworkRepository, TriggerRepository>,
				NetworkRepository,
				TriggerRepository,
			>(None, None, None)
			.await
			.unwrap();

		// Test parameters
		let path = "nonexistent_monitor.json".to_string();
		let network_slug = Some("test_network".to_string());
		let block_number = Some(12345);

		let client_pool = Arc::new(ClientPool::new());
		// Execute test
		let result = test_monitor_execution(MonitorExecutionTestConfig {
			path,
			network_slug,
			block_number,
			monitor_service: monitor_service.clone(),
			network_service: network_service.clone(),
			filter_service: filter_service.clone(),
			trigger_execution_service: trigger_execution_service.clone(),
			active_monitors_trigger_scripts: HashMap::new(),
			raw_output: false,
			client_pool: client_pool.clone(),
		})
		.await;

		// Verify result
		assert!(result.is_err());
		assert!(result
			.err()
			.unwrap()
			.to_string()
			.contains("Monitor execution failed"));
	}
}
