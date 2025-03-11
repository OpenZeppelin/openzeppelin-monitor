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
	utils::logging::setup_logging,
};

use clap::{Arg, Command};
use dotenvy::dotenv;
use log::{debug, error, info};
use models::BlockChainType;
use std::sync::Arc;
use tokio::sync::watch;

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
		.get_matches();

	// Load environment variables from .env file
	dotenv().ok();

	// Only apply CLI options if the corresponding environment variables are NOT already set
	if matches.get_flag("log-file") && std::env::var("LOG_MODE").is_err() {
		std::env::set_var("LOG_MODE", "file");
	}

	if let Some(level) = matches.get_one::<String>("log-level") {
		if std::env::var("LOG_LEVEL").is_err() {
			std::env::set_var("LOG_LEVEL", level);
		}
	}

	if let Some(path) = matches.get_one::<String>("log-path") {
		if std::env::var("LOG_FILE_PATH").is_err() {
			std::env::set_var("LOG_FILE_PATH", path);
		}
	}

	if let Some(max_size) = matches.get_one::<String>("log-max-size") {
		if std::env::var("LOG_MAX_SIZE").is_err() {
			std::env::set_var("LOG_MAX_SIZE", max_size);
		}
	}

	setup_logging();

	let (filter_service, trigger_execution_service, active_monitors, networks) =
		initialize_services::<
			MonitorRepository<NetworkRepository, TriggerRepository>,
			NetworkRepository,
			TriggerRepository,
		>(None, None, None)?;

	let networks_with_monitors: Vec<Network> = networks
		.values()
		.filter(|network| has_active_monitors(&active_monitors.clone(), &network.slug))
		.cloned()
		.collect();

	if networks_with_monitors.is_empty() {
		debug!("No networks with active monitors found. Exiting...");
		info!("No networks with active monitors found. Exiting...");
		return Ok(());
	}

	let (shutdown_tx, _) = watch::channel(false);

	let client_pool = Arc::new(ClientPool::new());
	let block_handler = create_block_handler(
		shutdown_tx.clone(),
		filter_service,
		active_monitors,
		client_pool.clone(),
	);
	let trigger_handler = create_trigger_handler(shutdown_tx.clone(), trigger_execution_service);

	let file_block_storage = Arc::new(FileBlockStorage::default());
	let block_watcher = BlockWatcherService::new(
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
						.await;
				}
			}
			BlockChainType::Stellar => {
				if let Ok(client) = client_pool.get_stellar_client(&network).await {
					let _ = block_watcher
						.start_network_watcher(&network, (*client).clone())
						.await;
				}
			}
			BlockChainType::Midnight => unimplemented!("Midnight not implemented"),
			BlockChainType::Solana => unimplemented!("Solana not implemented"),
		}
	}

	info!("Service started. Press Ctrl+C to shutdown");
	tokio::select! {
		_ = tokio::signal::ctrl_c() => {
			info!("Shutdown signal received, stopping services...");
			let _ = shutdown_tx.send(true);

			// Create a future for all network shutdown operations
			let shutdown_futures = networks.values().map(|network| {
				block_watcher.stop_network_watcher(&network.slug)
			});

			// Wait for all shutdown operations to complete
			for result in futures::future::join_all(shutdown_futures).await {
				if let Err(e) = result {
					error!("Error during shutdown: {}", e);
				}
			}

			// Give some time for in-flight tasks to complete
			tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
		}
	}

	info!("Shutdown complete");
	Ok(())
}
