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
use std::{collections::HashMap, error::Error, sync::Arc};
use tokio::sync::{watch, Mutex};

use crate::{
	models::{
		BlockChainType, BlockType, ContractSpec, Monitor, MonitorMatch, Network, ProcessedBlock,
		ScriptLanguage, TriggerConditions,
	},
	repositories::{
		MonitorRepositoryTrait, MonitorService, NetworkRepositoryTrait, NetworkService,
		TriggerRepositoryTrait, TriggerService,
	},
	services::{
		blockchain::{BlockChainClient, BlockFilterFactory, ClientPoolTrait},
		filter::{evm_helpers, handle_match, stellar_helpers, FilterService},
		notification::NotificationService,
		trigger::{
			ScriptError, ScriptExecutorFactory, TriggerError, TriggerExecutionService,
			TriggerExecutionServiceTrait,
		},
	},
	utils::normalize_string,
};

/// Type alias for handling ServiceResult
pub type Result<T> = std::result::Result<T, Box<dyn Error>>;

type ServiceResult<M, N, T> = Result<(
	Arc<FilterService>,
	Arc<TriggerExecutionService<T>>,
	Vec<Monitor>,
	HashMap<String, Network>,
	Arc<Mutex<MonitorService<M, N, T>>>,
	Arc<Mutex<NetworkService<N>>>,
	Arc<Mutex<TriggerService<T>>>,
)>;

/// Initializes all required services for the blockchain monitor.
///
/// # Returns
/// Returns a tuple containing:
/// - FilterService: Handles filtering of blockchain data
/// - TriggerExecutionService: Manages trigger execution
/// - `Vec<Monitor>`: List of active monitors
/// - `HashMap<String, Network>`: Available networks indexed by slug
/// - `Arc<Mutex<M>>`: Data access for monitor configs
/// - `Arc<Mutex<N>>`: Data access for network configs
/// - `Arc<Mutex<T>>`: Data access for trigger configs
/// # Errors
/// Returns an error if any service initialization fails
pub async fn initialize_services<M, N, T>(
	monitor_service: Option<MonitorService<M, N, T>>,
	network_service: Option<NetworkService<N>>,
	trigger_service: Option<TriggerService<T>>,
) -> ServiceResult<M, N, T>
where
	M: MonitorRepositoryTrait<N, T> + Send + Sync + 'static,
	N: NetworkRepositoryTrait + Send + Sync + 'static,
	T: TriggerRepositoryTrait + Send + Sync + 'static,
{
	let network_service = match network_service {
		Some(service) => service,
		None => {
			let repository = N::new(None).await?;
			NetworkService::<N>::new_with_repository(repository)?
		}
	};

	let trigger_service = match trigger_service {
		Some(service) => service,
		None => {
			let repository = T::new(None).await?;
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
			.await?;
			MonitorService::<M, N, T>::new_with_repository(repository)?
		}
	};

	let notification_service = NotificationService::new();

	let filter_service = Arc::new(FilterService::new());
	let trigger_execution_service = Arc::new(TriggerExecutionService::new(
		trigger_service.clone(),
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
		Arc::new(Mutex::new(monitor_service)),
		Arc::new(Mutex::new(network_service)),
		Arc::new(Mutex::new(trigger_service)),
	))
}

/// Creates a block handler function that processes new blocks from the blockchain.
///
/// # Arguments
/// * `shutdown_tx` - Watch channel for shutdown signals
/// * `filter_service` - Service for filtering blockchain data
/// * `active_monitors` - List of active monitors
/// * `client_pools` - Client pools for accessing blockchain clients
///
/// # Returns
/// Returns a function that handles incoming blocks
pub fn create_block_handler<P: ClientPoolTrait + 'static>(
	shutdown_tx: watch::Sender<bool>,
	filter_service: Arc<FilterService>,
	active_monitors: Vec<Monitor>,
	client_pools: Arc<P>,
	contract_specs: Vec<(String, ContractSpec)>,
) -> Arc<impl Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync> {
	Arc::new(
		move |block: BlockType, network: Network| -> BoxFuture<'static, ProcessedBlock> {
			let filter_service = filter_service.clone();
			let active_monitors = active_monitors.clone();
			let client_pools = client_pools.clone();
			let shutdown_tx = shutdown_tx.clone();
			let contract_specs = contract_specs.clone();
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
						BlockChainType::EVM => match client_pools.get_evm_client(&network).await {
							Ok(client) => {
								process_block(
									client.as_ref(),
									&network,
									&block,
									&applicable_monitors,
									Some(&contract_specs),
									&filter_service,
									&mut shutdown_rx,
								)
								.await
							}
							Err(_) => None,
						},
						BlockChainType::Stellar => {
							match client_pools.get_stellar_client(&network).await {
								Ok(client) => {
									process_block(
										client.as_ref(),
										&network,
										&block,
										&applicable_monitors,
										Some(&contract_specs),
										&filter_service,
										&mut shutdown_rx,
									)
									.await
								}
								Err(_) => None,
							}
						}
						BlockChainType::Midnight => None,
						BlockChainType::Solana => None,
					};

					processed_block.processing_results = matches.unwrap_or_default();
				}

				processed_block
			})
		},
	)
}

/// Processes a single block for all applicable monitors.
///
/// # Arguments
/// * `client` - The client to use to process the block
/// * `network` - The network the block belongs to
/// * `block` - The block to process
/// * `applicable_monitors` - List of monitors that apply to this network
/// * `filter_service` - Service for filtering blockchain data
/// * `shutdown_rx` - Receiver for shutdown signals
pub async fn process_block<T>(
	client: &T,
	network: &Network,
	block: &BlockType,
	applicable_monitors: &[Monitor],
	contract_specs: Option<&[(String, ContractSpec)]>,
	filter_service: &FilterService,
	shutdown_rx: &mut watch::Receiver<bool>,
) -> Option<Vec<MonitorMatch>>
where
	T: BlockChainClient + BlockFilterFactory<T>,
{
	tokio::select! {
		result = filter_service.filter_block(client, network, block, applicable_monitors, contract_specs) => {
			result.ok()
		}
		_ = shutdown_rx.changed() => {
			tracing::info!("Shutting down block processing task");
			None
		}
	}
}

/// Get contract specs for all applicable monitors
///
/// # Arguments
/// * `client_pool` - The client pool to use to get the contract specs
/// * `network_monitors` - The monitors to get the contract specs for
///
/// # Returns
/// Returns a vector of contract specs
pub async fn get_contract_specs<P: ClientPoolTrait + 'static>(
	client_pool: &Arc<P>,
	network_monitors: &[(Network, Vec<Monitor>)],
) -> Vec<(String, ContractSpec)> {
	let mut all_specs = Vec::new();

	for (network, monitors) in network_monitors {
		for monitor in monitors {
			let specs = match network.network_type {
				BlockChainType::Stellar => {
					let mut contract_specs = Vec::new();
					let mut addresses_without_specs = Vec::new();
					// First collect addresses that have contract specs configured in the monitor
					for monitored_addr in &monitor.addresses {
						if let Some(spec) = &monitored_addr.contract_spec {
							let parsed_spec = match spec {
								ContractSpec::Stellar(spec) => spec,
								_ => {
									tracing::warn!(
										"Skipping non-Stellar contract spec for address {}",
										monitored_addr.address
									);
									continue;
								}
							};

							contract_specs.push((
								stellar_helpers::normalize_address(&monitored_addr.address),
								ContractSpec::Stellar(parsed_spec.clone()),
							))
						} else {
							addresses_without_specs.push(monitored_addr.address.clone());
						}
					}

					// Fetch remaining specs from chain
					if !addresses_without_specs.is_empty() {
						// Get the client once
						let client: Arc<P::StellarClient> =
							match client_pool.get_stellar_client(network).await {
								Ok(client) => client,
								Err(_) => {
									tracing::warn!("Failed to get stellar client");
									continue;
								}
							};

						let chain_specs = futures::future::join_all(
							addresses_without_specs.iter().map(|address| {
								let client = client.clone();
								async move {
									let spec = client.get_contract_spec(address).await;
									(address.clone(), spec)
								}
							}),
						)
						.await
						.into_iter()
						.filter_map(|(addr, spec)| match spec {
							Ok(s) => Some((addr, s)),
							Err(e) => {
								tracing::warn!(
									"Failed to fetch contract spec for address {}: {:?}",
									addr,
									e
								);
								None
							}
						})
						.collect::<Vec<_>>();

						contract_specs.extend(chain_specs);
					}
					contract_specs
				}
				BlockChainType::EVM => {
					let mut contract_specs = Vec::new();
					// First collect addresses that have contract specs configured in the monitor
					for monitored_addr in &monitor.addresses {
						if let Some(spec) = &monitored_addr.contract_spec {
							let parsed_spec = match spec {
								ContractSpec::EVM(spec) => spec,
								_ => {
									tracing::warn!(
										"Skipping non-EVM contract spec for address {}",
										monitored_addr.address
									);
									continue;
								}
							};

							contract_specs.push((
								format!(
									"0x{}",
									evm_helpers::normalize_address(&monitored_addr.address)
								),
								ContractSpec::EVM(parsed_spec.clone()),
							))
						}
					}
					contract_specs
				}
				_ => {
					vec![]
				}
			};
			all_specs.extend(specs);
		}
	}
	all_specs
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
	active_monitors_trigger_scripts: HashMap<String, (ScriptLanguage, String)>,
) -> Arc<impl Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync> {
	Arc::new(move |block: &ProcessedBlock| {
		let mut shutdown_rx = shutdown_tx.subscribe();
		let trigger_service = trigger_service.clone();
		let trigger_scripts = active_monitors_trigger_scripts.clone();
		let block = block.clone();

		tokio::spawn(async move {
			tokio::select! {
				_ = async {
					if block.processing_results.is_empty() {
						return;
					}
					let filtered_matches = run_trigger_filters(&block.processing_results, &block.network_slug, &trigger_scripts).await;
					for monitor_match in &filtered_matches {
						if let Err(e) = handle_match(monitor_match.clone(), &*trigger_service, &trigger_scripts).await {
							TriggerError::execution_error(e.to_string(), Some(e.into()), None);
						}
					}
				} => {}
				_ = shutdown_rx.changed() => {
					tracing::info!("Shutting down trigger handling task");
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
	script_content: &(ScriptLanguage, String),
) -> bool {
	let executor = ScriptExecutorFactory::create(&script_content.0, &script_content.1);

	let result = executor
		.execute(
			monitor_match.clone(),
			&trigger_condition.timeout_ms,
			trigger_condition.arguments.as_deref(),
			false,
		)
		.await;

	match result {
		Ok(true) => true,
		Err(e) => {
			ScriptError::execution_error(e.to_string(), None, None);
			false
		}
		_ => false,
	}
}

async fn run_trigger_filters(
	matches: &[MonitorMatch],
	_network: &str,
	trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
) -> Vec<MonitorMatch> {
	let mut filtered_matches = vec![];

	for monitor_match in matches {
		let mut is_filtered = false;
		let trigger_conditions = match monitor_match {
			MonitorMatch::EVM(evm_match) => &evm_match.monitor.trigger_conditions,
			MonitorMatch::Stellar(stellar_match) => &stellar_match.monitor.trigger_conditions,
		};

		for trigger_condition in trigger_conditions {
			let monitor_name = match monitor_match {
				MonitorMatch::EVM(evm_match) => evm_match.monitor.name.clone(),
				MonitorMatch::Stellar(stellar_match) => stellar_match.monitor.name.clone(),
			};

			let script_content = trigger_scripts
				.get(&format!(
					"{}|{}",
					normalize_string(&monitor_name),
					trigger_condition.script_path
				))
				.ok_or_else(|| {
					ScriptError::execution_error("Script content not found".to_string(), None, None)
				});
			if let Ok(script_content) = script_content {
				if execute_trigger_condition(trigger_condition, monitor_match, script_content).await
				{
					is_filtered = true;
					break;
				}
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
	use crate::{
		models::{
			EVMMonitorMatch, EVMReceiptLog, EVMTransaction, EVMTransactionReceipt, MatchConditions,
			Monitor, MonitorMatch, ScriptLanguage, StellarBlock, StellarMonitorMatch,
			StellarTransaction, StellarTransactionInfo, TriggerConditions,
		},
		utils::tests::{builders::evm::monitor::MonitorBuilder, evm::receipt::ReceiptBuilder},
	};
	use alloy::{
		consensus::{transaction::Recovered, Signed, TxEnvelope},
		primitives::{Address, Bytes, TxKind, B256, U256},
	};
	use std::io::Write;
	use tempfile::NamedTempFile;

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
		let mut builder = MonitorBuilder::new()
			.name(name)
			.networks(networks.into_iter().map(|s| s.to_string()).collect())
			.paused(paused);

		if let Some(path) = script_path {
			builder = builder.trigger_condition(path, 1000, ScriptLanguage::Python, None);
		}

		builder.build()
	}

	fn create_test_evm_transaction_receipt() -> EVMTransactionReceipt {
		ReceiptBuilder::new().build()
	}

	fn create_test_evm_logs() -> Vec<EVMReceiptLog> {
		ReceiptBuilder::new().build().logs.clone()
	}

	fn create_test_evm_transaction() -> EVMTransaction {
		let tx = alloy::consensus::TxLegacy {
			chain_id: None,
			nonce: 0,
			gas_price: 0,
			gas_limit: 0,
			to: TxKind::Call(Address::ZERO),
			value: U256::ZERO,
			input: Bytes::default(),
		};

		let signature =
			alloy::signers::Signature::from_scalars_and_parity(B256::ZERO, B256::ZERO, false);

		let hash = B256::ZERO;

		EVMTransaction::from(alloy::rpc::types::Transaction {
			inner: Recovered::new_unchecked(
				TxEnvelope::Legacy(Signed::new_unchecked(tx, signature, hash)),
				Address::ZERO,
			),
			block_hash: None,
			block_number: None,
			transaction_index: None,
			effective_gas_price: None,
		})
	}

	fn create_test_stellar_transaction() -> StellarTransaction {
		StellarTransaction::from({
			StellarTransactionInfo {
				..Default::default()
			}
		})
	}

	fn create_test_stellar_block() -> StellarBlock {
		StellarBlock::default()
	}

	fn create_mock_monitor_match_from_path(
		blockchain_type: BlockChainType,
		script_path: Option<&str>,
	) -> MonitorMatch {
		match blockchain_type {
			BlockChainType::EVM => MonitorMatch::EVM(Box::new(EVMMonitorMatch {
				monitor: create_test_monitor("test", vec![], false, script_path),
				transaction: create_test_evm_transaction(),
				receipt: Some(create_test_evm_transaction_receipt()),
				logs: Some(create_test_evm_logs()),
				network_slug: "ethereum_mainnet".to_string(),
				matched_on: MatchConditions {
					functions: vec![],
					events: vec![],
					transactions: vec![],
				},
				matched_on_args: None,
			})),
			BlockChainType::Stellar => MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
				monitor: create_test_monitor("test", vec![], false, script_path),
				transaction: create_test_stellar_transaction(),
				ledger: create_test_stellar_block(),
				network_slug: "stellar_mainnet".to_string(),
				matched_on: MatchConditions {
					functions: vec![],
					events: vec![],
					transactions: vec![],
				},
				matched_on_args: None,
			})),
			BlockChainType::Midnight => unimplemented!(),
			BlockChainType::Solana => unimplemented!(),
		}
	}

	fn create_mock_monitor_match_from_monitor(
		blockchain_type: BlockChainType,
		monitor: Monitor,
	) -> MonitorMatch {
		match blockchain_type {
			BlockChainType::EVM => MonitorMatch::EVM(Box::new(EVMMonitorMatch {
				monitor,
				transaction: create_test_evm_transaction(),
				receipt: Some(create_test_evm_transaction_receipt()),
				logs: Some(create_test_evm_logs()),
				network_slug: "ethereum_mainnet".to_string(),
				matched_on: MatchConditions {
					functions: vec![],
					events: vec![],
					transactions: vec![],
				},
				matched_on_args: None,
			})),
			BlockChainType::Stellar => MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
				monitor,
				transaction: create_test_stellar_transaction(),
				ledger: create_test_stellar_block(),
				network_slug: "stellar_mainnet".to_string(),
				matched_on: MatchConditions {
					functions: vec![],
					events: vec![],
					transactions: vec![],
				},
				matched_on_args: None,
			})),
			BlockChainType::Midnight => unimplemented!(),
			BlockChainType::Solana => unimplemented!(),
		}
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
		// Create empty matches vector
		let matches: Vec<MonitorMatch> = vec![];

		// Create trigger scripts with a more realistic script path
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test-test.py".to_string(), // Using a more realistic key format
			(
				ScriptLanguage::Python,
				r#"
import sys
import json

input_data = sys.stdin.read()
data = json.loads(input_data)
print(False)
            "#
				.to_string(),
			),
		);

		// Test the filter function
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
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
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			format!("test-{}", temp_file.path().to_str().unwrap()),
			(ScriptLanguage::Python, script_content.to_string()),
		);
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::EVM,
			Some(temp_file.path().to_str().unwrap()),
		);
		let matches = vec![match_item.clone()];

		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 1);
		assert!(matches_equal(&filtered[0], &match_item));
	}

	#[tokio::test]
	async fn test_run_trigger_filters_false_condition() {
		let script_content = r#"
import sys
import json

input_data = sys.stdin.read()
data = json.loads(input_data)
print("debugging...")
def test():
	return False
result = test()
print(result)
"#;
		let temp_file = create_temp_script(script_content);
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			format!("test-{}", temp_file.path().to_str().unwrap()),
			(ScriptLanguage::Python, script_content.to_string()),
		);
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::EVM,
			Some(temp_file.path().to_str().unwrap()),
		);
		let matches = vec![match_item.clone()];

		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 1);
	}

	#[tokio::test]
	async fn test_execute_trigger_condition_returns_false() {
		let script_content = r#"print(False)  # Script returns false"#;
		let temp_file = create_temp_script(script_content);
		let trigger_condition = TriggerConditions {
			language: ScriptLanguage::Python,
			script_path: temp_file.path().to_str().unwrap().to_string(),
			timeout_ms: 1000,
			arguments: None,
		};
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::EVM,
			Some(temp_file.path().to_str().unwrap()),
		);
		let script_content = (ScriptLanguage::Python, script_content.to_string());

		let result =
			execute_trigger_condition(&trigger_condition, &match_item, &script_content).await;
		assert!(!result); // Should be false when script returns false
	}

	#[tokio::test]
	async fn test_execute_trigger_condition_script_error() {
		let script_content = r#"raise Exception("Test error")  # Raise an error"#;
		let temp_file = create_temp_script(script_content);
		let trigger_condition = TriggerConditions {
			language: ScriptLanguage::Python,
			script_path: temp_file.path().to_str().unwrap().to_string(),
			timeout_ms: 1000,
			arguments: None,
		};
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::EVM,
			Some(temp_file.path().to_str().unwrap()),
		);
		let script_content = (ScriptLanguage::Python, script_content.to_string());

		let result =
			execute_trigger_condition(&trigger_condition, &match_item, &script_content).await;
		assert!(!result); // Should be false when script errors
	}

	#[tokio::test]
	async fn test_execute_trigger_condition_invalid_script() {
		let trigger_condition = TriggerConditions {
			language: ScriptLanguage::Python,
			script_path: "non_existent_script.py".to_string(),
			timeout_ms: 1000,
			arguments: None,
		};
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::EVM,
			Some("non_existent_script.py"),
		);
		let script_content = (ScriptLanguage::Python, "invalid script content".to_string());

		let result =
			execute_trigger_condition(&trigger_condition, &match_item, &script_content).await;
		assert!(!result); // Should be false for invalid script
	}

	#[tokio::test]
	async fn test_run_trigger_filters_multiple_conditions_keep_match() {
		// Create a monitor with two trigger conditions
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("test1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("test2.py", 1000, ScriptLanguage::Python, None)
			.build();

		// Create a match with this monitor
		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|test1.py".to_string(),
			(
				ScriptLanguage::Python,
				r#"
import sys
import json

input_data = sys.stdin.read()
data = json.loads(input_data)
print(True)
"#
				.to_string(),
			),
		);
		trigger_scripts.insert(
			"monitor_test|test2.py".to_string(),
			(
				ScriptLanguage::Python,
				r#"
import sys
import json
input_data = sys.stdin.read()
data = json.loads(input_data)
print(True)
"#
				.to_string(),
			),
		);

		// Run the filter with our test data
		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;

		assert_eq!(filtered.len(), 0);
	}

	#[tokio::test]
	async fn test_run_trigger_filters_condition_two_combinations_exclude_match() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		// Test case 1: All conditions return true - match should be kept
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(True)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 0);
	}

	#[tokio::test]
	async fn test_run_trigger_filters_condition_two_combinations_keep_match() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 1);
	}

	#[tokio::test]
	async fn test_run_trigger_filters_condition_two_combinations_exclude_match_last_condition() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(True)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 0);
	}

	#[tokio::test]
	async fn test_run_trigger_filters_condition_three_combinations_exclude_match() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition3.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition3.py".to_string(),
			(ScriptLanguage::Python, "print(True)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 0);
	}

	#[tokio::test]
	async fn test_run_trigger_filters_condition_three_combinations_keep_match() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition3.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::EVM, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition3.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "ethereum_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 1);
	}

	// Add these new test cases
	#[tokio::test]
	async fn test_run_trigger_filters_stellar_empty_matches() {
		let matches: Vec<MonitorMatch> = vec![];
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|test.py".to_string(),
			(
				ScriptLanguage::Python,
				r#"
import sys
import json

input_data = sys.stdin.read()
data = json.loads(input_data)
print(False)
"#
				.to_string(),
			),
		);

		let filtered = run_trigger_filters(&matches, "stellar_mainnet", &trigger_scripts).await;
		assert!(filtered.is_empty());
	}

	#[tokio::test]
	async fn test_run_trigger_filters_stellar_true_condition() {
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
		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			format!("test|{}", temp_file.path().to_str().unwrap()),
			(ScriptLanguage::Python, script_content.to_string()),
		);
		let match_item = create_mock_monitor_match_from_path(
			BlockChainType::Stellar,
			Some(temp_file.path().to_str().unwrap()),
		);
		let matches = vec![match_item.clone()];

		let filtered = run_trigger_filters(&matches, "stellar_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 1);
		assert!(matches_equal(&filtered[0], &match_item));
	}

	#[tokio::test]
	async fn test_run_trigger_filters_stellar_multiple_conditions() {
		let monitor = MonitorBuilder::new()
			.name("monitor_test")
			.networks(vec!["stellar_mainnet".to_string()])
			.trigger_condition("condition1.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition("condition2.py", 1000, ScriptLanguage::Python, None)
			.build();

		let match_item = create_mock_monitor_match_from_monitor(BlockChainType::Stellar, monitor);

		let mut trigger_scripts = HashMap::new();
		trigger_scripts.insert(
			"monitor_test|condition1.py".to_string(),
			(ScriptLanguage::Python, "print(False)".to_string()),
		);
		trigger_scripts.insert(
			"monitor_test|condition2.py".to_string(),
			(ScriptLanguage::Python, "print(True)".to_string()),
		);

		let matches = vec![match_item.clone()];
		let filtered = run_trigger_filters(&matches, "stellar_mainnet", &trigger_scripts).await;
		assert_eq!(filtered.len(), 0); // Match should be filtered out because condition2 returns true
	}
}
