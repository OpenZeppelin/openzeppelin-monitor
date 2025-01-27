use futures::future::BoxFuture;
use log::{error, info};
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::watch;

use crate::{
	models::{BlockChainType, BlockType, Monitor, MonitorMatch, Network, ProcessedBlock},
	repositories::{
		MonitorRepositoryTrait, MonitorService, NetworkRepositoryTrait, NetworkService,
		RepositoryError, TriggerRepositoryTrait, TriggerService,
	},
	services::{
		blockchain::{BlockChainClient, BlockFilterFactory, EvmClient, StellarClient},
		filter::{handle_match, FilterService},
		notification::NotificationService,
		trigger::{TriggerExecutionService, TriggerExecutionServiceTrait},
	},
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
type ServiceResult<T> = Result<(
	Arc<FilterService>,
	Arc<TriggerExecutionService<T>>,
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
pub fn initialize_services<M, N, T>(
	monitor_service: Option<MonitorService<M, N, T>>,
	network_service: Option<NetworkService<N>>,
	trigger_service: Option<TriggerService<T>>,
) -> ServiceResult<T>
where
	M: MonitorRepositoryTrait<N, T>,
	N: NetworkRepositoryTrait,
	T: TriggerRepositoryTrait,
{
	let network_service = match network_service {
		Some(service) => service,
		None => {
			let repository =
				N::new(None).map_err(|_| RepositoryError::load_error("Unable to load networks"))?;
			NetworkService::<N>::new_with_repository(repository)?
		}
	};

	let trigger_service = match trigger_service {
		Some(service) => service,
		None => {
			let repository =
				T::new(None).map_err(|_| RepositoryError::load_error("Unable to load triggers"))?;
			TriggerService::<T>::new_with_repository(repository)?
		}
	};

	let monitor_service = match monitor_service {
		Some(service) => service,
		None => {
			let repository = M::new(
				None,
				Some(network_service.clone()),
				Some(trigger_service.clone()),
			)
			.map_err(|_| RepositoryError::load_error("Unable to load monitors"))?;
			MonitorService::<M, N, T>::new_with_repository(repository)?
		}
	};

	let notification_service = NotificationService::new();

	let filter_service = Arc::new(FilterService::new());
	let trigger_execution_service = Arc::new(TriggerExecutionService::new(
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
/// * `shutdown_tx` - Watch channel for shutdown signals
/// * `filter_service` - Service for filtering blockchain data
/// * `active_monitors` - List of active monitors
///
/// # Returns
/// Returns a function that handles incoming blocks
pub fn create_block_handler(
	shutdown_tx: watch::Sender<bool>,
	filter_service: Arc<FilterService>,
	active_monitors: Vec<Monitor>,
) -> Arc<impl Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync> {
	Arc::new(
		move |block: BlockType, network: Network| -> BoxFuture<'static, ProcessedBlock> {
			let filter_service = filter_service.clone();
			let active_monitors = active_monitors.clone();
			let shutdown_tx = shutdown_tx.clone();
			Box::pin(async move {
				let applicable_monitors = filter_network_monitors(&active_monitors, &network.slug);

				let mut processed_block = ProcessedBlock {
					block_number: block.number().unwrap_or(0),
					network_slug: network.slug.clone(),
					processing_results: Vec::new(),
				};

				if !applicable_monitors.is_empty() {
					let mut shutdown_rx = shutdown_tx.subscribe();

					let matches = match network.network_type {
						BlockChainType::EVM => {
							if let Ok(client) = EvmClient::new(&network).await {
								process_block(
									&client,
									&network,
									&block,
									&applicable_monitors,
									&filter_service,
									&mut shutdown_rx,
								)
								.await
								.unwrap_or_default()
							} else {
								Vec::new()
							}
						}
						BlockChainType::Stellar => {
							if let Ok(client) = StellarClient::new(&network).await {
								process_block(
									&client,
									&network,
									&block,
									&applicable_monitors,
									&filter_service,
									&mut shutdown_rx,
								)
								.await
								.unwrap_or_default()
							} else {
								Vec::new()
							}
						}
						BlockChainType::Midnight => Vec::new(), // unimplemented
						BlockChainType::Solana => Vec::new(),   // unimplemented
					};

					processed_block.processing_results = matches;
				}

				processed_block
			})
		},
	)
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
pub async fn process_block<T>(
	client: &T,
	network: &Network,
	block: &BlockType,
	applicable_monitors: &[Monitor],
	filter_service: &FilterService,
	shutdown_rx: &mut watch::Receiver<bool>,
) -> Option<Vec<MonitorMatch>>
where
	T: BlockChainClient + BlockFilterFactory<T>,
{
	tokio::select! {
		result = filter_service.filter_block(client, network, block, applicable_monitors) => {
			match result {
				Ok(matches) => Some(matches),
				Err(e) => {
					error!("Error filtering block: {}", e);
					None
				}
			}
		}
		_ = shutdown_rx.changed() => {
			info!("Shutting down block processing task");
			None
		}
	}
}

/// Creates a trigger handler function that processes trigger events from the block processing
/// pipeline.
///
/// # Arguments
/// * `shutdown_tx` - Watch channel for shutdown signals
/// * `trigger_service` - Service for executing triggers
///
/// # Returns
/// Returns a function that handles trigger execution for matching monitors
pub fn create_trigger_handler<S: TriggerExecutionServiceTrait + Send + Sync + 'static>(
	shutdown_tx: watch::Sender<bool>,
	trigger_service: Arc<S>,
) -> Arc<impl Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync> {
	Arc::new(move |block: &ProcessedBlock| {
		let mut shutdown_rx = shutdown_tx.subscribe();
		let trigger_service = trigger_service.clone();
		let block = block.clone();
		tokio::spawn(async move {
			tokio::select! {
				_ = async {
					for monitor_match in &block.processing_results {
						if let Err(e) = handle_match(monitor_match.clone(), &*trigger_service).await {
							error!("Error handling trigger: {}", e);
						}
					}
				} => {}
				_ = shutdown_rx.changed() => {
					info!("Shutting down trigger handling task");
				}
			}
		})
	})
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
			create_test_monitor("2", vec!["stellar_mainnet"], true),
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
