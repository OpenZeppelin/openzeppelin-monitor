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
		create_block_handler, create_trigger_handler, has_active_monitors, initialize_services,
		Result,
	},
	models::{BlockChainType, Network},
	repositories::{
		MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
	},
	services::{
		blockchain::{ClientPool, ClientPoolTrait},
		blockwatcher::{BlockTracker, BlockTrackerTrait, BlockWatcherService, FileBlockStorage},
		filter::FilterService,
		trigger::TriggerExecutionServiceTrait,
	},
	utils::{
		constants::DOCUMENTATION_URL, logging::setup_logging,
		metrics::server::create_metrics_server, monitor::execution::execute_monitor,
		monitor::MonitorExecutionError,
	},
};

use clap::{Arg, Command};
use dotenvy::dotenv;
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

/// Tests the execution of a blockchain monitor configuration file.
///
/// This function loads and executes a monitor configuration from the specified path,
/// allowing for optional network and block number specifications. It's primarily used
/// for testing and debugging monitor configurations before deploying them.
///
/// # Arguments
/// * `path` - Path to the monitor configuration file
/// * `network_slug` - Optional network identifier to run the monitor against
/// * `block_number` - Optional specific block number to test the monitor against
/// * `monitor_service` - Service handling monitor operations
/// * `network_service` - Service handling network operations
/// * `filter_service` - Service handling filter operations
/// * `raw_output` - Whether to print the raw output of the monitor execution
///
/// # Returns
/// * `Result<()>` - Ok(()) if execution succeeds, or an error if execution fails
///
/// # Errors
/// * Returns an error if network slug is missing when block number is specified
/// * Returns an error if monitor execution fails for any reason (invalid path, network issues, etc.)
#[instrument(skip_all)]
async fn test_monitor_execution(
	path: String,
	network_slug: Option<String>,
	block_number: Option<u64>,
	monitor_service: Arc<Mutex<MonitorServiceType>>,
	network_service: Arc<Mutex<NetworkService<NetworkRepository>>>,
	filter_service: Arc<FilterService>,
	raw_output: bool,
) -> Result<()> {
	// Validate inputs first
	if block_number.is_some() && network_slug.is_none() {
		return Err(Box::new(MonitorExecutionError::execution_error(
			"Network name is required when executing a monitor for a specific block",
			None,
			None,
		)));
	}

	tracing::info!(
		message = "Starting monitor execution",
		path,
		network = network_slug,
		block = block_number,
	);

	let client_pool = ClientPool::new();
	let result = execute_monitor(
		&path,
		network_slug.as_ref(),
		block_number.as_ref(),
		monitor_service.clone(),
		network_service.clone(),
		filter_service.clone(),
		client_pool,
	)
	.await;

	match result {
		Ok(matches) => {
			tracing::info!("Monitor execution completed successfully");

			if matches.is_empty() {
				tracing::info!("No matches found");
				return Ok(());
			}

			tracing::info!("=========== Execution Results ===========");

			if raw_output {
				tracing::info!(matches = %matches, "Raw execution results");
			} else {
				// Parse and extract relevant information
				match serde_json::from_str::<serde_json::Value>(&matches) {
					Ok(json) => {
						if let Some(matches_array) = json.as_array() {
							tracing::info!(total = matches_array.len(), "Found matches");

							for (idx, match_result) in matches_array.iter().enumerate() {
								tracing::info!("Match #{}", idx + 1);
								tracing::info!("-------------");

								// Handle any network type (EVM, Stellar, etc.)
								for (network_type, details) in
									match_result.as_object().unwrap_or(&serde_json::Map::new())
								{
									// Get monitor name
									if let Some(monitor) = details.get("monitor") {
										if let Some(name) =
											monitor.get("name").and_then(|n| n.as_str())
										{
											tracing::info!("Monitor: {}", name);
										}
									}

									tracing::info!(
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
													tracing::info!("Block: {}", block_num);
												}

												// Get transaction hash
												if let Some(hash) = receipt
													.get("transactionHash")
													.and_then(|h| h.as_str())
												{
													tracing::info!("Transaction: {}", hash);
												}
											}
										}
										"Stellar" => {
											// Get block number from ledger
											if let Some(ledger) = details.get("ledger") {
												if let Some(sequence) =
													ledger.get("sequence").and_then(|s| s.as_u64())
												{
													tracing::info!("Ledger: {}", sequence);
												}
											}

											// Get transaction hash
											if let Some(transaction) = details.get("transaction") {
												if let Some(hash) = transaction
													.get("txHash")
													.and_then(|h| h.as_str())
												{
													tracing::info!("Transaction: {}", hash);
												}
											}
										}
										_ => {}
									}

									// Get matched conditions (common across networks)
									if let Some(matched_on) = details.get("matched_on") {
										tracing::info!("Matched Conditions:");

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
													tracing::info!("  - Event: {}", condition);
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
													tracing::info!("  - Function: {}", condition);
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
													tracing::info!(
														"  - Transaction Status: {}",
														status
													);
												}
											}
										}
									}
								}
								tracing::info!("-------------\n");
							}
						}
					}
					Err(e) => {
						tracing::warn!(
							error = %e,
							"Failed to parse JSON output, falling back to raw output"
						);
						tracing::info!(matches = %matches, "Raw execution results");
					}
				}
			}

			tracing::info!("=========================================");
			Ok(())
		}
		Err(e) => {
			// Convert to domain-specific error with proper context
			Err(MonitorExecutionError::execution_error(
				"Monitor execution failed",
				Some(e.into()),
				Some(std::collections::HashMap::from([
					("path".to_string(), path),
					("network".to_string(), network_slug.unwrap_or_default()),
					(
						"block".to_string(),
						block_number.map(|b| b.to_string()).unwrap_or_default(),
					),
				])),
			)
			.into())
		}
	}
}

/// Main entry point for the blockchain monitoring service.
///
/// # Errors
/// Returns an error if service initialization fails or if there's an error during shutdown.
#[tokio::main]
async fn main() -> Result<()> {
	// Initialize command-line interface
	let matches = Command::new("openzeppelin-monitor")
		.version(env!("CARGO_PKG_VERSION"))
		.about(
			"A blockchain monitoring service that watches for specific on-chain activities and \
			 triggers notifications based on configurable conditions.",
		)
		.arg(
			Arg::new("log-file")
				.long("log-file")
				.help("Write logs to file instead of stdout")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("log-level")
				.long("log-level")
				.help("Set log level (trace, debug, info, warn, error)")
				.value_name("LEVEL"),
		)
		.arg(
			Arg::new("log-path")
				.long("log-path")
				.help("Path to store log files (default: logs/)")
				.value_name("PATH"),
		)
		.arg(
			Arg::new("log-max-size")
				.long("log-max-size")
				.help("Maximum log file size in bytes before rolling (default: 1GB)")
				.value_name("BYTES"),
		)
		.arg(
			Arg::new("metrics-address")
				.long("metrics-address")
				.help("Address to start the metrics server on (default: 127.0.0.1:8081)")
				.value_name("HOST:PORT"),
		)
		.arg(
			Arg::new("metrics")
				.long("metrics")
				.help("Enable metrics server")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("monitorPath")
				.long("monitorPath")
				.help("Path to the monitor to execute")
				.value_name("MONITOR_PATH"),
		)
		.arg(
			Arg::new("network")
				.long("network")
				.help("Network to execute the monitor for")
				.value_name("NETWORK_SLUG"),
		)
		.arg(
			Arg::new("block")
				.long("block")
				.help("Block number to execute the monitor for")
				.value_name("BLOCK_NUMBER"),
		)
		.get_matches();

	// Load environment variables from .env file
	dotenv().ok();

	// Only apply CLI options if the corresponding environment variables are NOT already set
	if matches.get_flag("log-file") && var("LOG_MODE").is_err() {
		set_var("LOG_MODE", "file");
	}

	if let Some(level) = matches.get_one::<String>("log-level") {
		if var("LOG_LEVEL").is_err() {
			set_var("LOG_LEVEL", level);
		}
	}

	if let Some(path) = matches.get_one::<String>("log-path") {
		if var("LOG_DATA_DIR").is_err() {
			set_var("LOG_DATA_DIR", path);
		}
	}

	if let Some(max_size) = matches.get_one::<String>("log-max-size") {
		if var("LOG_MAX_SIZE").is_err() {
			set_var("LOG_MAX_SIZE", max_size);
		}
	}

	// Setup logging to stdout
	setup_logging().unwrap_or_else(|e| {
		error!("Failed to setup logging: {}", e);
	});

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
	.map_err(|e| anyhow::anyhow!("Failed to initialize services: {}. Please refer to the documentation quickstart ({}) on how to configure the service.", e, DOCUMENTATION_URL))?;

	// Read CLI arguments to determine if we should test monitor execution
	let monitor_path = matches
		.get_one::<String>("monitorPath")
		.map(|s| s.to_string());
	let network_slug = matches.get_one::<String>("network").map(|s| s.to_string());
	let block_number = matches
		.get_one::<String>("block")
		.map(|s| {
			s.parse::<u64>().map_err(|e| {
				error!("Failed to parse block number: {}", e);
				e
			})
		})
		.transpose()?;

	let should_test_monitor_execution = monitor_path.is_some();
	// If monitor path is provided, test monitor execution else start the service
	if should_test_monitor_execution {
		let monitor_path = monitor_path.ok_or(anyhow::anyhow!(
			"monitor_path must be defined when testing monitor execution"
		))?;
		return test_monitor_execution(
			monitor_path,
			network_slug,
			block_number,
			monitor_service,
			network_service,
			filter_service,
			false,
		)
		.await;
	}

	// Check if metrics should be enabled from either CLI flag or env var
	let metrics_enabled =
		matches.get_flag("metrics") || var("METRICS_ENABLED").map(|v| v == "true").unwrap_or(false);

	// Extract metrics address as a String to avoid borrowing issues
	let metrics_address = if var("IN_DOCKER").unwrap_or_default() == "true" {
		// For Docker, use METRICS_PORT env var if available
		var("METRICS_PORT")
			.map(|port| format!("0.0.0.0:{}", port))
			.unwrap_or_else(|_| "0.0.0.0:8081".to_string())
	} else {
		// For CLI, use the command line arg or default
		matches
			.get_one::<String>("metrics-address")
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

	let (shutdown_tx, _) = watch::channel(false);
	// Pre-load all trigger scripts into memory at startup to reduce file I/O operations.
	// This prevents repeated file descriptor usage during script execution and improves performance
	// by keeping scripts readily available in memory.
	let active_monitors_trigger_scripts = trigger_execution_service
		.load_scripts(&active_monitors)
		.await?;
	let client_pool = Arc::new(ClientPool::new());
	let block_handler = create_block_handler(
		shutdown_tx.clone(),
		filter_service,
		active_monitors,
		client_pool.clone(),
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

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_monitor_execution_without_network_slug_with_block_number() {
		// Initialize services
		let (filter_service, _, _, _, monitor_service, network_service, _) = initialize_services::<
			MonitorRepository<NetworkRepository, TriggerRepository>,
			NetworkRepository,
			TriggerRepository,
		>(None, None, None)
		.unwrap();

		let path = "test_monitor.json".to_string();
		let block_number = Some(12345);

		// Execute test
		let result = test_monitor_execution(
			path,
			None,
			block_number,
			monitor_service,
			network_service,
			filter_service,
			false,
		)
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
		let (filter_service, _, _, _, monitor_service, network_service, _) = initialize_services::<
			MonitorRepository<NetworkRepository, TriggerRepository>,
			NetworkRepository,
			TriggerRepository,
		>(None, None, None)
		.unwrap();

		// Test parameters
		let path = "nonexistent_monitor.json".to_string();
		let network_slug = Some("test_network".to_string());
		let block_number = Some(12345);

		// Execute test
		let result = test_monitor_execution(
			path,
			network_slug,
			block_number,
			monitor_service,
			network_service,
			filter_service,
			false,
		)
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
