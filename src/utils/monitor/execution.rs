//! Execution monitor module
//!
//! This module provides functionality to execute monitors against specific block numbers on blockchain networks.
use crate::{
	bootstrap::has_active_monitors,
	models::BlockChainType,
	repositories::{
		MonitorRepositoryTrait, MonitorService, NetworkRepositoryTrait, NetworkService,
		TriggerRepositoryTrait,
	},
	services::{
		blockchain::{BlockChainClient, ClientPoolTrait},
		filter::FilterService,
	},
	utils::monitor::MonitorExecutionError,
};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

pub type ExecutionResult<T> = std::result::Result<T, MonitorExecutionError>;

/// Executes a monitor against a specific block number on a blockchain network.
///
/// This function allows testing monitors by running them against historical blocks.
/// It supports both EVM and Stellar networks, retrieving the block data and applying
/// the monitor's filters to check for matches.
///
/// # Arguments
///
/// * `monitor_name` - The name of the monitor to execute
/// * `network_slug` - The network identifier to run the monitor against
/// * `block_number` - The specific block number to analyze
/// * `active_monitors` - List of currently active monitors
/// * `network_service` - The network service to use
/// * `filter_service` - The filter service to use
/// * `client_pool` - The client pool to use
///
/// # Returns
/// * `Result<String, ExecutionError>` - JSON string containing matches or error
#[instrument(skip_all)]
pub async fn execute_monitor<
	T: ClientPoolTrait,
	M: MonitorRepositoryTrait<N, TR>,
	N: NetworkRepositoryTrait,
	TR: TriggerRepositoryTrait,
>(
	monitor_path: &str,
	network_slug: Option<&String>,
	block_number: Option<&u64>,
	monitor_service: Arc<Mutex<MonitorService<M, N, TR>>>,
	network_service: Arc<Mutex<NetworkService<N>>>,
	filter_service: Arc<FilterService>,
	client_pool: T,
) -> ExecutionResult<String> {
	tracing::debug!("Loading monitor configuration");
	let monitor = monitor_service
		.lock()
		.await
		.load_from_path(Some(Path::new(monitor_path)), None, None)
		.map_err(|e| MonitorExecutionError::execution_error(e.to_string(), None, None))?;

	tracing::debug!(monitor_name = %monitor.name, "Monitor loaded successfully");

	let networks_for_monitor = if let Some(network_slug) = network_slug {
		tracing::debug!(network = %network_slug, "Finding specific network");
		let network = network_service
			.lock()
			.await
			.get(network_slug)
			.ok_or_else(|| {
				MonitorExecutionError::not_found(
					format!("Network '{}' not found", network_slug),
					None,
					None,
				)
			})?;
		vec![network.clone()]
	} else {
		tracing::debug!("Finding all active networks for monitor");
		network_service
			.lock()
			.await
			.get_all()
			.values()
			.filter(|network| has_active_monitors(&[monitor.clone()], &network.slug))
			.cloned()
			.collect()
	};

	tracing::debug!(
		networks_count = networks_for_monitor.len(),
		"Networks found for monitor"
	);

	let mut all_matches = Vec::new();
	for network in networks_for_monitor {
		tracing::debug!(
			network_type = ?network.network_type,
			network_slug = %network.slug,
			"Processing network"
		);

		let matches = match network.network_type {
			BlockChainType::EVM => {
				let client = client_pool.get_evm_client(&network).await.map_err(|e| {
					MonitorExecutionError::execution_error(
						format!("Failed to get EVM client: {}", e),
						None,
						None,
					)
				})?;

				let block_number = match block_number {
					Some(block_number) => {
						tracing::debug!(block = %block_number, "Using specified block number");
						*block_number
					}
					None => {
						let latest = client.get_latest_block_number().await.map_err(|e| {
							MonitorExecutionError::execution_error(e.to_string(), None, None)
						})?;
						tracing::debug!(block = %latest, "Using latest block number");
						latest
					}
				};

				tracing::debug!(block = %block_number, "Fetching block");
				let blocks = client.get_blocks(block_number, None).await.map_err(|e| {
					MonitorExecutionError::execution_error(
						format!("Failed to get block {}: {}", block_number, e),
						None,
						None,
					)
				})?;

				let block = blocks.first().ok_or_else(|| {
					MonitorExecutionError::not_found(
						format!("Block {} not found", block_number),
						None,
						None,
					)
				})?;

				tracing::debug!(block = %block_number, "Filtering block");
				filter_service
					.filter_block(&*client, &network, block, &[monitor.clone()])
					.await
					.map_err(|e| {
						MonitorExecutionError::execution_error(
							format!("Failed to filter block: {}", e),
							None,
							None,
						)
					})?
			}
			BlockChainType::Stellar => {
				let client = client_pool
					.get_stellar_client(&network)
					.await
					.map_err(|e| {
						MonitorExecutionError::execution_error(
							format!("Failed to get Stellar client: {}", e),
							None,
							None,
						)
					})?;

				// If block number is not provided, get the latest block number
				let block_number = match block_number {
					Some(block_number) => *block_number,
					None => client.get_latest_block_number().await.map_err(|e| {
						MonitorExecutionError::execution_error(e.to_string(), None, None)
					})?,
				};

				let blocks = client.get_blocks(block_number, None).await.map_err(|e| {
					MonitorExecutionError::execution_error(
						format!("Failed to get block {}: {}", block_number, e),
						None,
						None,
					)
				})?;

				let block = blocks.first().ok_or_else(|| {
					MonitorExecutionError::not_found(
						format!("Block {} not found", block_number),
						None,
						None,
					)
				})?;

				filter_service
					.filter_block(&*client, &network, block, &[monitor.clone()])
					.await
					.map_err(|e| {
						MonitorExecutionError::execution_error(
							format!("Failed to filter block: {}", e),
							None,
							None,
						)
					})?
			}
			BlockChainType::Midnight => {
				return Err(MonitorExecutionError::execution_error(
					"Midnight network not supported",
					None,
					None,
				))
			}
		};

		tracing::debug!(matches_count = matches.len(), "Found matches for network");
		all_matches.extend(matches);
	}

	tracing::debug!(total_matches = all_matches.len(), "Serializing results");
	let json_matches = serde_json::to_string(&all_matches).map_err(|e| {
		MonitorExecutionError::execution_error(
			format!("Failed to serialize matches: {}", e),
			None,
			None,
		)
	})?;

	tracing::debug!("Monitor execution completed successfully");
	Ok(json_matches)
}
