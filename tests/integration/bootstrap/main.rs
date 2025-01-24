use crate::integration::{
	filters::common::setup_trigger_execution_service, mocks::MockEvmClientTrait,
};
use openzeppelin_monitor::{
	bootstrap::{create_block_handler, create_trigger_handler, initialize_services, process_block},
	models::{
		BlockChainType, BlockType, EVMBlock, EVMMonitorMatch, EVMTransaction, MatchConditions,
		Monitor, MonitorMatch, Network, ProcessedBlock, RpcUrl, StellarBlock, StellarLedgerInfo,
		StellarMonitorMatch, StellarTransaction, StellarTransactionInfo,
	},
	services::filter::FilterService,
};

use std::sync::Arc;
use tokio::sync::watch;
use web3::types::{H160, U256};

fn create_test_monitor(name: &str, networks: Vec<&str>, paused: bool) -> Monitor {
	Monitor {
		name: name.to_string(),
		networks: networks.into_iter().map(|s| s.to_string()).collect(),
		paused,
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

fn create_test_network(name: &str, slug: &str, network_type: BlockChainType) -> Network {
	Network {
		name: name.to_string(),
		slug: slug.to_string(),
		network_type,
		rpc_urls: vec![RpcUrl {
			url: "http://localhost:8545".to_string(),
			type_: "rpc".to_string(),
			weight: 100,
		}],
		cron_schedule: "*/5 * * * * *".to_string(),
		confirmation_blocks: 1,
		store_blocks: Some(false),
		chain_id: Some(1),
		network_passphrase: None,
		block_time_ms: 1000,
		max_past_blocks: None,
	}
}

fn create_test_block(chain: BlockChainType, block_number: u64) -> BlockType {
	match chain {
		BlockChainType::EVM => BlockType::EVM(Box::new(EVMBlock::from(web3::types::Block {
			number: Some(block_number.into()),
			..Default::default()
		}))),
		BlockChainType::Stellar => {
			BlockType::Stellar(Box::new(StellarBlock::from(StellarLedgerInfo {
				sequence: block_number as u32,
				..Default::default()
			})))
		}
		_ => panic!("Unsupported chain"),
	}
}

fn create_test_monitor_match(chain: BlockChainType) -> MonitorMatch {
	match chain {
		BlockChainType::EVM => MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test", vec!["ethereum_mainnet"], false),
			transaction: create_test_evm_transaction(),
			receipt: web3::types::TransactionReceipt::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		BlockChainType::Stellar => MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
			monitor: create_test_monitor("test", vec!["stellar_mainnet"], false),
			transaction: create_test_stellar_transaction(),
			ledger: StellarBlock::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		})),
		_ => panic!("Unsupported chain"),
	}
}

#[test]
#[ignore]
/// Skipping as this test for now as it's expected to fail for CI but pass locally
/// We should mock the repositories called within initialize_services to avoid this
fn test_initialize_services() {
	// Initialize services
	let (filter_service, trigger_execution_service, active_monitors, networks) =
		initialize_services().expect("Failed to initialize services");

	assert!(
		Arc::strong_count(&filter_service) == 1,
		"FilterService should be wrapped in Arc"
	);
	assert!(
		Arc::strong_count(&trigger_execution_service) == 1,
		"TriggerExecutionService should be wrapped in Arc"
	);

	assert!(!active_monitors.is_empty());
	assert!(!networks.is_empty());
}

#[tokio::test]
async fn test_create_block_handler() {
	let (shutdown_tx, _) = watch::channel(false);
	let filter_service = Arc::new(FilterService::new());
	let monitors = vec![create_test_monitor("test", vec!["ethereum_mainnet"], false)];
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
	let monitors = vec![create_test_monitor("test", vec!["ethereum_mainnet"], false)];
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
	let monitors = vec![create_test_monitor("test", vec!["ethereum_mainnet"], false)];
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
