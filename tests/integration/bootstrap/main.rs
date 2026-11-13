use crate::integration::{
	filters::common::{
		setup_monitor_service, setup_network_service, setup_trigger_execution_service,
		setup_trigger_service,
	},
	mocks::{
		create_test_block, create_test_network, create_test_transaction, MockClientPool,
		MockEVMTransportClient, MockEvmClientTrait, MockMonitorRepository, MockNetworkRepository,
		MockStellarClientTrait, MockStellarTransportClient, MockTriggerExecutionService,
		MockTriggerRepository,
	},
};
use openzeppelin_monitor::{
	bootstrap::{
		create_block_handler, create_trigger_handler, get_contract_specs, initialize_services,
		process_block, ProcessBlockError,
	},
	models::{
		AddressWithSpec, BlockChainType, BlockFilterStatus, BlockType, ContractSpec,
		EVMContractSpec, EVMMonitorMatch, EVMTransactionReceipt, MatchConditions, MaxPastBlocks,
		Monitor, MonitorMatch, ProcessedBlock, ScriptLanguage, SecretString, SecretValue,
		StellarBlock, StellarContractSpec, StellarFormattedContractSpec, StellarMonitorMatch,
		TransactionType, Trigger, TriggerConditions,
	},
	services::{
		blockchain::BlockChainClient,
		blockwatcher::{
			process_new_blocks, BlockStorage, BlockTracker, BlockTrackerTrait, FileBlockStorage,
		},
		filter::{stellar_helpers::are_same_address, FilterService},
		notification::NotificationService,
		trigger::{TriggerExecutionService, TriggerExecutionServiceTrait},
	},
	utils::{
		tests::{
			evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
			trigger::TriggerBuilder,
		},
		RetryConfig,
	},
};
use std::str::FromStr;
use stellar_xdr::curr::{
	ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0, ScSpecTypeDef, ScSymbol, StringM,
};

use serde_json::json;
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};
use tokio::sync::watch;

fn create_test_monitor(
	name: &str,
	networks: Vec<&str>,
	paused: bool,
	triggers: Vec<&str>,
) -> Monitor {
	MonitorBuilder::new()
		.name(name)
		.networks(networks.into_iter().map(String::from).collect())
		.paused(paused)
		.triggers(triggers.into_iter().map(String::from).collect())
		.build()
}

fn create_test_trigger(name: &str) -> Trigger {
	TriggerBuilder::new()
		.name(name)
		.slack("https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX") //noboost
		.message("Test Title", "Test Body")
		.build()
}

fn create_test_monitor_match(chain: BlockChainType) -> MonitorMatch {
	match chain {
		BlockChainType::EVM => MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test", vec!["ethereum_mainnet"], false, vec![]),
			transaction: TransactionBuilder::new().build(),
			network_slug: "ethereum_mainnet".to_string(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		BlockChainType::Stellar => MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
			monitor: create_test_monitor("test", vec!["stellar_mainnet"], false, vec![]),
			transaction: match create_test_transaction(chain) {
				TransactionType::Stellar(tx) => *tx,
				_ => panic!("Expected Stellar transaction"),
			},
			network_slug: "stellar_mainnet".to_string(),
			ledger: StellarBlock::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		_ => panic!("Unsupported chain"),
	}
}

#[tokio::test]
async fn test_initialize_services() {
	let mut mocked_networks = HashMap::new();
	mocked_networks.insert(
		"ethereum_mainnet".to_string(),
		create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM),
	);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_monitor(
			"test",
			vec!["ethereum_mainnet"],
			false,
			vec!["evm_large_transfer_usdc_slack"],
		),
	);

	let mock_network_service = setup_network_service(mocked_networks);
	let mock_trigger_service = setup_trigger_service(mocked_triggers);
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	// Initialize services
	let (
		filter_service,
		trigger_execution_service,
		active_monitors,
		networks,
		monitor_service,
		network_service,
		trigger_service,
	) = initialize_services::<
		MockMonitorRepository<MockNetworkRepository, MockTriggerRepository>,
		MockNetworkRepository,
		MockTriggerRepository,
	>(
		Some(mock_monitor_service),
		Some(mock_network_service),
		Some(mock_trigger_service),
	)
	.await
	.expect("Failed to initialize services");

	assert!(
		Arc::strong_count(&filter_service) == 1,
		"FilterService should be wrapped in Arc"
	);
	assert!(
		Arc::strong_count(&trigger_execution_service) == 1,
		"TriggerExecutionService should be wrapped in Arc"
	);

	assert!(active_monitors.iter().any(|m| {
		m.name == "test"
			&& m.networks.contains(&"ethereum_mainnet".to_string())
			&& m.triggers
				.contains(&"evm_large_transfer_usdc_slack".to_string())
	}));
	assert!(networks.contains_key("ethereum_mainnet"));

	assert!(Arc::strong_count(&monitor_service) >= 1);
	assert!(Arc::strong_count(&network_service) >= 1);
	assert!(Arc::strong_count(&trigger_service) >= 1);
}

#[tokio::test]
async fn test_create_block_handler_evm() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];
	let block = create_test_block(BlockChainType::EVM, 100);
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(|_, _, _| Ok(vec![]));

	// Create a mock client pool that returns a successful client
	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let client_pool = Arc::new(mock_pool);

	let network_monitors = vec![(network.clone(), monitors.clone())];
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		filter_service,
		monitors,
		client_pool,
		contract_specs,
	);

	let result = block_handler(block, network).await;
	assert_eq!(result.block_number, 100);
	assert_eq!(result.network_slug, "ethereum_mainnet");
	// The mock client should return no matches
	assert!(result.processing_results.is_empty());
	assert_eq!(result.filter_status, BlockFilterStatus::Completed);
}

#[tokio::test]
async fn test_create_block_handler_evm_filter_error() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];
	let block = create_test_block(BlockChainType::EVM, 100);
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	// eth_getLogs keeps failing, as in #517 where the RPC answered HTTP 429
	let mut mock_client = MockEvmClientTrait::new();
	mock_client
		.expect_get_logs_for_blocks()
		.return_once(|_, _, _| Err(anyhow::anyhow!("HTTP error: status 429 Too Many Requests")));

	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));
	let client_pool = Arc::new(mock_pool);

	let network_monitors = vec![(network.clone(), monitors.clone())];
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		filter_service,
		monitors,
		client_pool,
		contract_specs,
	);

	let result = block_handler(block, network).await;
	assert_eq!(result.block_number, 100);
	assert!(result.processing_results.is_empty());
	assert_eq!(
		result.filter_status,
		BlockFilterStatus::Failed,
		"a failed filter must not look like a block with no matches"
	);
}

/// Serves EVM blocks without transactions, up to a fixed head
#[derive(Clone)]
struct FixedHeadClient {
	latest_block: u64,
}

#[async_trait::async_trait]
impl BlockChainClient for FixedHeadClient {
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		Ok(self.latest_block)
	}

	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		Ok((start_block..=end_block.unwrap_or(start_block))
			.map(|block_number| create_test_block(BlockChainType::EVM, block_number))
			.collect())
	}
}

/// The #517 reproduction through the real block handler and block watcher: `eth_getLogs`
/// for block 84 fails once with HTTP 429 on a network without recovery or stored blocks.
#[tokio::test]
async fn test_failed_get_logs_is_retried_on_next_run() {
	let (shutdown_tx, _) = watch::channel(false);
	let mut network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	network.confirmation_blocks = 0;
	network.store_blocks = Some(false);
	network.recovery_config = None;
	network.max_past_blocks = Some(MaxPastBlocks::Limited(100));
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];

	let rate_limited = Arc::new(AtomicBool::new(false));
	let logs_requested = Arc::new(std::sync::Mutex::new(Vec::new()));
	let mut mock_pool = MockClientPool::new();
	{
		let rate_limited = rate_limited.clone();
		let logs_requested = logs_requested.clone();
		mock_pool.expect_get_evm_client().returning(move |_| {
			let rate_limited = rate_limited.clone();
			let logs_requested = logs_requested.clone();
			let mut client = MockEvmClientTrait::new();
			client
				.expect_get_logs_for_blocks()
				.returning(move |from_block, _, _| {
					logs_requested.lock().unwrap().push(from_block);
					if from_block == 84 && !rate_limited.swap(true, Ordering::SeqCst) {
						return Err(anyhow::anyhow!("HTTP error: status 429 Too Many Requests"));
					}
					Ok(vec![])
				});
			Ok(Arc::new(client))
		});
	}

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		Arc::new(FilterService::new()),
		monitors,
		Arc::new(mock_pool),
		vec![],
	);
	let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
	let trigger_handler = {
		let triggered = triggered.clone();
		Arc::new(move |block: &ProcessedBlock| {
			triggered.lock().unwrap().push(block.block_number);
			tokio::spawn(async {})
		})
	};

	let temp_dir = tempfile::tempdir().unwrap();
	let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
	storage
		.save_last_processed_block("ethereum_mainnet", 80)
		.await
		.unwrap();
	let block_tracker = Arc::new(BlockTracker::new(100));
	let rpc_client = FixedHeadClient { latest_block: 88 };

	// First run: blocks 81-88, and the filter of block 84 fails
	process_new_blocks(
		&network,
		&rpc_client,
		storage.clone(),
		block_handler.clone(),
		trigger_handler.clone(),
		block_tracker.clone(),
	)
	.await
	.unwrap();

	assert!(
		rate_limited.load(Ordering::SeqCst),
		"block 84 should have been rate-limited"
	);
	assert_eq!(
		storage
			.get_last_processed_block("ethereum_mainnet")
			.await
			.unwrap(),
		Some(83),
		"the checkpoint must not move past block 84"
	);
	assert_eq!(*triggered.lock().unwrap(), vec![81, 82, 83]);

	// Second run: block 84 is filtered again, now successfully
	logs_requested.lock().unwrap().clear();
	process_new_blocks(
		&network,
		&rpc_client,
		storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await
	.unwrap();

	let mut requested = logs_requested.lock().unwrap().clone();
	requested.sort_unstable();
	assert_eq!(requested, (84..=88).collect::<Vec<_>>());
	assert_eq!(
		storage
			.get_last_processed_block("ethereum_mainnet")
			.await
			.unwrap(),
		Some(88)
	);
	assert_eq!(*triggered.lock().unwrap(), (81..=88).collect::<Vec<_>>());
}

#[tokio::test]
async fn test_create_trigger_handler() {
	// Set up expectation for the constructor first
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_trigger_service, _notification_service| {
			let mut mock = MockTriggerExecutionService::default();
			mock.expect_execute()
				.times(1)
				.return_once(|_, _, _, _| Ok(()));
			mock
		});

	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json")
			.await;

	let (shutdown_tx, _) = watch::channel(false);
	let trigger_handler = create_trigger_handler(
		shutdown_tx,
		Arc::new(trigger_execution_service),
		HashMap::new(),
	);

	assert!(Arc::strong_count(&trigger_handler) == 1);

	let processed_block = ProcessedBlock {
		block_number: 100,
		network_slug: "ethereum_mainnet".to_string(),
		processing_results: vec![create_test_monitor_match(BlockChainType::EVM)],
		filter_status: BlockFilterStatus::Completed,
	};

	let handle = trigger_handler(&processed_block);
	handle
		.await
		.expect("Trigger handler task should complete successfully");
}

#[tokio::test]
async fn test_create_trigger_handler_empty_matches() {
	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json")
			.await;

	let (shutdown_tx, _) = watch::channel(false);
	let trigger_handler = create_trigger_handler(
		shutdown_tx,
		Arc::new(trigger_execution_service),
		HashMap::new(),
	);

	assert!(Arc::strong_count(&trigger_handler) == 1);

	let processed_block = ProcessedBlock {
		block_number: 100,
		network_slug: "ethereum_mainnet".to_string(),
		processing_results: vec![],
		filter_status: BlockFilterStatus::Completed,
	};

	let handle = trigger_handler(&processed_block);
	handle
		.await
		.expect("Trigger handler task should complete successfully");
}

#[tokio::test]
async fn test_create_block_handler_stellar() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor(
		"test",
		vec!["stellar_mainnet"],
		false,
		vec![],
	)];

	let block = create_test_block(BlockChainType::Stellar, 100);
	let network = create_test_network("Stellar", "stellar_mainnet", BlockChainType::Stellar);

	let mut contract_spec_pool = MockClientPool::new();
	let mut handle_block_client_pool = MockClientPool::new();

	contract_spec_pool
		.expect_get_stellar_client()
		.returning(move |_| {
			let mut mock_client = MockStellarClientTrait::new();
			mock_client
				.expect_get_contract_spec()
				.times(1)
				.returning(move |_| {
					let contract_spec = ContractSpec::Stellar(StellarContractSpec::from(json!(
						{
							"function_v0": {
								"doc": "",
								"name": "transfer",
								"inputs": [
									{
										"doc": "",
										"name": "from",
										"type_": "address"
									},
									{
										"doc": "",
										"name": "to",
										"type_": "address"
									},
									{
										"doc": "",
										"name": "amount",
										"type_": "i128"
									}
								],
								"outputs": []
							}
						}
					)));
					Ok(contract_spec.clone())
				});
			Ok(Arc::new(mock_client))
		});

	handle_block_client_pool
		.expect_get_stellar_client()
		.returning(move |_| {
			let mut mock_client = MockStellarClientTrait::new();
			mock_client
				.expect_get_transactions()
				.times(1)
				.returning(move |_, _| Ok(vec![]));
			Ok(Arc::new(mock_client))
		});

	let network_monitors = vec![(network.clone(), monitors.clone())];
	let contract_specs = get_contract_specs(&Arc::new(contract_spec_pool), &network_monitors).await;

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		filter_service,
		monitors,
		Arc::new(handle_block_client_pool),
		contract_specs,
	);
	let result = block_handler(block, network).await;

	assert_eq!(result.block_number, 100);
	assert_eq!(result.network_slug, "stellar_mainnet");

	// The mock client should return no matches
	assert!(result.processing_results.is_empty());
}

#[tokio::test]
async fn test_create_block_handler_evm_client_error() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];

	let block = create_test_block(BlockChainType::EVM, 100);
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	// Create a mock client pool that returns an error
	let mut mock_pool = MockClientPool::new();

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Err(anyhow::anyhow!("Failed to get EVM client")));
	let client_pool = Arc::new(mock_pool);

	let network_monitors = vec![(network.clone(), monitors.clone())];
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		filter_service,
		monitors,
		client_pool,
		contract_specs,
	);
	let result = block_handler(block, network).await;

	assert_eq!(result.block_number, 100);
	assert_eq!(result.network_slug, "ethereum_mainnet");
	assert!(result.processing_results.is_empty());
	assert_eq!(result.filter_status, BlockFilterStatus::Failed);
}

#[tokio::test]
async fn test_create_block_handler_stellar_client_error() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor(
		"test",
		vec!["stellar_mainnet"],
		false,
		vec![],
	)];

	let block = create_test_block(BlockChainType::Stellar, 100);
	let network = create_test_network("Stellar", "stellar_mainnet", BlockChainType::Stellar);

	// Create a mock client pool that returns an error
	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_stellar_client()
		.returning(move |_| Err(anyhow::anyhow!("Failed to get Stellar client")));

	let client_pool = Arc::new(mock_pool);

	let network_monitors = vec![(network.clone(), monitors.clone())];
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	let block_handler = create_block_handler::<MockClientPool>(
		shutdown_tx,
		filter_service,
		monitors,
		client_pool,
		contract_specs,
	);

	let result = block_handler(block, network).await;
	assert_eq!(result.block_number, 100);
	assert_eq!(result.network_slug, "stellar_mainnet");
	assert!(result.processing_results.is_empty());
	assert_eq!(result.filter_status, BlockFilterStatus::Failed);
}

#[tokio::test]
async fn test_create_trigger_handler_with_conditions() {
	// Set up expectation for the constructor first
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_trigger_service, _notification_service| {
			let mut mock = MockTriggerExecutionService::default();
			mock.expect_execute()
				.times(1)
				.return_once(|_, _, _, _| Ok(()));
			mock
		});

	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json")
			.await;

	// Create a HashMap with trigger conditions
	let mut trigger_scripts = HashMap::new();
	trigger_scripts.insert(
		"test_trigger|test_script.py".to_string(),
		(
			ScriptLanguage::Python,
			r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print(True)  # Always return true for test
"#
			.to_string(),
		),
	);

	let (shutdown_tx, _) = watch::channel(false);
	let trigger_handler = create_trigger_handler(
		shutdown_tx,
		Arc::new(trigger_execution_service),
		trigger_scripts,
	);

	assert!(Arc::strong_count(&trigger_handler) == 1);

	// Create a monitor with trigger conditions
	let mut monitor = create_test_monitor("test_trigger", vec!["ethereum_mainnet"], false, vec![]);
	monitor.trigger_conditions = vec![TriggerConditions {
		script_path: "test_script.py".to_string(),
		language: ScriptLanguage::Python,
		timeout_ms: 1000,
		arguments: None,
		runtime_arguments: vec![],
		interpreter: None,
	}];

	let processed_block = ProcessedBlock {
		block_number: 100,
		network_slug: "ethereum_mainnet".to_string(),
		processing_results: vec![MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor,
			transaction: TransactionBuilder::new().build(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			network_slug: "ethereum_mainnet".to_string(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}))],
		filter_status: BlockFilterStatus::Completed,
	};

	let handle = trigger_handler(&processed_block);
	handle
		.await
		.expect("Trigger handler task should complete successfully");
}

#[cfg(unix)]
#[tokio::test]
async fn test_create_trigger_handler_with_condition_interpreter_override() {
	// Count the matches that pass the trigger conditions and reach trigger execution
	let executions = Arc::new(std::sync::atomic::AtomicUsize::new(0));
	let executions_clone = executions.clone();

	// Built directly: `create_trigger_handler` takes a ready-made service, so this test needs
	// neither a trigger repository nor the mocked constructor (a global mockall context).
	let mut trigger_execution_service =
		MockTriggerExecutionService::<MockTriggerRepository>::default();
	trigger_execution_service
		.expect_execute()
		.returning(move |_, _, _, _| {
			executions_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			Ok(())
		});

	// Valid shell, but a syntax error in Python: only `sh` can make this condition return true
	let mut trigger_scripts = HashMap::new();
	trigger_scripts.insert(
		"test_trigger|test_script.py".to_string(),
		(
			ScriptLanguage::Python,
			"cat >/dev/null; echo true".to_string(),
		),
	);

	let (shutdown_tx, _) = watch::channel(false);
	let trigger_handler = create_trigger_handler(
		shutdown_tx,
		Arc::new(trigger_execution_service),
		trigger_scripts,
	);

	let processed_block_with_interpreter = |interpreter: Option<&str>| {
		let mut monitor =
			create_test_monitor("test_trigger", vec!["ethereum_mainnet"], false, vec![]);
		monitor.trigger_conditions = vec![TriggerConditions {
			script_path: "test_script.py".to_string(),
			language: ScriptLanguage::Python,
			timeout_ms: 5000,
			arguments: None,
			runtime_arguments: vec![],
			interpreter: interpreter.map(str::to_string),
		}];

		ProcessedBlock {
			block_number: 100,
			network_slug: "ethereum_mainnet".to_string(),
			processing_results: vec![MonitorMatch::EVM(Box::new(EVMMonitorMatch {
				monitor,
				transaction: TransactionBuilder::new().build(),
				receipt: Some(EVMTransactionReceipt::default()),
				logs: Some(vec![]),
				network_slug: "ethereum_mainnet".to_string(),
				matched_on: MatchConditions::default(),
				matched_on_args: None,
			}))],
			filter_status: BlockFilterStatus::Completed,
		}
	};

	// Premise: with the default interpreter the condition script fails, so the match is not
	// filtered out and goes on to trigger execution
	trigger_handler(&processed_block_with_interpreter(None))
		.await
		.expect("Trigger handler task should complete successfully");
	assert_eq!(
		executions.load(std::sync::atomic::Ordering::SeqCst),
		1,
		"python3 cannot run the shell-only condition, so the match should be executed"
	);

	// With the configured interpreter the condition returns true and filters the match out
	trigger_handler(&processed_block_with_interpreter(Some("sh")))
		.await
		.expect("Trigger handler task should complete successfully");
	assert_eq!(
		executions.load(std::sync::atomic::Ordering::SeqCst),
		1,
		"the `sh` condition should filter the match out, so nothing more should be executed"
	);
}

#[tokio::test]
async fn test_process_block() {
	let mut mock_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let block = create_test_block(BlockChainType::EVM, 100);
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];
	let filter_service = FilterService::new();

	// Keep the shutdown_tx variable to avoid unexpected shutdown signal changes
	#[allow(unused_variables)]
	let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

	// Configure mock behavior
	mock_client
		.expect_get_latest_block_number()
		.return_once(|| Ok(100));

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(|_, _, _| Ok(vec![]));

	let result = process_block(
		&mock_client,
		&network,
		&block,
		&monitors,
		None,
		&filter_service,
		&mut shutdown_rx,
	)
	.await;

	assert!(
		!*shutdown_rx.borrow(),
		"Shutdown signal was unexpectedly triggered"
	);
	assert!(
		matches!(result, Ok(ref matches) if matches.is_empty()),
		"Expected an empty successful result when no shutdown signal, got {result:?}"
	);
}

#[tokio::test]
async fn test_process_block_returns_filter_error() {
	let mut mock_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let block = create_test_block(BlockChainType::EVM, 100);
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];
	let filter_service = FilterService::new();

	#[allow(unused_variables)]
	let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(|_, _, _| Err(anyhow::anyhow!("HTTP error: status 429 Too Many Requests")));

	let result = process_block(
		&mock_client,
		&network,
		&block,
		&monitors,
		None,
		&filter_service,
		&mut shutdown_rx,
	)
	.await;

	assert!(
		matches!(result, Err(ProcessBlockError::Filter(_))),
		"Expected the filter error, got {result:?}"
	);
}

/// Shutdown is checked before the filter, so a pending shutdown cancels the block without
/// calling the client (the mock has no expectations and would panic if it were called).
#[tokio::test]
async fn test_process_block_with_shutdown() {
	let mock_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let block = create_test_block(BlockChainType::EVM, 100);
	let monitors = vec![create_test_monitor(
		"test",
		vec!["ethereum_mainnet"],
		false,
		vec![],
	)];
	let filter_service = FilterService::new();
	let (shutdown_tx, shutdown_rx) = watch::channel(false);

	// Send shutdown signal
	shutdown_tx
		.send(true)
		.expect("Failed to send shutdown signal");

	let mut shutdown_rx = shutdown_rx.clone();

	let result = process_block(
		&mock_client,
		&network,
		&block,
		&monitors,
		None,
		&filter_service,
		&mut shutdown_rx,
	)
	.await;

	assert!(
		matches!(result, Err(ProcessBlockError::Cancelled)),
		"Expected cancellation when shutdown signal is received, got {result:?}"
	);
}

#[tokio::test]
async fn test_load_scripts() {
	// Create a temporary test script file
	let temp_dir = tempfile::tempdir().unwrap();
	let script_path = temp_dir.path().join("test_script.py");
	tokio::fs::write(&script_path, "print('test script content')")
		.await
		.unwrap();

	// Create test monitors with real trigger conditions

	let monitor = MonitorBuilder::new()
		.name("test_monitor")
		.networks(vec!["evm_mainnet".to_string()])
		.trigger_condition(
			script_path.to_str().unwrap(),
			1000,
			ScriptLanguage::Python,
			None,
		)
		.build();

	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	// Test loading scripts
	let scripts = trigger_execution_service
		.load_scripts(&[monitor])
		.await
		.unwrap();

	// Verify results
	assert_eq!(scripts.len(), 1);

	let script_key = format!("test_monitor|{}", script_path.to_str().unwrap());
	assert!(scripts.contains_key(&script_key));

	let (lang, content) = &scripts[&script_key];
	assert_eq!(*lang, ScriptLanguage::Python);
	assert_eq!(content.trim(), "print('test script content')");

	// Cleanup is handled automatically when temp_dir is dropped
}

// Also add a test for the error case
#[tokio::test]
async fn test_load_scripts_error() {
	// Create test monitors with non-existent script path
	let monitors = vec![MonitorBuilder::new()
		.name("test_monitor")
		.trigger_condition("non_existent_script.py", 1000, ScriptLanguage::Python, None)
		.build()];

	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	// Test loading scripts
	let result = trigger_execution_service.load_scripts(&monitors).await;
	assert!(result.is_err());
	let error = result.unwrap_err();
	assert!(error.to_string().contains("Failed to read script file"));
}

#[tokio::test]
async fn test_load_scripts_empty_conditions() {
	// Create test monitors with empty trigger conditions
	let monitors = vec![MonitorBuilder::new().name("test_monitor").build()];

	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	// Test loading scripts
	let scripts = trigger_execution_service
		.load_scripts(&monitors)
		.await
		.unwrap();

	// Verify results
	assert!(
		scripts.is_empty(),
		"Scripts map should be empty when there are no trigger conditions"
	);
}

#[tokio::test]
async fn test_load_scripts_for_custom_triggers_notifications() {
	let temp_dir = tempfile::tempdir().unwrap();
	let script_path = temp_dir.path().join("test_script.py");
	tokio::fs::write(&script_path, "print('test script content')")
		.await
		.unwrap();

	let script_trigger_path = temp_dir.path().join("custom_trigger_script.py");
	tokio::fs::write(&script_trigger_path, "print('test script trigger content')")
		.await
		.unwrap();

	let monitors = vec![MonitorBuilder::new()
		.name("test_monitor")
		.trigger_condition(
			script_path.to_str().unwrap(),
			1000,
			ScriptLanguage::Python,
			None,
		)
		.triggers(vec!["custom_trigger".to_string()])
		.build()];

	let mut mocked_triggers = HashMap::new();

	let custom_trigger = TriggerBuilder::new()
		.name("custom_trigger")
		.script(
			script_trigger_path.to_str().unwrap(),
			ScriptLanguage::Python,
		)
		.build();

	mocked_triggers.insert("custom_trigger".to_string(), custom_trigger.clone());

	// Set up mock repository
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	// Test loading scripts
	let scripts = trigger_execution_service
		.load_scripts(&monitors)
		.await
		.unwrap();

	// Verify results
	assert_eq!(scripts.len(), 2);

	let script_key = format!("test_monitor|{}", script_path.to_str().unwrap());
	assert!(scripts.contains_key(&script_key));

	let (lang, content) = &scripts[&script_key];
	assert_eq!(*lang, ScriptLanguage::Python);
	assert_eq!(content.trim(), "print('test script content')");

	let script_key_trigger = format!("test_monitor|{}", script_trigger_path.to_str().unwrap());
	assert!(scripts.contains_key(&script_key_trigger));

	let (lang, content) = &scripts[&script_key_trigger];
	assert_eq!(*lang, ScriptLanguage::Python);
	assert_eq!(content.trim(), "print('test script trigger content')");
}

#[tokio::test]
async fn test_load_scripts_for_custom_triggers_notifications_error() {
	let temp_dir = tempfile::tempdir().unwrap();
	let script_path = temp_dir.path().join("test_script.py");
	tokio::fs::write(&script_path, "print('test script content')")
		.await
		.unwrap();

	let script_trigger_path = temp_dir.path().join("custom_trigger_script.py");
	tokio::fs::write(&script_trigger_path, "print('test script trigger content')")
		.await
		.unwrap();

	let monitors = vec![MonitorBuilder::new()
		.name("test_monitor")
		.trigger_condition(
			script_path.to_str().unwrap(),
			1000,
			ScriptLanguage::Python,
			None,
		)
		.triggers(vec!["custom_trigger".to_string()])
		.build()];

	let mut mocked_triggers = HashMap::new();
	let custom_trigger = TriggerBuilder::new()
		.name("custom_trigger")
		.script("non_existent_script.py", ScriptLanguage::Python)
		.build();
	mocked_triggers.insert("custom_trigger".to_string(), custom_trigger.clone());

	// Set up mock repository
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	// Test loading scripts
	let result = trigger_execution_service.load_scripts(&monitors).await;
	assert!(result.is_err());

	match result {
		Err(e) => {
			assert!(e.to_string().contains("Failed to read script file"));
		}
		_ => panic!("Expected error"),
	}
}

#[tokio::test]
async fn test_load_scripts_for_custom_triggers_notifications_failed() {
	let temp_dir = tempfile::tempdir().unwrap();
	let script_path = temp_dir.path().join("test_script.py");
	tokio::fs::write(&script_path, "print('test script content')")
		.await
		.unwrap();

	let monitors = vec![MonitorBuilder::new()
		.name("test_monitor")
		.trigger_condition(
			script_path.to_str().unwrap(),
			1000,
			ScriptLanguage::Python,
			None,
		)
		.triggers(vec!["custom_trigger_not_found".to_string()])
		.build()];

	let mut mocked_triggers = HashMap::new();
	let custom_trigger = TriggerBuilder::new()
		.name("custom_trigger_not_found")
		.script(script_path.to_str().unwrap(), ScriptLanguage::Python)
		.build();
	mocked_triggers.insert("custom_trigger".to_string(), custom_trigger.clone());

	// Set up mock repository
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	// Test loading scripts
	let result = trigger_execution_service.load_scripts(&monitors).await;

	assert!(result.is_err());
	match result {
		Err(e) => {
			assert!(e.to_string().contains("Failed to get trigger"));
		}
		_ => panic!("Expected error"),
	}
}

#[tokio::test]
async fn test_trigger_execution_service_execute_multiple_triggers_failed_retryable_error() {
	// Slack execution success - Webhook execution failure - Script execution failure
	// We should see two errors regarding the webhook and one regarding the script
	let mut server = mockito::Server::new_async().await;
	let default_retries_count = RetryConfig::default().max_retries as usize;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(500)
		.expect(1 + default_retries_count)
		.create_async()
		.await;
	let mut mocked_triggers = HashMap::new();

	mocked_triggers.insert(
		"example_trigger_slack".to_string(),
		TriggerBuilder::new()
			.name("test_trigger")
			.slack(&server.url())
			.message("Test Alert", "Test message with value ${value}")
			.build(),
	);
	mocked_triggers.insert(
		"example_trigger_webhook".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_webhook")
			.webhook(
				"https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX", //noboost
			)
			.webhook_secret(SecretValue::Plain(SecretString::new("secret".to_string())))
			.webhook_method("POST")
			.message("Test Title", "Test Body")
			.build(),
	);
	let script_path = "tests/integration/fixtures/evm/triggers/scripts/custom_notification.py";
	mocked_triggers.insert(
		"example_trigger_script".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_script")
			.script(script_path, ScriptLanguage::Python)
			.build(),
	);
	let mock_trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	let triggers = vec![
		"example_trigger_slack".to_string(),
		"example_trigger_webhook".to_string(),
		"example_trigger_script".to_string(),
	];
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_monitor_match(BlockChainType::EVM);

	let result = trigger_execution_service
		.execute(&triggers, variables, &monitor_match, &HashMap::new())
		.await;
	assert!(result.is_err());

	match result {
		Err(e) => {
			assert!(e
				.to_string()
				.contains("Some trigger(s) failed (3 failure(s))"));
		}
		_ => panic!("Expected error"),
	}
	mock.assert();
}

#[tokio::test]
async fn test_trigger_execution_service_execute_multiple_triggers_failed_non_retryable_error() {
	// Slack execution success - Webhook execution failure - Script execution failure
	// We should see two errors regarding the webhook and one regarding the script
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(400) // Non-retryable error
		.expect(1) // 1 initial call, no retries
		.create_async()
		.await;
	let mut mocked_triggers = HashMap::new();

	mocked_triggers.insert(
		"example_trigger_slack".to_string(),
		TriggerBuilder::new()
			.name("test_trigger")
			.slack(&server.url())
			.message("Test Alert", "Test message with value ${value}")
			.build(),
	);
	mocked_triggers.insert(
		"example_trigger_webhook".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_webhook")
			.webhook(
				"https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX", //noboost
			)
			.webhook_secret(SecretValue::Plain(SecretString::new("secret".to_string())))
			.webhook_method("POST")
			.message("Test Title", "Test Body")
			.build(),
	);
	let script_path = "tests/integration/fixtures/evm/triggers/scripts/custom_notification.py";
	mocked_triggers.insert(
		"example_trigger_script".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_script")
			.script(script_path, ScriptLanguage::Python)
			.build(),
	);
	let mock_trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	let triggers = vec![
		"example_trigger_slack".to_string(),
		"example_trigger_webhook".to_string(),
		"example_trigger_script".to_string(),
	];
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_monitor_match(BlockChainType::EVM);

	let result = trigger_execution_service
		.execute(&triggers, variables, &monitor_match, &HashMap::new())
		.await;
	assert!(result.is_err());

	match result {
		Err(e) => {
			assert!(e
				.to_string()
				.contains("Some trigger(s) failed (3 failure(s))"));
		}
		_ => panic!("Expected error"),
	}
	mock.assert();
}

#[tokio::test]
async fn test_trigger_execution_service_execute_multiple_triggers_success() {
	// Set up mock servers for both Slack and Webhook endpoints
	let mut slack_server = mockito::Server::new_async().await;
	let mut webhook_server = mockito::Server::new_async().await;

	// Set up Slack mock
	let slack_mock = slack_server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(200)
		.create_async()
		.await;

	// Set up Webhook mock
	let webhook_mock = webhook_server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Any)
		.with_status(200)
		.create_async()
		.await;

	let mut mocked_triggers = HashMap::new();

	// Add Slack trigger
	mocked_triggers.insert(
		"example_trigger_slack".to_string(),
		TriggerBuilder::new()
			.name("test_trigger")
			.slack(&slack_server.url())
			.message("Test Alert", "Test message with value ${value}")
			.build(),
	);

	// Add Webhook trigger
	mocked_triggers.insert(
		"example_trigger_webhook".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_webhook")
			.webhook(&webhook_server.url())
			.webhook_headers(HashMap::new())
			.webhook_secret(SecretValue::Plain(SecretString::new("secret".to_string())))
			.webhook_method("POST")
			.message("Test Title", "Test Body")
			.build(),
	);

	let mock_trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	let triggers = vec![
		"example_trigger_slack".to_string(),
		"example_trigger_webhook".to_string(),
	];
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_monitor_match(BlockChainType::EVM);

	let result = trigger_execution_service
		.execute(&triggers, variables, &monitor_match, &HashMap::new())
		.await;
	// Assert all triggers executed successfully
	assert!(result.is_ok());

	// Verify that both mock servers received their expected calls
	slack_mock.assert();
	webhook_mock.assert();
}

#[tokio::test]
async fn test_trigger_execution_service_execute_multiple_triggers_partial_success() {
	// Set up mock servers for both Slack and Webhook endpoints
	let mut slack_server = mockito::Server::new_async().await;
	let mut webhook_server = mockito::Server::new_async().await;
	let default_retries_count = RetryConfig::default().max_retries as usize;

	// Set up Slack mock
	let slack_mock = slack_server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(500)
		.expect(1 + default_retries_count)
		.create_async()
		.await;

	// Set up Webhook mock
	let webhook_mock = webhook_server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Any)
		.with_status(200)
		.create_async()
		.await;

	let mut mocked_triggers = HashMap::new();

	// Add Slack trigger
	mocked_triggers.insert(
		"example_trigger_slack".to_string(),
		TriggerBuilder::new()
			.name("test_trigger")
			.slack(&slack_server.url())
			.message("Test Alert", "Test message with value ${value}")
			.build(),
	);

	// Add Webhook trigger
	mocked_triggers.insert(
		"example_trigger_webhook".to_string(),
		TriggerBuilder::new()
			.name("example_trigger_webhook")
			.webhook(&webhook_server.url())
			.webhook_headers(HashMap::new())
			.webhook_secret(SecretValue::Plain(SecretString::new("secret".to_string())))
			.webhook_method("POST")
			.message("Test Title", "Test Body")
			.build(),
	);

	let mock_trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(mock_trigger_service, notification_service);

	let triggers = vec![
		"example_trigger_slack".to_string(),
		"example_trigger_webhook".to_string(),
	];
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_monitor_match(BlockChainType::EVM);

	let result = trigger_execution_service
		.execute(&triggers, variables, &monitor_match, &HashMap::new())
		.await;

	// Assert all triggers executed successfully
	assert!(result.is_err());

	match result {
		Err(e) => {
			assert!(e
				.to_string()
				.contains("Some trigger(s) failed (1 failure(s))"));
		}
		_ => panic!("Expected error"),
	}
	// Verify that both mock servers received their expected calls
	slack_mock.assert();
	webhook_mock.assert();
}

#[tokio::test]
async fn test_get_contract_specs() {
	// Test EVM contract specs
	let mock_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	// Create a mock client pool that returns a successful client
	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));
	let client_pool = Arc::new(mock_pool);

	// Create a monitor with EVM contract specs
	let mut monitor = create_test_monitor("test", vec!["ethereum_mainnet"], false, vec![]);
	monitor.addresses.push(AddressWithSpec {
		address: "0x1234567890123456789012345678901234567890".to_string(),
		contract_spec: Some(ContractSpec::EVM(EVMContractSpec::from(
			serde_json::json!([{
				"type": "function",
				"name": "transfer",
				"inputs": [
					{
						"name": "to",
						"type": "address",
						"internalType": "address"
					},
					{
						"name": "amount",
						"type": "uint256",
						"internalType": "uint256"
					}
				],
				"outputs": [
					{
						"name": "",
						"type": "bool",
						"internalType": "bool"
					}
				],
				"stateMutability": "nonpayable"
			}]),
		))),
	});

	monitor.addresses.push(AddressWithSpec {
		address: "0x1234567890123456789012345678901234567890".to_string(),
		contract_spec: None,
	});

	let monitors = vec![monitor];

	// Create a vector of networks with their associated monitors
	let network_monitors = vec![(network, monitors)];

	// Fetch all contract specs for all active monitors
	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	// Verify EVM specs and second spec is not added
	assert_eq!(contract_specs.len(), 1);

	let (addr, spec) = &contract_specs[0];
	assert_eq!(addr, "0x1234567890123456789012345678901234567890");
	match spec {
		ContractSpec::EVM(evm_spec) => {
			let functions: Vec<_> = evm_spec.functions().collect();
			assert_eq!(functions.len(), 1);
			assert_eq!(functions[0].name, "transfer");
			assert_eq!(functions[0].inputs.len(), 2);
			assert_eq!(functions[0].inputs[0].name, "to");
			assert_eq!(functions[0].inputs[0].ty, "address");
			assert_eq!(functions[0].inputs[1].name, "amount");
			assert_eq!(functions[0].inputs[1].ty, "uint256");
		}
		_ => panic!("Expected EVM contract spec"),
	}

	// Test Stellar contract specs
	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let network = create_test_network("Stellar", "stellar_mainnet", BlockChainType::Stellar);

	// Mock the get_contract_spec response for the address without a spec
	mock_client
		.expect_get_contract_spec()
		.withf(|addr| addr == "GZYXWVUTSRQPONMLKJIHGFEDCBA0987654321")
		.times(1)
		.returning(|_| {
			Ok(ContractSpec::Stellar(StellarContractSpec::from(vec![
				ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
					doc: StringM::<1024>::from_str("").unwrap(),
					name: ScSymbol(StringM::<32>::from_str("balance").unwrap()),
					inputs: vec![].try_into().unwrap(),
					outputs: vec![ScSpecTypeDef::I128].try_into().unwrap(),
				}),
			])))
		});

	// Create a mock client pool that returns a successful client
	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));
	let client_pool = Arc::new(mock_pool);

	// Create a monitor with Stellar contract specs
	let mut stellar_monitor =
		create_test_monitor("test_stellar", vec!["stellar_mainnet"], false, vec![]);

	// Remove default ZERO address
	stellar_monitor.addresses = vec![];

	stellar_monitor.addresses.push(AddressWithSpec {
		address: "GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890".to_string(),
		contract_spec: Some(ContractSpec::Stellar(StellarContractSpec::from(vec![
			ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
				doc: StringM::<1024>::from_str("").unwrap(),
				name: ScSymbol(StringM::<32>::from_str("transfer").unwrap()),
				inputs: vec![
					ScSpecFunctionInputV0 {
						doc: StringM::<1024>::from_str("").unwrap(),
						name: StringM::<30>::from_str("to").unwrap(),
						type_: ScSpecTypeDef::String,
					},
					ScSpecFunctionInputV0 {
						doc: StringM::<1024>::from_str("").unwrap(),
						name: StringM::<30>::from_str("amount").unwrap(),
						type_: ScSpecTypeDef::I128,
					},
				]
				.try_into()
				.unwrap(),
				outputs: vec![ScSpecTypeDef::Bool].try_into().unwrap(),
			}),
		]) as StellarContractSpec)),
	});

	// Add an address without a contract spec to test fetching from chain
	stellar_monitor.addresses.push(AddressWithSpec {
		address: "GZYXWVUTSRQPONMLKJIHGFEDCBA0987654321".to_string(),
		contract_spec: None,
	});

	let network_monitors = vec![(network, vec![stellar_monitor])];

	let contract_specs = get_contract_specs(&client_pool, &network_monitors).await;

	// Verify Stellar specs
	assert_eq!(contract_specs.len(), 2);
	let (addr, spec) = &contract_specs[0];
	assert!(are_same_address(
		addr,
		"GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890"
	));
	match spec {
		ContractSpec::Stellar(stellar_spec) => {
			let formatted_spec = StellarFormattedContractSpec::from(stellar_spec.clone());
			assert_eq!(formatted_spec.functions.len(), 1);
			let function = &formatted_spec.functions[0];
			assert_eq!(function.name, "transfer");
			assert_eq!(function.inputs.len(), 2);
			assert_eq!(function.inputs[0].name, "to");
			assert_eq!(function.inputs[1].name, "amount");
		}
		_ => panic!("Expected Stellar contract spec"),
	}
}
