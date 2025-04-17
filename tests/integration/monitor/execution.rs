use crate::integration::{
	filters::common::{
		load_test_data, setup_monitor_service, setup_network_service, setup_trigger_service,
	},
	mocks::{
		create_test_network, MockClientPool, MockEvmClientTrait, MockNetworkRepository,
		MockStellarClientTrait, MockTriggerRepository,
	},
};
use mockall::predicate;
use openzeppelin_monitor::{
	models::{
		BlockChainType, EVMTransactionReceipt, Monitor, NotificationMessage, Trigger, TriggerType,
		TriggerTypeConfig,
	},
	repositories::{
		MonitorRepository, MonitorRepositoryTrait, NetworkRepository, NetworkService,
		RepositoryError, TriggerRepository, TriggerService,
	},
	services::filter::FilterService,
	utils::monitor::execution::execute_monitor,
};
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	sync::Arc,
};
use tempfile::TempDir;
use tokio::sync::Mutex;

fn setup_mocked_networks(
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
			  "slack_url": "https://hooks.slack.com/services/AA/BB/CC",
			  "message": {
				"title": "large_transfer_slack triggered",
				"body": "Large transfer of ${event_0_value} USDC from ${event_0_from} to ${event_0_to} | https://etherscan.io/tx/${transaction_hash}#eventlog"
			  }
			}
		},
	});
	fs::write(&trigger_path, trigger_json.to_string()).unwrap();
	trigger_path
}

fn create_test_trigger(name: &str) -> Trigger {
	Trigger {
		name: name.to_string(),
		trigger_type: TriggerType::Email,
		config: TriggerTypeConfig::Email {
			host: "smtp.example.com".to_string(),
			port: Some(465),
			username: "user@example.com".to_string(),
			password: "password123".to_string(),
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Something happened!".to_string(),
			},
			sender: "alerts@example.com".parse().unwrap(),
			recipients: vec!["user@example.com".parse().unwrap()],
		},
	}
}

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

#[tokio::test]
async fn test_execute_monitor_evm() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(21305050u64), predicate::eq(None))
		.return_once(move |_, _| Ok(test_data.blocks.clone()));

	let receipts = test_data.receipts.clone();
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

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_mainnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
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
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mock_client = MockEvmClientTrait::new();

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 22197425;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_goerli".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_evm_wrong_block_number() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 1;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_mainnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_evm_failed_to_get_block_by_number() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block by number")));

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 1;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_mainnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_evm_failed_to_get_evm_client() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Err(anyhow::anyhow!("Failed to get evm client")));

	let block_number = 1;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_mainnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_stellar() {
	let test_data = load_test_data("stellar");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_networks("Stellar", "stellar_testnet", BlockChainType::Stellar);

	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

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

	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 172627;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"stellar_testnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
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
	let test_data = load_test_data("stellar");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_networks("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Ok(vec![]));

	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 172627;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"stellar_testnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_failed_to_get_stellar_client() {
	let test_data = load_test_data("stellar");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_networks("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();

	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Err(anyhow::anyhow!("Failed to get stellar client")));

	let block_number = 172627;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"stellar_testnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_failed_to_get_block_by_number() {
	let test_data = load_test_data("stellar");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_network_service =
		setup_mocked_networks("Stellar", "stellar_testnet", BlockChainType::Stellar);
	let mut mock_pool = MockClientPool::new();
	let mut mock_client = MockStellarClientTrait::new();

	mock_client
		.expect_get_blocks()
		.with(predicate::eq(172627u64), predicate::eq(None))
		.return_once(move |_, _| Err(anyhow::anyhow!("Failed to get block by number")));

	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let block_number = 172627;

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"stellar_testnet".to_string()),
		Some(&block_number),
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_get_latest_block_number_failed() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"ethereum_mainnet".to_string()),
		None,
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_network_slug_not_defined() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);
	let mut mock_client = MockEvmClientTrait::new();

	mock_client
		.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let receipts = test_data.receipts.clone();

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

	mock_pool
		.expect_get_evm_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let result = execute_monitor(
		&test_data.monitor.name,
		None,
		None,
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;

	assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_monitor_midnight() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Midnight", "midnight_mainnet", BlockChainType::Midnight);

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"midnight_mainnet".to_string()),
		None,
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_solana() {
	let test_data = load_test_data("evm");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Solana", "solana_mainnet", BlockChainType::Solana);

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"solana_mainnet".to_string()),
		None,
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_monitor_stellar_get_latest_block_number_failed() {
	let test_data = load_test_data("stellar");
	let mut mocked_monitors = HashMap::new();
	mocked_monitors.insert("monitor".to_string(), test_data.monitor.clone());
	let mock_monitor_service = setup_monitor_service(mocked_monitors);

	let mut mock_pool = MockClientPool::new();
	let mock_network_service =
		setup_mocked_networks("Stellar", "stellar_mainnet", BlockChainType::Stellar);
	let mut mock_client = MockStellarClientTrait::new();

	mock_client
		.expect_get_latest_block_number()
		.return_once(move || Err(anyhow::anyhow!("Failed to get latest block number")));

	mock_pool
		.expect_get_stellar_client()
		.return_once(move |_| Ok(Arc::new(mock_client)));

	let result = execute_monitor(
		&test_data.monitor.name,
		Some(&"stellar_mainnet".to_string()),
		None,
		Arc::new(Mutex::new(mock_monitor_service)),
		Arc::new(Mutex::new(mock_network_service)),
		Arc::new(FilterService::new()),
		mock_pool,
	)
	.await;
	assert!(result.is_err());
}

#[test]
fn test_load_from_path() {
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
	let result = monitor_service.load_from_path(Some(&monitor_path), None, None);

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[test]
fn test_load_from_path_with_services() {
	// Setup temporary directory and files
	let temp_dir = TempDir::new().unwrap();
	let monitor_path = create_test_monitor_file(
		temp_dir.path(),
		"monitor",
		vec!["test-trigger"],
		vec!["ethereum_mainnet"],
	);

	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

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

	let result = mock_monitor_service.load_from_path(
		Some(&monitor_path),
		Some(mock_network_service),
		Some(mock_trigger_service),
	);

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[test]
fn test_load_from_path_trait_implementation() {
	// Setup temporary directory and files
	let temp_dir = TempDir::new().unwrap();
	let monitor_path = create_test_monitor_file(
		temp_dir.path(),
		"monitor",
		vec!["test-trigger"],
		vec!["ethereum_mainnet"],
	);

	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

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
		);

	assert!(result.is_ok());
	let monitor = result.unwrap();
	assert_eq!(monitor.name, "monitor");
	assert!(monitor.networks.contains(&"ethereum_mainnet".to_string()));
	assert!(monitor.triggers.contains(&"test-trigger".to_string()));
}

#[test]
fn test_load_from_path_trait_implementation_error() {
	// Setup temporary directory and files
	let mock_network_service =
		setup_mocked_networks("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

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
		);

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to load monitors"));
}

// This test is ignored because it creates files in the config directory
// and we don't want to pollute the default config directory
#[test]
#[cfg_attr(not(feature = "test-ci-only"), ignore)]
fn test_load_from_path_with_mixed_services() {
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
	let network_repo = NetworkRepository::new(Some(network_path.parent().unwrap())).unwrap();
	let network_service = NetworkService::new_with_repository(network_repo).unwrap();

	let trigger_path = create_test_trigger_file(&trigger_path, "integration_test_trigger");
	let trigger_repo = TriggerRepository::new(Some(trigger_path.parent().unwrap())).unwrap();
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
	let result = repository.load_from_path(Some(&monitor_path), None, None);
	assert!(result.is_ok());

	// Test 2: Empty monitor content
	let monitor_temp_dir = TempDir::new().unwrap();
	let result = repository.load_from_path(Some(monitor_temp_dir.path()), None, None);
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(matches!(err, RepositoryError::LoadError(_)));
	assert!(err.to_string().contains("Failed to load monitors"));

	// Test 3: Mixed service configuration
	let result =
		repository.load_from_path(Some(&monitor_path), Some(network_service.clone()), None);
	assert!(result.is_ok());

	let result =
		repository.load_from_path(Some(&monitor_path), None, Some(trigger_service.clone()));
	assert!(result.is_ok());

	// Test 4: Invalid monitor references
	let invalid_monitor_path = create_test_monitor_file(
		monitor_temp_dir.path(),
		"invalid_monitor",
		vec!["invalid-trigger"],
		vec!["integration_test_ethereum_mainnet"],
	);
	let result = repository.load_from_path(Some(&invalid_monitor_path), None, None);
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("references non-existent"));

	// Clean up after test
	// Remove integration_test_* files from config directory
	std::fs::remove_file(network_path).unwrap();
	std::fs::remove_file(trigger_path).unwrap();
	std::fs::remove_file(monitor_path).unwrap();
}
