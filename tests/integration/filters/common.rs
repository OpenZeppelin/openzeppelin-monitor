//! Common test utilities and helper functions.
//!
//! Provides shared functionality for loading test fixtures and setting up
//! test environments for both EVM and Stellar chain tests.

use alloy::json_abi::JsonAbi;
use openzeppelin_monitor::{
	models::{
		BlockType, ContractSpec, EVMContractSpec, EVMTransactionReceipt, Monitor, Network,
		StellarContractSpec, StellarEvent, StellarTransaction, Trigger,
	},
	repositories::{
		MonitorService, NetworkService, RepositoryError, TriggerRepositoryTrait, TriggerService,
	},
	services::notification::NotificationService,
};
use std::{collections::HashMap, fs};
use stellar_xdr::curr::ScSpecEntry;

use crate::integration::mocks::{
	MockMonitorRepository, MockNetworkRepository, MockTriggerExecutionService,
	MockTriggerRepository,
};

pub const TEST_FIXTURES_BASE: &str = "tests/integration/fixtures";

#[derive(Clone)]
pub struct TestData {
	pub blocks: Vec<BlockType>,
	pub monitor: Monitor,
	pub network: Network,
	pub receipts: Vec<EVMTransactionReceipt>,
	pub stellar_transactions: Vec<StellarTransaction>,
	pub stellar_events: Vec<StellarEvent>,
	pub contract_spec: Option<ContractSpec>,
}

pub struct TestDataBuilder {
	chain: String,
	monitor_file: Option<String>,
	network_file: Option<String>,
	blocks_file: Option<String>,
	contract_spec_file: Option<String>,
}

impl TestDataBuilder {
	pub fn new(chain: &str) -> Self {
		Self {
			chain: chain.to_string(),
			monitor_file: None,
			network_file: None,
			blocks_file: None,
			contract_spec_file: None,
		}
	}

	pub fn with_monitor(mut self, monitor_file: &str) -> Self {
		self.monitor_file = Some(monitor_file.to_string());
		self
	}

	pub fn with_contract_spec(mut self, contract_spec_file: &str) -> Self {
		self.contract_spec_file = Some(contract_spec_file.to_string());
		self
	}

	pub fn build(self) -> TestData {
		let base_path = format!("{}/{}", TEST_FIXTURES_BASE, self.chain);

		let blocks: Vec<BlockType> = read_and_parse_json(&format!(
			"{}/{}",
			base_path,
			self.blocks_file
				.unwrap_or_else(|| "blocks.json".to_string())
		));

		let monitor: Monitor = read_and_parse_json(&format!(
			"{}/monitors/{}",
			base_path,
			self.monitor_file
				.unwrap_or_else(|| "monitor.json".to_string())
		));

		let network: Network = read_and_parse_json(&format!(
			"{}/networks/{}",
			base_path,
			self.network_file
				.unwrap_or_else(|| "network.json".to_string())
		));

		let receipts: Vec<EVMTransactionReceipt> = if self.chain == "evm" {
			read_and_parse_json(&format!("{}/transaction_receipts.json", base_path))
		} else {
			Vec::new()
		};

		let stellar_transactions: Vec<StellarTransaction> = if self.chain == "stellar" {
			read_and_parse_json(&format!("{}/transactions.json", base_path))
		} else {
			Vec::new()
		};

		let stellar_events: Vec<StellarEvent> = if self.chain == "stellar" {
			read_and_parse_json(&format!("{}/events.json", base_path))
		} else {
			Vec::new()
		};

		let contract_spec: Option<ContractSpec> = if self.chain == "stellar" {
			Some(ContractSpec::Stellar(StellarContractSpec::from(
				read_and_parse_json::<Vec<ScSpecEntry>>(&format!(
					"{}/contract_specs/{}",
					base_path,
					self.contract_spec_file
						.unwrap_or_else(|| "contract_spec.json".to_string())
				)),
			)))
		} else {
			Some(ContractSpec::EVM(EVMContractSpec::from(
				read_and_parse_json::<JsonAbi>(&format!(
					"{}/contract_specs/{}",
					base_path,
					self.contract_spec_file
						.unwrap_or_else(|| "contract_spec.json".to_string())
				)),
			)))
		};

		TestData {
			blocks,
			monitor,
			network,
			receipts,
			stellar_transactions,
			stellar_events,
			contract_spec,
		}
	}
}

pub fn read_and_parse_json<T: serde::de::DeserializeOwned>(path: &str) -> T {
	let content =
		fs::read_to_string(path).unwrap_or_else(|_| panic!("Failed to read file: {}", path));
	serde_json::from_str(&content).unwrap_or_else(|_| panic!("Failed to parse JSON from: {}", path))
}

pub async fn setup_trigger_execution_service(
	trigger_json: &str,
) -> MockTriggerExecutionService<MockTriggerRepository> {
	let trigger_map: HashMap<String, Trigger> = read_and_parse_json(trigger_json);

	let triggers = trigger_map.clone();

	// mock_trigger_repository load all with triggers
	MockTriggerRepository::load_all_context()
		.expect()
		.return_once(move |_| Ok(triggers.clone()));

	let ctx = MockTriggerRepository::new_context();
	ctx.expect()
		.withf(|_| true)
		.returning(|_| Ok(MockTriggerRepository::default()));

	let mut mock_trigger_repository = MockTriggerRepository::new(None).await.unwrap();

	mock_trigger_repository
		.expect_get()
		.returning(move |id| trigger_map.get(id).cloned());

	let trigger_service = TriggerService::new_with_repository(mock_trigger_repository).unwrap();
	let notification_service = NotificationService::new();

	// Set up expectation for the constructor
	let ctx = MockTriggerExecutionService::<MockTriggerRepository>::new_context();
	ctx.expect()
		.with(mockall::predicate::always(), mockall::predicate::always())
		.returning(|_, _| MockTriggerExecutionService::default());

	// Then make the actual call that will match the expectation
	MockTriggerExecutionService::new(trigger_service, notification_service)
}

pub fn setup_network_service(
	networks: HashMap<String, Network>,
) -> NetworkService<MockNetworkRepository> {
	let networks_clone = networks.clone();
	MockNetworkRepository::load_all_context()
		.expect()
		.return_once(move |_| Ok(networks_clone.clone()));

	let mut mock_repo = MockNetworkRepository::default();

	let networks_clone = networks.clone();

	mock_repo
		.expect_get_all()
		.return_once(move || networks_clone.clone());

	mock_repo.expect_clone().return_once({
		let networks = networks.clone();
		move || {
			let mut cloned_repo = MockNetworkRepository::default();
			let networks_clone = networks.clone();
			cloned_repo.expect_get_all().return_once(|| networks_clone);
			cloned_repo
		}
	});

	mock_repo
		.expect_get()
		.return_once(move |id| networks.get(id).cloned());

	NetworkService::new_with_repository(mock_repo).unwrap()
}

pub fn setup_trigger_service(
	triggers: HashMap<String, Trigger>,
) -> TriggerService<MockTriggerRepository> {
	let triggers_clone = triggers.clone();
	MockTriggerRepository::load_all_context()
		.expect()
		.return_once(move |_| Ok(triggers_clone));

	let mut mock_repo = MockTriggerRepository::default();

	let triggers_clone = triggers.clone();
	let triggers_for_get = triggers.clone();

	mock_repo
		.expect_get_all()
		.return_once(move || triggers_clone.clone());

	// // Set up get() expectation
	mock_repo
		.expect_get()
		.returning(move |id| triggers_for_get.get(id).cloned());

	mock_repo.expect_clone().return_once(move || {
		let mut cloned_repo = MockTriggerRepository::default();
		let triggers_clone = triggers.clone();
		cloned_repo.expect_get_all().return_once(|| triggers_clone);
		cloned_repo
	});
	TriggerService::new_with_repository(mock_repo).unwrap()
}

pub fn setup_monitor_service(
	monitors: HashMap<String, Monitor>,
) -> MonitorService<
	MockMonitorRepository<MockNetworkRepository, MockTriggerRepository>,
	MockNetworkRepository,
	MockTriggerRepository,
> {
	let monitors_clone = monitors.clone();
	MockMonitorRepository::<MockNetworkRepository, MockTriggerRepository>::load_all_context()
		.expect()
		.return_once(move |_, _, _| Ok(monitors_clone));

	let mut mock_repo = MockMonitorRepository::default();

	let monitors_clone = monitors.clone();

	mock_repo
		.expect_get_all()
		.return_once(move || monitors_clone.clone());

	let monitors_for_load = monitors.clone();
	mock_repo
		.expect_load_from_path()
		.return_once(move |path, _, _| match path {
			Some(_) => Ok(monitors_for_load.get("monitor").unwrap().clone()),
			None => Err(RepositoryError::load_error(
				"Failed to load monitors",
				None,
				None,
			)),
		});

	mock_repo.expect_clone().return_once(move || {
		let mut cloned_repo = MockMonitorRepository::default();
		let monitors_clone = monitors.clone();
		cloned_repo.expect_get_all().return_once(|| monitors_clone);
		cloned_repo
	});
	MonitorService::<
		MockMonitorRepository<MockNetworkRepository, MockTriggerRepository>,
		MockNetworkRepository,
		MockTriggerRepository,
	>::new_with_repository(mock_repo)
	.unwrap()
}
