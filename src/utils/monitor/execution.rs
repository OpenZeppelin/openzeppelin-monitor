//! Execution monitor module
//!
//! This module provides functionality to execute monitors against specific block numbers on blockchain networks.
use crate::{
	bootstrap::{get_contract_specs, has_active_monitors},
	models::{BlockChainType, ScriptLanguage},
	repositories::{
		MonitorRepositoryTrait, MonitorService, NetworkRepositoryTrait, NetworkService,
		TriggerRepositoryTrait,
	},
	services::{
		blockchain::{BlockChainClient, ClientPoolTrait},
		filter::{handle_match, FilterService},
		trigger::TriggerExecutionService,
	},
	utils::monitor::MonitorExecutionError,
};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::sync::Mutex;
use tracing::{info, instrument};

/// Configuration for executing a monitor
///
/// # Arguments
///
/// * `path` - The path to the monitor to execute
/// * `network_slug` - The network slug to execute the monitor against
/// * `block_number` - The block number to execute the monitor against
/// * `monitor_service` - The monitor service to use
/// * `network_service` - The network service to use
/// * `filter_service` - The filter service to use
/// * `trigger_execution_service` - The trigger execution service to use
/// * `active_monitors_trigger_scripts` - The active monitors trigger scripts to use
/// * `client_pool` - The client pool to use
pub struct MonitorExecutionConfig<
	M: MonitorRepositoryTrait<N, TR>,
	N: NetworkRepositoryTrait + Send + Sync + 'static,
	TR: TriggerRepositoryTrait + Send + Sync + 'static,
	CP: ClientPoolTrait + Send + Sync + 'static,
> {
	pub path: String,
	pub network_slug: Option<String>,
	pub block_number: Option<u64>,
	pub monitor_service: Arc<Mutex<MonitorService<M, N, TR>>>,
	pub network_service: Arc<Mutex<NetworkService<N>>>,
	pub filter_service: Arc<FilterService>,
	pub trigger_execution_service: Arc<TriggerExecutionService<TR>>,
	pub active_monitors_trigger_scripts: HashMap<String, (ScriptLanguage, String)>,
	pub client_pool: Arc<CP>,
}
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
#[allow(clippy::too_many_arguments)]
pub async fn execute_monitor<
	M: MonitorRepositoryTrait<N, TR>,
	N: NetworkRepositoryTrait + Send + Sync + 'static,
	TR: TriggerRepositoryTrait + Send + Sync + 'static,
	CP: ClientPoolTrait + Send + Sync + 'static,
>(
	config: MonitorExecutionConfig<M, N, TR, CP>,
) -> ExecutionResult<String> {
	tracing::debug!("Loading monitor configuration");
	let monitor = config
		.monitor_service
		.lock()
		.await
		.load_from_path(Some(Path::new(&config.path)), None, None)
		.await
		.map_err(|e| MonitorExecutionError::execution_error(e.to_string(), None, None))?;

	tracing::debug!(monitor_name = %monitor.name, "Monitor loaded successfully");

	let networks_for_monitor = if let Some(network_slug) = config.network_slug {
		tracing::debug!(network = %network_slug, "Finding specific network");
		let network = config
			.network_service
			.lock()
			.await
			.get(network_slug.as_str())
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
		config
			.network_service
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

		let contract_specs = get_contract_specs(
			&config.client_pool,
			&[(network.clone(), vec![monitor.clone()])],
		)
		.await;

		let matches = match network.network_type {
			BlockChainType::EVM => {
				let client = config
					.client_pool
					.get_evm_client(&network)
					.await
					.map_err(|e| {
						MonitorExecutionError::execution_error(
							format!("Failed to get EVM client: {}", e),
							None,
							None,
						)
					})?;

				let block_number = match config.block_number {
					Some(block_number) => {
						tracing::debug!(block = %block_number, "Using specified block number");
						block_number
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
				config
					.filter_service
					.filter_block(
						&*client,
						&network,
						block,
						&[monitor.clone()],
						Some(&contract_specs),
					)
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
				let client = config
					.client_pool
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
				let block_number = match config.block_number {
					Some(block_number) => block_number,
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

				config
					.filter_service
					.filter_block(
						&*client,
						&network,
						block,
						&[monitor.clone()],
						Some(&contract_specs),
					)
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
				));
			}
			BlockChainType::Solana => {
				return Err(MonitorExecutionError::execution_error(
					"Solana network not supported",
					None,
					None,
				));
			}
		};

		tracing::debug!(matches_count = matches.len(), "Found matches for network");
		all_matches.extend(matches);
	}

	// Send notifications for each match
	for match_result in all_matches.clone() {
		let result = handle_match(
			match_result,
			&*config.trigger_execution_service,
			&config.active_monitors_trigger_scripts,
		)
		.await;
		match result {
			Ok(_result) => info!("Successfully sent notifications for match"),
			Err(e) => {
				tracing::error!("Error sending notifications: {}", e);
				continue;
			}
		};
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
