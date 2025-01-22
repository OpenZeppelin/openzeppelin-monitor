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

pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

use crate::{
	models::{BlockChainType, BlockType, Monitor, Network},
	repositories::{
		MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
		TriggerService,
	},
	services::{
		blockchain::{BlockFilterFactory, EvmClient, StellarClient},
		blockwatcher::{BlockTracker, BlockWatcherService, FileBlockStorage},
		filter::{handle_match, FilterService},
		notification::NotificationService,
		trigger::TriggerExecutionService,
	},
};

use dotenvy::dotenv;
use log::{error, info};
use services::blockchain::BlockChainClient;
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::broadcast;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type ServiceResult = Result<(
	Arc<FilterService>,
	Arc<TriggerExecutionService<TriggerRepository>>,
	Vec<Monitor>,
	HashMap<String, Network>,
)>;

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
		initialize_services()?;

	let networks_with_monitors: Vec<Network> = networks
		.values()
		.filter(|network| has_active_monitors(&active_monitors.clone(), &network.slug))
		.cloned()
		.collect();

	if networks_with_monitors.is_empty() {
		info!("No networks with active monitors found. Exiting...");
		return Ok(());
	}

	let (shutdown_tx, _) = broadcast::channel(1);

	let block_handler = create_block_handler(
		shutdown_tx.clone(),
		trigger_execution_service,
		filter_service,
		active_monitors,
	);

	let file_block_storage = Arc::new(FileBlockStorage::default());
	let block_watcher = BlockWatcherService::new(
		file_block_storage.clone(),
		block_handler,
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
			let _ = shutdown_tx.send(());

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

/// Initializes all required services for the blockchain monitor.
///
/// # Returns
/// Returns a tuple containing:
/// - FilterService: Handles filtering of blockchain data
/// - TriggerExecutionService: Manages trigger execution
/// - Vec<Monitor>: List of active monitors
/// - HashMap<String, Network>: Available networks indexed by slug
///
/// # Errors
/// Returns an error if any service initialization fails
fn initialize_services() -> ServiceResult {
	let network_service = NetworkService::<NetworkRepository>::new(None)?;
	let trigger_service = TriggerService::<TriggerRepository>::new(None)?;
	let monitor_service = Arc::new(MonitorService::<MonitorRepository>::new(
		None,
		Some(&network_service),
		Some(&trigger_service),
	)?);
	let notification_service = NotificationService::new();

	let filter_service = Arc::new(FilterService::new());
	let trigger_execution_service = Arc::new(TriggerExecutionService::<TriggerRepository>::new(
		trigger_service,
		notification_service,
	));

	let monitors = monitor_service.get_all();
	let active_monitors = filter_active_monitors(monitors);
	let networks = network_service.get_all();

	Ok((
		filter_service,
		trigger_execution_service,
		active_monitors,
		networks,
	))
}

/// Creates a block handler function that processes new blocks from the blockchain.
///
/// # Arguments
/// * `shutdown_tx` - Broadcast channel for shutdown signals
/// * `trigger_service` - Service for executing triggers
/// * `filter_service` - Service for filtering blockchain data
/// * `active_monitors` - List of active monitors
///
/// # Returns
/// Returns a function that handles incoming blocks
fn create_block_handler(
	shutdown_tx: broadcast::Sender<()>,
	trigger_service: Arc<TriggerExecutionService<TriggerRepository>>,
	filter_service: Arc<FilterService>,
	active_monitors: Vec<Monitor>,
) -> Arc<impl Fn(&BlockType, &Network) + Send + Sync> {
	Arc::new(move |block: &BlockType, network: &Network| {
		let mut shutdown_rx = shutdown_tx.subscribe();
		let trigger_service = trigger_service.clone();
		let filter_service = filter_service.clone();
		let network = network.clone();
		let block = block.clone();
		let applicable_monitors = filter_network_monitors(&active_monitors, &network.slug);

		tokio::spawn(async move {
			if applicable_monitors.is_empty() {
				info!(
					"No monitors for network {} to process. Skipping block.",
					network.slug
				);
				return;
			}

			match network.network_type {
				BlockChainType::EVM => {
					let Ok(client) = EvmClient::new(&network).await else {
						error!("Failure while creating EVM client");
						return;
					};
					process_block(
						&client,
						&network,
						&block,
						&applicable_monitors,
						&filter_service,
						&trigger_service,
						&mut shutdown_rx,
					)
					.await;
				}

				BlockChainType::Stellar => {
					let Ok(client) = StellarClient::new(&network).await else {
						error!("Failure while creating Stellar client");
						return;
					};
					process_block(
						&client,
						&network,
						&block,
						&applicable_monitors,
						&filter_service,
						&trigger_service,
						&mut shutdown_rx,
					)
					.await;
				}
				BlockChainType::Midnight => unimplemented!("Midnight not implemented"),
				BlockChainType::Solana => unimplemented!("Solana not implemented"),
			}
		});
	})
}

/// Processes a single block for all applicable monitors.
///
/// # Arguments
/// * `network` - The network the block belongs to
/// * `block` - The block to process
/// * `applicable_monitors` - List of monitors that apply to this network
/// * `filter_service` - Service for filtering blockchain data
/// * `trigger_service` - Service for executing triggers
/// * `shutdown_rx` - Receiver for shutdown signals
async fn process_block<T>(
	client: &T,
	network: &Network,
	block: &BlockType,
	applicable_monitors: &[Monitor],
	filter_service: &FilterService,
	trigger_service: &TriggerExecutionService<TriggerRepository>,
	shutdown_rx: &mut broadcast::Receiver<()>,
) where
	T: BlockChainClient + BlockFilterFactory<T>,
{
	tokio::select! {
		result = filter_service.filter_block(client, network, block, applicable_monitors) => {
			match result {
				Ok(matches) => {
					for matching_monitor in matches {
						if let Err(e) = handle_match(matching_monitor, trigger_service).await {
							error!("Error handling match: {}", e);
						}
					}
				}
				Err(e) => error!("Error filtering block: {}", e),
			}
		}
		_ = shutdown_rx.recv() => {
			info!("Shutting down block processing task");
		}
	}
}

/// Checks if a network has any active monitors.
///
/// # Arguments
/// * `monitors` - List of monitors to check
/// * `network_slug` - Network identifier to check for
///
/// # Returns
/// Returns true if there are any active monitors for the given network
fn has_active_monitors(monitors: &[Monitor], network_slug: &String) -> bool {
	monitors.iter().any(|m| m.networks.contains(network_slug))
}

/// Filters out paused monitors from the provided collection.
///
/// # Arguments
/// * `monitors` - HashMap of monitors to filter
///
/// # Returns
/// Returns a vector containing only active (non-paused) monitors
fn filter_active_monitors(monitors: HashMap<String, Monitor>) -> Vec<Monitor> {
	monitors
		.into_values()
		.filter(|m| !m.paused)
		.collect::<Vec<_>>()
}

/// Filters monitors that are applicable to a specific network.
///
/// # Arguments
/// * `monitors` - List of monitors to filter
/// * `network_slug` - Network identifier to filter by
///
/// # Returns
/// Returns a vector of monitors that are configured for the specified network
fn filter_network_monitors(monitors: &[Monitor], network_slug: &String) -> Vec<Monitor> {
	monitors
		.iter()
		.filter(|m| m.networks.contains(network_slug))
		.cloned()
		.collect()
}
