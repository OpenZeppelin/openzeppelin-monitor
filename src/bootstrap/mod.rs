//! Bootstrap module for initializing services and creating handlers.
//!
//! This module provides functions to initialize the necessary services and create handlers for
//! processing blocks and triggers. It also includes helper functions for filtering and processing
//! monitors and networks.
//!
//! # Services
//! - `FilterService`: Handles filtering of blockchain data
//! - `TriggerExecutionService`: Manages trigger execution
//! - `NotificationService`: Handles notifications
//!
//! # Handlers
//! - `create_block_handler`: Creates a block handler function that processes new blocks from the
//!   blockchain
//! - `create_trigger_handler`: Creates a trigger handler function that processes trigger events
//!   from the block processing pipeline

use futures::future::BoxFuture;
use log::{error, info, warn};
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::{
	sync::{watch, Semaphore},
	time::Duration,
};

use crate::{
	models::{
		BlockChainType, BlockType, Monitor, MonitorMatch, Network, ProcessedBlock,
		TriggerConditions,
	},
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
	utils::script::{ScriptError, ScriptExecutorFactory},
};

/// Type alias for handling ServiceResult
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
/// - `Vec<Monitor>`: List of active monitors
/// - `HashMap<String, Network>`: Available networks indexed by slug
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
					let filtered_matches = run_trigger_filters(&block.processing_results, &block.network_slug).await;
					for monitor_match in &filtered_matches {
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

async fn execute_trigger_condition(
	trigger_condition: &TriggerConditions,
	monitor_match: &MonitorMatch,
) -> bool {
	let executor =
		ScriptExecutorFactory::create(&trigger_condition.language, &trigger_condition.script_path);

	match tokio::time::timeout(
		Duration::from_millis(u64::from(trigger_condition.timeout_ms)),
		executor.execute(monitor_match.clone()),
	)
	.await
	{
		Ok(Ok(false)) => true,
		Err(e) => {
			ScriptError::execution_error(e.to_string());
			false
		}
		Ok(Err(e)) => {
			ScriptError::execution_error(e.to_string());
			false
		}
		_ => false,
	}
}

async fn run_trigger_filters(matches: &[MonitorMatch], _network: &str) -> Vec<MonitorMatch> {
	let mut filtered_matches = vec![];
	// We are running this function for every block, so we need to limit the number of concurrent
	// open files to avoid running out of file descriptors
	const MAX_CONCURRENT_OPEN_FILES: usize = 100;
	// Create a semaphore to limit concurrent script executions
	static PERMITS: Semaphore = Semaphore::const_new(MAX_CONCURRENT_OPEN_FILES);

	for monitor_match in matches {
		let mut is_filtered = false;
		let trigger_conditions = match monitor_match {
			MonitorMatch::EVM(evm_match) => &evm_match.monitor.trigger_conditions,
			MonitorMatch::Stellar(stellar_match) => &stellar_match.monitor.trigger_conditions,
		};

		let mut sorted_conditions = trigger_conditions.clone();
		sorted_conditions.sort_by_key(|condition| condition.execution_order);

		for trigger_condition in sorted_conditions {
			// Acquire semaphore permit, this will block if the semaphore is full
			let _permit = match PERMITS.acquire().await {
				Ok(permit) => permit,
				Err(e) => {
					ScriptError::system_error(e.to_string());
					continue;
				}
			};

			if execute_trigger_condition(&trigger_condition, monitor_match).await {
				is_filtered = true;
				break;
			}
		}

		if !is_filtered {
			filtered_matches.push(monitor_match.clone());
		}
	}
	filtered_matches
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch, ScriptLanguage,
	};
	use std::io::Write;
	use tempfile::NamedTempFile;
	use web3::types::{TransactionReceipt, H160, U256};

	// Helper function to create a temporary script file
	fn create_temp_script(content: &str) -> NamedTempFile {
		let mut file = NamedTempFile::new().unwrap();
		file.write_all(content.as_bytes()).unwrap();
		file
	}
	fn create_test_monitor(
		name: &str,
		networks: Vec<&str>,
		paused: bool,
		script_path: Option<&str>,
	) -> Monitor {
		Monitor {
			name: name.to_string(),
			networks: networks.into_iter().map(|s| s.to_string()).collect(),
			paused,
			trigger_conditions: vec![TriggerConditions {
				language: ScriptLanguage::Python,
				script_path: script_path.unwrap_or("test.py").to_string(),
				execution_order: Some(0),
				timeout_ms: 1000,
				arguments: None,
			}],
			..Default::default()
		}
	}

	fn create_test_evm_transaction() -> EVMTransaction {
		EVMTransaction::from({
			web3::types::Transaction {
				from: Some(H160::default()),
				to: Some(H160::default()),
				value: U256::default(),
				..Default::default()
			}
		})
	}

	fn create_mock_monitor_match(script_path: Option<&str>) -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test", vec![], false, script_path),
			transaction: create_test_evm_transaction(),
			receipt: TransactionReceipt::default(),
			matched_on: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			matched_on_args: None,
		}))
	}

	fn matches_equal(a: &MonitorMatch, b: &MonitorMatch) -> bool {
		match (a, b) {
			(MonitorMatch::EVM(a), MonitorMatch::EVM(b)) => a.monitor.name == b.monitor.name,
			(MonitorMatch::Stellar(a), MonitorMatch::Stellar(b)) => {
				a.monitor.name == b.monitor.name
			}
			_ => false,
		}
	}

	#[test]
	fn test_has_active_monitors() {
		let monitors = vec![
			create_test_monitor("1", vec!["ethereum_mainnet"], false, None),
			create_test_monitor("2", vec!["ethereum_sepolia"], false, None),
			create_test_monitor(
				"3",
				vec!["ethereum_mainnet", "ethereum_sepolia"],
				false,
				None,
			),
			create_test_monitor("4", vec!["stellar_mainnet"], true, None),
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
			create_test_monitor("1", vec!["ethereum_mainnet"], false, None),
		);
		monitors.insert(
			"2".to_string(),
			create_test_monitor("2", vec!["stellar_mainnet"], true, None),
		);
		monitors.insert(
			"3".to_string(),
			create_test_monitor("3", vec!["ethereum_mainnet"], false, None),
		);

		let active_monitors = filter_active_monitors(monitors);
		assert_eq!(active_monitors.len(), 2);
		assert!(active_monitors.iter().all(|m| !m.paused));
	}

	#[test]
	fn test_filter_network_monitors() {
		let monitors = vec![
			create_test_monitor("1", vec!["ethereum_mainnet"], false, None),
			create_test_monitor("2", vec!["stellar_mainnet"], true, None),
			create_test_monitor(
				"3",
				vec!["ethereum_mainnet", "stellar_mainnet"],
				false,
				None,
			),
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

	#[tokio::test]
	async fn test_run_trigger_filters_empty_matches() {
		let matches: Vec<MonitorMatch> = vec![];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet").await;
		assert!(filtered.is_empty());
	}

	#[tokio::test]
	async fn test_run_trigger_filters_true_condition() {
		let script_content = r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print("debugging...")
def test():
    return True
result = test()
print(result)
"#;
		let temp_file = create_temp_script(script_content);
		let match_item = create_mock_monitor_match(Some(temp_file.path().to_str().unwrap()));
		let matches = vec![match_item.clone()];

		let filtered = run_trigger_filters(&matches, "ethereum_mainnet").await;
		assert_eq!(filtered.len(), 1);
		assert!(matches_equal(&filtered[0], &match_item));
	}

	#[tokio::test]
	async fn test_run_trigger_filters_false_condition() {
		let script_content = r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print("debugging...")
def test():
    return False
result = test()
print(result)
"#;
		let temp_file = create_temp_script(script_content);
		let match_item = create_mock_monitor_match(Some(temp_file.path().to_str().unwrap()));
		let matches = vec![match_item.clone()];

		let filtered = run_trigger_filters(&matches, "ethereum_mainnet").await;
		assert_eq!(filtered.len(), 0);
	}
}
