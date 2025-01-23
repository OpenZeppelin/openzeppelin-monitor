use std::{collections::HashMap, error::Error, sync::Arc};

use log::{error, info};
use tokio::sync::broadcast;

use crate::{
	models::{BlockChainType, BlockType, Monitor, Network},
	repositories::{
		MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
		TriggerService,
	},
	services::{
		blockchain::{BlockChainClient, BlockFilterFactory, EvmClient, StellarClient},
		filter::{handle_match, FilterService},
		notification::NotificationService,
		trigger::TriggerExecutionService,
	},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
type ServiceResult = Result<(
	Arc<FilterService>,
	Arc<TriggerExecutionService<TriggerRepository>>,
	Vec<Monitor>,
	HashMap<String, Network>,
)>;

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
pub fn initialize_services() -> ServiceResult {
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
pub fn create_block_handler(
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
pub fn has_active_monitors(monitors: &[Monitor], network_slug: &String) -> bool {
	monitors
		.iter()
		.any(|m| m.networks.contains(network_slug) && !m.paused)
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

#[cfg(test)]
mod tests {
	use super::*;

	fn create_test_monitor(name: &str, networks: Vec<&str>, paused: bool) -> Monitor {
		Monitor {
			name: name.to_string(),
			networks: networks.into_iter().map(|s| s.to_string()).collect(),
			paused,
			..Default::default()
		}
	}

	#[test]
	fn test_has_active_monitors() {
		let monitors = vec![
			create_test_monitor("1", vec!["ethereum_mainnet"], false),
			create_test_monitor("2", vec!["ethereum_sepolia"], false),
			create_test_monitor("3", vec!["ethereum_mainnet", "ethereum_sepolia"], false),
			create_test_monitor("4", vec!["stellar_mainnet"], true),
		];

		assert!(has_active_monitors(
			&monitors,
			&"ethereum_mainnet".to_string()
		));
		assert!(has_active_monitors(
			&monitors,
			&"ethereum_sepolia".to_string()
		));
		assert!(!has_active_monitors(
			&monitors,
			&"solana_mainnet".to_string()
		));
		assert!(!has_active_monitors(
			&monitors,
			&"stellar_mainnet".to_string()
		));
	}

	#[test]
	fn test_filter_active_monitors() {
		let mut monitors = HashMap::new();
		monitors.insert(
			"1".to_string(),
			create_test_monitor("1", vec!["ethereum_mainnet"], false),
		);
		monitors.insert(
			"2".to_string(),
			create_test_monitor("2", vec!["stellar_mainnet"], true),
		);
		monitors.insert(
			"3".to_string(),
			create_test_monitor("3", vec!["ethereum_mainnet"], false),
		);

		let active_monitors = filter_active_monitors(monitors);
		assert_eq!(active_monitors.len(), 2);
		assert!(active_monitors.iter().all(|m| !m.paused));
	}

	#[test]
	fn test_filter_network_monitors() {
		let monitors = vec![
			create_test_monitor("1", vec!["ethereum_mainnet"], false),
			create_test_monitor("2", vec!["stellar_mainnet"], false),
			create_test_monitor("3", vec!["ethereum_mainnet", "stellar_mainnet"], false),
		];

		let eth_monitors = filter_network_monitors(&monitors, &"ethereum_mainnet".to_string());
		assert_eq!(eth_monitors.len(), 2);
		assert!(eth_monitors
			.iter()
			.all(|m| m.networks.contains(&"ethereum_mainnet".to_string())));

		let stellar_monitors = filter_network_monitors(&monitors, &"stellar_mainnet".to_string());
		assert_eq!(stellar_monitors.len(), 2);
		assert!(stellar_monitors
			.iter()
			.all(|m| m.networks.contains(&"stellar_mainnet".to_string())));

		let sol_monitors = filter_network_monitors(&monitors, &"solana_mainnet".to_string());
		assert!(sol_monitors.is_empty());
	}
}
