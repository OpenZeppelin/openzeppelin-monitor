use crate::integration::{
	filters::common::{
		setup_monitor_service, setup_network_service, setup_trigger_execution_service,
		setup_trigger_service,
	},
	mocks::{create_test_block, create_test_network, MockEvmClientTrait},
};
use openzeppelin_monitor::{
	bootstrap::{create_block_handler, create_trigger_handler, initialize_services, process_block},
	models::{
		BlockChainType, EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch,
		NotificationMessage, ProcessedBlock, StellarBlock, StellarMonitorMatch, StellarTransaction,
		StellarTransactionInfo, Trigger, TriggerType, TriggerTypeConfig,
	},
	services::filter::FilterService,
};

use std::{collections::HashMap, sync::Arc};
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
async fn test_create_block_handler() {
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
	let block_handler = create_block_handler(shutdown_tx, filter_service, monitors);

	assert!(Arc::strong_count(&block_handler) == 1);

	let result = block_handler(block, network).await;
	assert!(result.block_number == 100);
	assert!(result.network_slug == "ethereum_mainnet");
	assert!(result.processing_results.is_empty());
}

#[tokio::test]
async fn test_create_trigger_handler() {
	// Setup test triggers in JSON with known configurations
	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

	trigger_execution_service
		.expect_execute()
		.times(1)
		.return_once(|_, _| Ok(()));

	let (shutdown_tx, _) = watch::channel(false);
	let trigger_handler = create_trigger_handler(shutdown_tx, Arc::new(trigger_execution_service));

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
async fn test_process_block() {
	let mut mock_client = MockEvmClientTrait::new();
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
	let mock_client = MockEvmClientTrait::new();
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
