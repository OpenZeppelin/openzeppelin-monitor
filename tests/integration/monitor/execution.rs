use crate::integration::{
	filters::common::{
		setup_monitor_service, setup_network_service, setup_trigger_service, TestDataBuilder,
	},
	mocks::{
		create_test_network, MockClientPool, MockEVMTransportClient, MockEvmClientTrait,
		MockFilterService, MockMidnightClientTrait, MockMidnightWsTransportClient,
		MockNetworkRepository, MockSolanaClientTrait, MockSolanaTransportClient,
		MockStellarClientTrait, MockStellarTransportClient, MockTriggerRepository,
	},
};
use mockall::predicate;
use openzeppelin_monitor::{
	models::{
		AddressWithSpec, BlockChainType, BlockType, EVMTransactionReceipt, MatchConditions,
		Monitor, ScriptLanguage, SolanaBlock, SolanaConfirmedBlock, SolanaInstruction,
		SolanaTransaction, SolanaTransactionInfo, SolanaTransactionMessage, SolanaTransactionMeta,
		TransactionStatus, Trigger, TriggerConditions,
	},
	repositories::{
		MonitorRepository, MonitorRepositoryTrait, NetworkRepository, NetworkService,
		RepositoryError, TriggerRepository, TriggerService,
	},
	services::{
		filter::{FilterError, FilterService},
		notification::NotificationService,
		trigger::TriggerExecutionService,
	},
	utils::{
		monitor::execution::{execute_monitor, MonitorExecutionConfig},
		tests::builders::{evm::monitor::MonitorBuilder, trigger::TriggerBuilder},
	},
};
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	sync::Arc,
};
use tempfile::TempDir;
use tokio::sync::Mutex;

fn setup_mocked_network_service(
	network_name: &str,
	network_slug: &str,
	block_chain_type: BlockChainType,
) -> NetworkService<MockNetworkRepository> {
	let mut mocked_networks = HashMap::new();
	mocked_networks.insert(
		network_slug.to_string(),
		create_test_network(network_name, network_slug, block_chain_type),
	);
	setup_network_service(mocked_networks)
}

// Helper to create a valid monitor JSON file
fn create_test_monitor_file(
	path: &Path,
	name: &str,
	triggers: Vec<&str>,
	networks: Vec<&str>,
) -> std::path::PathBuf {
	let monitor_path = path.join(format!("{}.json", name));
	let monitor_json = serde_json::json!({
		"name": name,
		"paused": false,
		"networks": networks,
		"addresses": [],
		"match_conditions": {
			"functions": [],
			"events": [],
			"transactions": []
		},
		"trigger_conditions": [],
		"triggers": triggers,
	});
	fs::write(&monitor_path, monitor_json.to_string()).unwrap();
	monitor_path
}

// Helper to create a valid network JSON file
fn create_test_network_file(path: &Path, name: &str) -> std::path::PathBuf {
	let network_path = path.join(format!("{}.json", name));
	let network_json = serde_json::json!({
		"network_type": "EVM",
		"slug": name,
		"name": name,
		"rpc_urls": [
			{
			"type_": "rpc",
			"url": "https://eth.drpc.org",
			"weight": 100
			}
		],
		"chain_id": 1,
		"block_time_ms": 12000,
		"confirmation_blocks": 12,
		"cron_schedule": "0 */1 * * * *",
		"max_past_blocks": 18,
		"store_blocks": false
	});
	fs::write(&network_path, network_json.to_string()).unwrap();
	network_path
}

// Helper to create a valid trigger JSON file
fn create_test_trigger_file(path: &Path, name: &str) -> std::path::PathBuf {
	let trigger_path = path.join(format!("{}.json", name));
	let trigger_json = serde_json::json!({
		name: {
			"name": name,
			"trigger_type": "slack",
			"config": {
			  "slack_url": {
				"type": "plain",
				"value": "https://hooks.slack.com/services/AA/BB/CC"
			  },
			  "message": {
				"title": "large_transfer_slack triggered",
				"body": "Large transfer of ${events.0.args.value} USDC from ${events.0.args.from} to ${events.0.args.to} | https://etherscan.io/tx/${transaction.hash}#eventlog"
			  }
			}
		},
	});
	fs::write(&trigger_path, trigger_json.to_string()).unwrap();
	trigger_path
}

fn create_test_trigger(name: &str) -> Trigger {
	TriggerBuilder::new()
		.name(name)
		.email(
			"smtp.example.com",
			"user@example.com",
			"password123",
			"alerts@example.com",
			vec!["user@example.com"],
		)
		.message("Alert", "Something happened!")
		.build()
}

// Solana test helpers
fn create_solana_test_monitor() -> Monitor {
	Monitor {
		name: "Test Solana Monitor".to_string(),
		paused: false,
		networks: vec!["solana_devnet".to_string()],
		addresses: vec![AddressWithSpec {
			address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			contract_spec: None,
		}],
		match_conditions: MatchConditions {
			functions: vec![],
			events: vec![],
			transactions: vec![openzeppelin_monitor::models::TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			}],
		},
		trigger_conditions: vec![],
		triggers: vec![],
		chain_configurations: vec![],
	}
}

fn create_solana_test_transaction() -> SolanaTransaction {
	SolanaTransaction::from(SolanaTransactionInfo {
		signature: "5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
			.to_string(),
		slot: 123456789,
		block_time: Some(1234567890),
		transaction: SolanaTransactionMessage {
			account_keys: vec![
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				"11111111111111111111111111111111".to_string(),
			],
			recent_blockhash: "ABC123".to_string(),
			instructions: vec![SolanaInstruction {
				program_id_index: 0,
				accounts: vec![1, 2],
				data: "3Bxs4h24hBtQy9rw".to_string(),
				parsed: None,
				program: Some("spl-token".to_string()),
				program_id: Some("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()),
			}],
			address_table_lookups: vec![],
		},
		meta: Some(SolanaTransactionMeta {
			err: None,
			fee: 5000,
			pre_balances: vec![1000000000],
			post_balances: vec![999000000],
			inner_instructions: vec![],
			log_messages: vec![
				"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]".to_string(),
				"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success".to_string(),
			],
			pre_token_balances: vec![],
			post_token_balances: vec![],
			compute_units_consumed: None,
			loaded_addresses: None,
		}),
	})
}

fn create_solana_test_block(transactions: Vec<SolanaTransaction>) -> BlockType {
	BlockType::Solana(Box::new(SolanaBlock::from(SolanaConfirmedBlock {
		slot: 123456789,
		blockhash: "ABC123".to_string(),
		previous_blockhash: "ZYX987".to_string(),
		parent_slot: 123456788,
		block_time: Some(1234567890),
		block_height: Some(123456789),
		transactions,
	})))
}

fn create_test_monitor(
	name: &str,
	networks: Vec<&str>,
	paused: bool,
	triggers: Vec<&str>,
) -> Monitor {
	MonitorBuilder::new()
		.name(name)
		.networks(networks.into_iter().map(|s| s.to_string()).collect())
		.paused(paused)
		.triggers(triggers.into_iter().map(|s| s.to_string()).collect())
		.build()
}

#[tokio::test]
async fn test_execute_monitor_evm() {
	let test_data = TestDataBuilder::new("evm").build();
	let receipts = test_data.receipts.clone();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(21305050u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(move |_, _, _| {
			Ok(test_data
				.receipts
				.clone()
				.into_iter()
				.flat_map(|r| r.logs.clone())
				.collect())
		});

	let receipt_map: std::collections::HashMap<String, EVMTransactionReceipt> = receipts
		.iter()
		.map(|r| (format!("0x{:x}", r.transaction_hash), r.clone()))
		.collect();

	let receipt_map = Arc::new(receipt_map);
	mock_client
		.expect_get_transaction_receipt()
		.returning(move |hash| {
			let receipt_map = Arc::clone(&receipt_map);
			Ok(receipt_map
				.get(&hash)
				.cloned()
				.unwrap_or_else(|| panic!("Receipt not found for hash: {}", hash)))
		});

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let block_number = 21305050;

	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(
		result.is_ok(),
		"Monitor execution failed: {:?}",
		result.err()
	);

	// Parse the JSON result and add more specific assertions based on expected matches
	let matches: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
	assert!(matches.len() == 1);
}

#[tokio::test]
async fn test_execute_monitor_evm_wrong_network() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let block_number = 22197425;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_goerli".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_evm_wrong_block_number() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let block_number = 1;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_evm_failed_to_get_evm_client() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Err(anyhow::anyhow!("Failed to get evm client")));

	let client_pool = Arc::new(mock_pool);

	let block_number = 1;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_stellar() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));
	mock_client
		.expect_get_transactions()
		.return_once(move |_, _| Ok(test_data.stellar_transactions.clone()));
	mock_client
		.expect_get_events()
		.return_once(move |_, _| Ok(test_data.stellar_events.clone()));
	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(
		result.is_ok(),
		"Monitor execution failed: {:?}",
		result.err()
	);

	// Parse the JSON result and add more specific assertions based on expected matches
	let matches: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
	assert!(matches.len() == 1);
}

#[tokio::test]
async fn test_execute_monitor_failed_to_get_block() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();
	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_failed_to_get_stellar_client() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_pool
		.expect_get_stellar_client()
		.returning(move |_| Err(anyhow::anyhow!("Failed to get stellar client")));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_failed_to_get_block_by_number() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block by number")));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_get_latest_block_number_failed() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_network_slug_not_defined() {
	let test_data = TestDataBuilder::new("evm").build();
	let receipts = test_data.receipts.clone();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(move |_, _, _| Ok(vec![]));

	let receipt_map: std::collections::HashMap<String, EVMTransactionReceipt> = receipts
		.iter()
		.map(|r| (format!("0x{:x}", r.transaction_hash), r.clone()))
		.collect();
	let receipt_map = Arc::new(receipt_map);
	mock_client
		.expect_get_transaction_receipt()
		.returning(move |hash| {
			let receipt_map = Arc::clone(&receipt_map);
			Ok(receipt_map
				.get(&hash)
				.cloned()
				.unwrap_or_else(|| panic!("Receipt not found for hash: {}", hash)))
		});

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(100u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: None,
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_monitor_stellar_get_latest_block_number_failed() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let block_number = 22197425;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_goerli".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_execute_monitor_evm_failed_to_get_block_by_number() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let block_number = 1;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get block"));
}

#[tokio::test]
async fn test_execute_monitor_evm_failed_to_get_latest_block_number() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get latest block number"));
}

#[tokio::test]
async fn test_execute_monitor_evm_with_trigger_scripts() {
	let mut test_data = TestDataBuilder::new("evm").build();
	let receipts = test_data.receipts.clone();
	let mut mocked_monitors = HashMap::new();
	test_data.monitor.trigger_conditions = vec![TriggerConditions {
		script_path: "./examples/config/filters/evm_large_transfer_usdc.py".to_string(),
		language: ScriptLanguage::Python,
		timeout_ms: 10000,
		arguments: None,
		runtime_flags: None,
	}];
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(21305050u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	mock_client
		.expect_get_logs_for_blocks()
		.return_once(move |_, _, _| {
			Ok(test_data
				.receipts
				.clone()
				.into_iter()
				.flat_map(|r| r.logs.clone())
				.collect())
		});
	let receipt_map: std::collections::HashMap<String, EVMTransactionReceipt> = receipts
		.iter()
		.map(|r| (format!("0x{:x}", r.transaction_hash), r.clone()))
		.collect();

	let receipt_map = Arc::new(receipt_map);
	mock_client
		.expect_get_transaction_receipt()
		.returning(move |hash| {
			let receipt_map = Arc::clone(&receipt_map);
			Ok(receipt_map
				.get(&hash)
				.cloned()
				.unwrap_or_else(|| panic!("Receipt not found for hash: {}", hash)))
		});

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 21305050;

	let trigger_service = setup_trigger_service(HashMap::new());
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let mut trigger_scripts = HashMap::new();
	trigger_scripts.insert(
		"monitor".to_string(),
		(
			ScriptLanguage::Python,
			"evm_large_transfer_usdc".to_string(),
		),
	);

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: trigger_scripts,
		client_pool,
	})
	.await;
	assert!(
		result.is_ok(),
		"Monitor execution failed: {:?}",
		result.err()
	);

	let matches: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
	assert!(matches.len() == 1);
}

#[tokio::test]
async fn test_execute_monitor_stellar_failed_to_get_block() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();
	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_execute_monitor_stellar_failed_to_get_stellar_client() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_pool
		.expect_get_stellar_client()
		.returning(move |_| Err(anyhow::anyhow!("Failed to get stellar client")));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get stellar client"));
}

#[tokio::test]
async fn test_execute_monitor_stellar_failed_to_get_block_by_number() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block")));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 172627;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get block"));
}

#[tokio::test]
async fn test_execute_monitor_stellar_failed_to_get_latest_block_number() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_mainnet", BlockChainType::Stellar);
	let mut mock_client = MockStellarClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_mainnet".to_string()),
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get latest block number"));
}

#[tokio::test]
async fn test_execute_monitor_midnight() {
	let test_data = TestDataBuilder::new("midnight").build();

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockMidnightClientTrait::new();

	mock_client
		.expect_get_chain_type()
		.returning(move || Ok("testnet-02-1".to_string()));
	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	// Create actual TriggerExecutionService instance
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let block_number = 11243;

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(block_number), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_midnight_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(
		result.is_ok(),
		"Monitor execution failed: {:?}",
		result.err()
	);

	let matches: Vec<serde_json::Value> = serde_json::from_str(&result.unwrap()).unwrap();
	assert!(matches.len() == 1);
}

#[tokio::test]
async fn test_execute_monitor_midnight_failed_to_get_block() {
	let test_data = TestDataBuilder::new("midnight").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockMidnightClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(11243u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_midnight_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 11243;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_execute_monitor_midnight_failed_to_get_midnight_client() {
	let test_data = TestDataBuilder::new("midnight").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);
	let mut mock_pool = MockClientPool::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_pool
		.expect_get_midnight_client()
		.returning(move |_| Err(anyhow::anyhow!("Failed to get midnight client")));

	let client_pool = Arc::new(mock_pool);

	let block_number = 11243;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get midnight client"));
}

#[tokio::test]
async fn test_execute_monitor_midnight_failed_to_get_blocks() {
	let test_data = TestDataBuilder::new("midnight").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockMidnightClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(11243u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_midnight_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 11243;

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get block"));
}

#[tokio::test]
async fn test_execute_monitor_midnight_failed_to_get_latest_block_number() {
	let test_data = TestDataBuilder::new("midnight").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockMidnightClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_midnight_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get latest block number"));
}

#[tokio::test]
async fn test_load_from_path() {
	// Setup temporary directory and files
	let temp_dir = TempDir::new().unwrap();
	let monitor_path = create_test_monitor_file(
		temp_dir.path(),
		"monitor",
		vec!["test-trigger"],
		vec!["ethereum_mainnet"],
	);

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert(
		"monitor".to_string(),
		create_test_monitor(
			"monitor",
			vec!["ethereum_mainnet"],
			false,
			vec!["test-trigger"],
		),
	);

	// Create monitor service
	let monitor_service = setup_monitor_service(mocked_monitors);

	// Test loading from path
	let result = monitor_service
		.load_from_path(Some(&monitor_path), None, None)
		.await;

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[tokio::test]
async fn test_load_from_path_with_services() {
	// Setup temporary directory and files
	let temp_dir = TempDir::new().unwrap();
	let monitor_path = create_test_monitor_file(
		temp_dir.path(),
		"monitor",
		vec!["test-trigger"],
		vec!["ethereum_mainnet"],
	);

	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert("test-trigger".to_string(), create_test_trigger("test"));
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert(
		"monitor".to_string(),
		create_test_monitor(
			"monitor",
			vec!["ethereum_mainnet"],
			false,
			vec!["test-trigger"],
		),
	);

	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let result = mock_monitor_service
		.load_from_path(
			Some(&monitor_path),
			Some(mock_network_service),
			Some(mock_trigger_service),
		)
		.await;

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[tokio::test]
async fn test_load_from_path_trait_implementation() {
	// Setup temporary directory and files
	let temp_dir = TempDir::new().unwrap();
	let monitor_path = create_test_monitor_file(
		temp_dir.path(),
		"monitor",
		vec!["test-trigger"],
		vec!["ethereum_mainnet"],
	);

	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert("test-trigger".to_string(), create_test_trigger("test"));
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert(
		"monitor".to_string(),
		create_test_monitor(
			"monitor",
			vec!["ethereum_mainnet"],
			false,
			vec!["test-trigger"],
		),
	);

	// Create repository directly
	let repository = MonitorRepository::new_with_monitors(mocked_monitors);

	// Test the trait implementation directly
	let result =
		<MonitorRepository<MockNetworkRepository, MockTriggerRepository> as MonitorRepositoryTrait<
			MockNetworkRepository,
			MockTriggerRepository,
		>>::load_from_path(
			&repository,
			Some(&monitor_path),
			Some(mock_network_service),
			Some(mock_trigger_service),
		)
		.await;

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[tokio::test]
async fn test_load_from_path_trait_implementation_error() {
	// Setup temporary directory and files
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert("test-trigger".to_string(), create_test_trigger("test"));
	let mock_trigger_service = setup_trigger_service(mocked_triggers);

	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert(
		"monitor".to_string(),
		create_test_monitor(
			"monitor",
			vec!["ethereum_mainnet"],
			false,
			vec!["test-trigger"],
		),
	);

	// Create repository directly
	let repository = MonitorRepository::new_with_monitors(mocked_monitors);

	// Test the trait implementation directly
	let result =
		<MonitorRepository<MockNetworkRepository, MockTriggerRepository> as MonitorRepositoryTrait<
			MockNetworkRepository,
			MockTriggerRepository,
		>>::load_from_path(
			&repository,
			None,
			Some(mock_network_service),
			Some(mock_trigger_service),
		)
		.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to load monitors"));
}

// This test is ignored because it creates files in the config directory
// and we don't want to pollute the default config directory
#[tokio::test]
#[cfg_attr(not(feature = "test-ci-only"), ignore)]
async fn test_load_from_path_with_mixed_services() {
	// Create default config paths for when we use None for path
	let config_path = PathBuf::from("config");
	let network_path = config_path.join("networks");
	let trigger_path = config_path.join("triggers");
	let monitor_path = config_path.join("monitors");

	std::fs::create_dir_all(&config_path).unwrap();
	std::fs::create_dir_all(&network_path).unwrap();
	std::fs::create_dir_all(&trigger_path).unwrap();
	std::fs::create_dir_all(&monitor_path).unwrap();

	let network_path = create_test_network_file(&network_path, "integration_test_ethereum_mainnet");
	let network_repo = NetworkRepository::new(Some(network_path.parent().unwrap()))
		.await
		.unwrap();
	let network_service = NetworkService::new_with_repository(network_repo).unwrap();

	let trigger_path = create_test_trigger_file(&trigger_path, "integration_test_trigger");
	let trigger_repo = TriggerRepository::new(Some(trigger_path.parent().unwrap()))
		.await
		.unwrap();
	let trigger_service = TriggerService::new_with_repository(trigger_repo).unwrap();

	let repository = MonitorRepository::<NetworkRepository, TriggerRepository>::new_with_monitors(
		HashMap::new(),
	);

	// Test 1: With no services
	let monitor_path = create_test_monitor_file(
		&monitor_path,
		"integration_test_monitor",
		vec![],
		vec!["integration_test_ethereum_mainnet"],
	);
	let result = repository
		.load_from_path(Some(&monitor_path), None, None)
		.await;
	assert!(result.is_ok());

	// Test 2: Empty monitor content
	let monitor_temp_dir = TempDir::new().unwrap();
	let result = repository
		.load_from_path(Some(monitor_temp_dir.path()), None, None)
		.await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(matches!(err, RepositoryError::LoadError(_)));
	assert!(err.to_string().contains("Failed to load monitors"));

	// Test 3: Mixed service configuration
	let result = repository
		.load_from_path(Some(&monitor_path), Some(network_service.clone()), None)
		.await;
	assert!(result.is_ok());

	let result = repository
		.load_from_path(Some(&monitor_path), None, Some(trigger_service.clone()))
		.await;
	assert!(result.is_ok());

	// Test 4: Invalid monitor references
	let invalid_monitor_path = create_test_monitor_file(
		monitor_temp_dir.path(),
		"invalid_monitor",
		vec!["invalid-trigger"],
		vec!["integration_test_ethereum_mainnet"],
	);
	let result = repository
		.load_from_path(Some(&invalid_monitor_path), None, None)
		.await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("references non-existent"));

	// Clean up after test
	// Remove integration_test_* files from config directory
	std::fs::remove_file(network_path).unwrap();
	std::fs::remove_file(trigger_path).unwrap();
	std::fs::remove_file(monitor_path).unwrap();
}

#[tokio::test]
async fn test_filter_block_failure_evm() {
	let test_data = TestDataBuilder::new("evm").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"evm_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	// Mock the client to fail when getting blocks
	mock_client
		.expect_get_blocks()
		.with(predicate::eq(12345u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(mock_client));

	let client_pool = Arc::new(mock_pool);

	let mut mock_filter_service = MockFilterService::new();

	mock_filter_service
		.expect_filter_block::<MockEvmClientTrait<MockEVMTransportClient>>()
		.returning(
			move |_: &MockEvmClientTrait<MockEVMTransportClient>, _, _, _| {
				Err(FilterError::internal_error(
					"Internal Error".to_string(),
					None,
					None,
				))
			},
		);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("ethereum_mainnet".to_string()),
		block_number: Some(12345),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(mock_filter_service),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to filter block"));
}

#[tokio::test]
async fn test_filter_block_failure_midnight() {
	let test_data = TestDataBuilder::new("midnight").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Midnight", "midnight_testnet", BlockChainType::Midnight);
	let mut mock_client = MockMidnightClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"midnight_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	// Mock the client to fail when getting blocks
	mock_client
		.expect_get_blocks()
		.with(predicate::eq(11243u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_midnight_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let mut mock_filter_service = MockFilterService::new();

	mock_filter_service
		.expect_filter_block::<MockMidnightClientTrait<MockMidnightWsTransportClient>>()
		.returning(
			move |_: &MockMidnightClientTrait<MockMidnightWsTransportClient>, _, _, _| {
				Err(FilterError::internal_error(
					"Internal Error".to_string(),
					None,
					None,
				))
			},
		);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("midnight_testnet".to_string()),
		block_number: Some(11243),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(mock_filter_service),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to filter block"));
}

#[tokio::test]
async fn test_filter_block_failure_stellar() {
	let test_data = TestDataBuilder::new("stellar").build();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_client = MockStellarClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"stellar_large_transfer_usdc_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(11243u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_stellar_client()
		.times(2)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let mut mock_filter_service = MockFilterService::new();

	mock_filter_service
		.expect_filter_block::<MockStellarClientTrait<MockStellarTransportClient>>()
		.returning(
			move |_: &MockStellarClientTrait<MockStellarTransportClient>, _, _, _| {
				Err(FilterError::internal_error(
					"Internal Error".to_string(),
					None,
					None,
				))
			},
		);

	let result = execute_monitor(MonitorExecutionConfig {
		path: test_data.monitor.name.clone(),
		network_slug: Some("stellar_testnet".to_string()),
		block_number: Some(11243),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(mock_filter_service),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to filter block"));
}

// Solana execution tests
#[tokio::test]
async fn test_execute_monitor_solana() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockSolanaClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let transaction = create_solana_test_transaction();
	let block = create_solana_test_block(vec![transaction.clone()]);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(123456789u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![block]));

	mock_client
		.expect_get_transactions()
		.returning(move |_| Ok(vec![transaction.clone()]));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_solana_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 123456789;

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(
		result.is_ok(),
		"Monitor execution failed: {:?}",
		result.err()
	);
}

#[tokio::test]
async fn test_execute_monitor_solana_failed_to_get_block() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockSolanaClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(123456789u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_solana_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 123456789;

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not found"));
}

#[tokio::test]
async fn test_execute_monitor_solana_failed_to_get_solana_client() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);
	let mut mock_pool = MockClientPool::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_pool
		.expect_get_solana_client()
		.returning(move |_| Err(anyhow::anyhow!("Failed to get solana client")));

	let client_pool = Arc::new(mock_pool);

	let block_number = 123456789;

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get solana client"));
}

#[tokio::test]
async fn test_execute_monitor_solana_failed_to_get_blocks() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockSolanaClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(123456789u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_solana_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let block_number = 123456789;

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: Some(block_number),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get block"));
}

#[tokio::test]
async fn test_execute_monitor_solana_failed_to_get_latest_block_number() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);
	let mut mock_client = MockSolanaClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_solana_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: None,
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(FilterService::new()),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get latest block number"));
}

#[tokio::test]
async fn test_filter_block_failure_solana() {
	let monitor = create_solana_test_monitor();
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_network_service("Solana", "solana_devnet", BlockChainType::Solana);
	let mut mock_client = MockSolanaClientTrait::new();

	let mut mocked_triggers = HashMap::new();
	mocked_triggers.insert(
		"solana_large_transfer_slack".to_string(),
		create_test_trigger("test"),
	);
	let trigger_service = setup_trigger_service(mocked_triggers);
	let notification_service = NotificationService::new();
	let trigger_execution_service =
		TriggerExecutionService::new(trigger_service, notification_service);

	let transaction = create_solana_test_transaction();
	let block = create_solana_test_block(vec![transaction]);

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(123456789u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![block]));

	let mock_client = Arc::new(mock_client);

	mock_pool
		.expect_get_solana_client()
		.times(1)
		.returning(move |_| Ok(mock_client.clone()));

	let client_pool = Arc::new(mock_pool);

	let mut mock_filter_service = MockFilterService::new();

	mock_filter_service
		.expect_filter_block::<MockSolanaClientTrait<MockSolanaTransportClient>>()
		.returning(
			move |_: &MockSolanaClientTrait<MockSolanaTransportClient>, _, _, _| {
				Err(FilterError::internal_error(
					"Internal Error".to_string(),
					None,
					None,
				))
			},
		);

	let result = execute_monitor(MonitorExecutionConfig {
		path: monitor.name.clone(),
		network_slug: Some("solana_devnet".to_string()),
		block_number: Some(123456789),
		monitor_service: Arc::new(Mutex::new(mock_monitor_service)),
		network_service: Arc::new(Mutex::new(mock_network_service)),
		filter_service: Arc::new(mock_filter_service),
		trigger_execution_service: Arc::new(trigger_execution_service),
		active_monitors_trigger_scripts: HashMap::new(),
		client_pool,
	})
	.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to filter block"));
}
