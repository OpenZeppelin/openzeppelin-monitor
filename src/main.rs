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
	repositories::{MonitorRepository, NetworkRepository, TriggerRepository},
	services::{
		blockchain::{EvmClient, StellarClient},
		blockwatcher::{BlockTracker, BlockTrackerTrait, BlockWatcherService, FileBlockStorage},
	},
};

use dotenvy::dotenv;
use log::{error, info};
use std::sync::Arc;
use tokio::sync::watch;

/// Main entry point for the blockchain monitoring service.
///
/// # Errors
/// Returns an error if service initialization fails or if there's an error during shutdown.
#[tokio::main]
async fn main() -> Result<()> {
	// Load environment variables from .env file
	dotenv().ok();
	env_logger::init();

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
		info!("No networks with active monitors found. Exiting...");
		return Ok(());
	}

	let (shutdown_tx, _) = watch::channel(false);

	let block_handler = create_block_handler(shutdown_tx.clone(), filter_service, active_monitors);
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
				let client = EvmClient::new(&network).await?;
				let _ = block_watcher.start_network_watcher(&network, client).await;
			}

			BlockChainType::Stellar => {
				let client = StellarClient::new(&network).await?;
				let _ = block_watcher.start_network_watcher(&network, client).await;
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
