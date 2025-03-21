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
	models::Network,
	repositories::{MonitorRepository, NetworkRepository, TriggerRepository},
	services::{
		blockchain::{ClientPool, ClientPoolTrait},
		blockwatcher::{BlockTracker, BlockTrackerTrait, BlockWatcherService, FileBlockStorage},
	},
	utils::{logging::setup_logging, metrics::server::create_metrics_server},
};

use clap::{Arg, Command};
use dotenvy::dotenv;
use models::BlockChainType;
use services::trigger::TriggerExecutionServiceTrait;
use std::env::{set_var, var};
use std::sync::Arc;
use tokio::sync::watch;
use tokio_cron_scheduler::JobScheduler;
use tracing::{error, info};

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
	>(None, None, None)?;

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
