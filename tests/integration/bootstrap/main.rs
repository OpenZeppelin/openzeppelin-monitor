use crate::integration::{
	filters::common::{
		setup_monitor_service, setup_network_service, setup_trigger_execution_service,
		setup_trigger_service,
	},
	mocks::{
		create_test_block, create_test_network, MockClientPool, MockEvmClientTrait,
		MockStellarClientTrait, MockTriggerExecutionService, MockTriggerRepository,
		MockWeb3TransportClient,
	},
};
use openzeppelin_monitor::{
	bootstrap::{create_block_handler, create_trigger_handler, initialize_services, process_block},
	models::{
		BlockChainType, EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch,
		NotificationMessage, ProcessedBlock, ScriptLanguage, StellarBlock, StellarMonitorMatch,
		StellarTransaction, StellarTransactionInfo, Trigger, TriggerConditions, TriggerType,
		TriggerTypeConfig,
	},
	services::{
		blockchain::BlockChainError,
		filter::FilterService,
		notification::NotificationService,
		trigger::{TriggerError, TriggerExecutionService, TriggerExecutionServiceTrait},
	},
};

use std::{collections::HashMap, sync::Arc};
use tempfile;
use tokio::sync::watch;
use web3::types::{H160, U256};

fn create_test_monitor(
	name: &str,
	networks: Vec<&str>,
	paused: bool,
	triggers: Vec<&str>,
) -> Monitor {
	Monitor {
		name: name.to_string(),
		networks: networks.into_iter().map(|s| s.to_string()).collect(),
		paused,
		triggers: triggers.into_iter().map(|s| s.to_string()).collect(),
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

fn create_test_stellar_transaction() -> StellarTransaction {
	StellarTransaction::from({
		StellarTransactionInfo {
			..Default::default()
		}
	})
}

fn create_test_trigger(name: &str) -> Trigger {
	Trigger {
		name: name.to_string(),
		trigger_type: TriggerType::Slack,
		config: TriggerTypeConfig::Slack {
			slack_url:
				"https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXXXXXXXXXXXXXXXXXX"
					.to_string(),
			message: NotificationMessage {
				title: "Test Title".to_string(),
				body: "Test Body".to_string(),
			},
		},
	}
}

fn create_test_monitor_match(chain: BlockChainType) -> MonitorMatch {
	match chain {
		BlockChainType::EVM => MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test", vec!["ethereum_mainnet"], false, vec![]),
			transaction: create_test_evm_transaction(),
			receipt: web3::types::TransactionReceipt::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		BlockChainType::Stellar => MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
			monitor: create_test_monitor("test", vec!["stellar_mainnet"], false, vec![]),
			transaction: create_test_stellar_transaction(),
			ledger: StellarBlock::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		_ => panic!("Unsupported chain"),
	}
}

#[test]
fn test_initialize_services() {
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
	let (filter_service, trigger_execution_service, active_monitors, networks) =
		initialize_services(
			Some(mock_monitor_service),
			Some(mock_network_service),
			Some(mock_trigger_service),
		)
		.expect("Failed to initialize services");

	assert!(
		Arc::strong_count(&filter_service) == 1,
		"FilterService should be wrapped in Arc"
	);
	assert!(
		Arc::strong_count(&trigger_execution_service) == 1,
		"TriggerExecutionService should be wrapped in Arc"
	);

	assert!(active_monitors.iter().any(|m| m.name == "test"
		&& m.networks.contains(&"ethereum_mainnet".to_string())
		&& m.triggers
			.contains(&"evm_large_transfer_usdc_slack".to_string())));
	assert!(networks.contains_key("ethereum_mainnet"));
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

	// Create a mock client pool that returns a successful client
	let mut mock_pool = MockClientPool::new();
	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(MockEvmClientTrait::new())));
	let client_pool = Arc::new(mock_pool);

	let block_handler =
		create_block_handler::<MockClientPool>(shutdown_tx, filter_service, monitors, client_pool);

	let result = block_handler(block, network).await;
	assert_eq!(result.block_number, 100);
	assert_eq!(result.network_slug, "ethereum_mainnet");
	// The mock client should return no matches
	assert!(result.processing_results.is_empty());
}

#[tokio::test]
async fn test_create_trigger_handler() {
	// Set up expectation for the constructor first
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_trigger_service, _notification_service| {
			let mut mock = MockTriggerExecutionService::default();
			mock.expect_execute().times(1).return_once(|_, _| Ok(()));
			mock
		});

	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

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
	};

	let handle = trigger_handler(&processed_block);
	handle
		.await
		.expect("Trigger handler task should complete successfully");
}

#[tokio::test]
async fn test_create_trigger_handler_empty_matches() {
	// Set up expectation for the constructor first
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_trigger_service, _notification_service| {
			let mut mock = MockTriggerExecutionService::default();
			mock.expect_execute().times(0);
			mock
		});

	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

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

	// Create a mock client pool that returns a successful client

	let mut mock_pool = MockClientPool::new();

	mock_pool.expect_get_stellar_client().returning(move |_| {
		let mut mock_client = MockStellarClientTrait::new();

		// Stellar does an additional call to get the transactions as opposed to EVM where

		// transactions are already in the block

		mock_client
			.expect_get_transactions()
			.times(1)
			.returning(move |_, _| Ok(vec![]));

		Ok(Arc::new(mock_client))
	});

	let client_pool = Arc::new(mock_pool);

	let block_handler =
		create_block_handler::<MockClientPool>(shutdown_tx, filter_service, monitors, client_pool);

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

	mock_pool.expect_get_evm_client().return_once(move |_| {
		Err(BlockChainError::client_pool_error(
			"Failed to get EVM client".to_string(),
		))
	});

	let client_pool = Arc::new(mock_pool);

	let block_handler =
		create_block_handler::<MockClientPool>(shutdown_tx, filter_service, monitors, client_pool);

	let result = block_handler(block, network).await;

	assert_eq!(result.block_number, 100);

	assert_eq!(result.network_slug, "ethereum_mainnet");

	assert!(result.processing_results.is_empty());
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

	mock_pool.expect_get_stellar_client().return_once(move |_| {
		Err(BlockChainError::client_pool_error(
			"Failed to get Stellar client".to_string(),
		))
	});

	let client_pool = Arc::new(mock_pool);

	let block_handler =
		create_block_handler::<MockClientPool>(shutdown_tx, filter_service, monitors, client_pool);

	let result = block_handler(block, network).await;

	assert_eq!(result.block_number, 100);

	assert_eq!(result.network_slug, "stellar_mainnet");

	assert!(result.processing_results.is_empty());
}

#[tokio::test]
async fn test_create_trigger_handler_with_conditions() {
	// Set up expectation for the constructor first
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_trigger_service, _notification_service| {
			let mut mock = MockTriggerExecutionService::default();
			mock.expect_execute().times(1).return_once(|_, _| Ok(()));
			mock
		});

	// Setup test triggers in JSON with known configurations
	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

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
	}];

	let processed_block = ProcessedBlock {
		block_number: 100,
		network_slug: "ethereum_mainnet".to_string(),
		processing_results: vec![MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor,
			transaction: create_test_evm_transaction(),
			receipt: web3::types::TransactionReceipt::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}))],
	};

	let handle = trigger_handler(&processed_block);
	handle
		.await
		.expect("Trigger handler task should complete successfully");
}

#[tokio::test]
async fn test_process_block() {
	let mut mock_client = MockEvmClientTrait::<MockWeb3TransportClient>::new();
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

	let result = process_block(
		&mock_client,
		&network,
		&block,
		&monitors,
		&filter_service,
		&mut shutdown_rx,
	)
	.await;

	assert!(
		!*shutdown_rx.borrow(),
		"Shutdown signal was unexpectedly triggered"
	);
	assert!(
		result.is_some(),
		"Expected Some result when no shutdown signal"
	);
}

#[tokio::test]
#[ignore]
/// Skipping as this test is flaky and fails intermittently
async fn test_process_block_with_shutdown() {
	let mock_client = MockEvmClientTrait::<MockWeb3TransportClient>::new();
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
		&filter_service,
		&mut shutdown_rx,
	)
	.await;

	assert!(
		result.is_none(),
		"Expected None when shutdown signal is received"
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
	let monitors = vec![Monitor {
		name: "test_monitor".to_string(),
		trigger_conditions: vec![TriggerConditions {
			script_path: script_path.to_str().unwrap().to_string(),
			language: ScriptLanguage::Python,
			timeout_ms: 1000,
			arguments: None,
		}],
		..Default::default()
	}];

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
	let monitors = vec![Monitor {
		name: "test_monitor".to_string(),
		trigger_conditions: vec![TriggerConditions {
			script_path: "non_existent_script.py".to_string(),
			language: ScriptLanguage::Python,
			timeout_ms: 1000,
			arguments: None,
		}],
		..Default::default()
	}];

	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	// Test loading scripts
	let result = trigger_execution_service.load_scripts(&monitors).await;
	assert!(result.is_err());

	match result {
		Err(e) => {
			assert!(matches!(e, TriggerError::ConfigurationError(_)));
			assert!(e.to_string().contains("Failed to read script file"));
		}
		_ => panic!("Expected error"),
	}
}

#[tokio::test]
async fn test_load_scripts_empty_conditions() {
	// Create test monitors with empty trigger conditions
	let monitors = vec![Monitor {
		name: "test_monitor".to_string(),
		trigger_conditions: vec![], // Empty trigger conditions
		..Default::default()
	}];

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
