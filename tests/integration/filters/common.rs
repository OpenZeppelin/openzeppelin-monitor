//! Common test utilities and helper functions.
//!
//! Provides shared functionality for loading test fixtures and setting up
//! test environments for both EVM and Stellar chain tests.

use openzeppelin_monitor::{
	models::{BlockType, Monitor, Network, Trigger},
	repositories::TriggerService,
	services::notification::NotificationService,
};
use std::{collections::HashMap, fs};

use crate::integration::mocks::{MockTriggerExecutionService, MockTriggerRepository};

pub const TEST_FIXTURES_BASE: &str = "tests/integration/fixtures";

pub struct TestData {
	pub blocks: Vec<BlockType>,
	pub monitor: Monitor,
	pub network: Network,
}

pub fn load_test_data(chain: &str) -> TestData {
	let base_path = format!("{}/{}", TEST_FIXTURES_BASE, chain);

	let blocks: Vec<BlockType> = read_and_parse_json(&format!("{}/blocks.json", base_path));
	let monitor: Monitor = read_and_parse_json(&format!("{}/monitors/monitor.json", base_path));
	let network: Network = read_and_parse_json(&format!("{}/networks/network.json", base_path));

	TestData {
		blocks,
		monitor,
		network,
	}
}

pub fn read_and_parse_json<T: serde::de::DeserializeOwned>(path: &str) -> T {
	let content =
		fs::read_to_string(path).unwrap_or_else(|_| panic!("Failed to read file: {}", path));
	serde_json::from_str(&content).unwrap_or_else(|_| panic!("Failed to parse JSON from: {}", path))
}

pub fn setup_trigger_execution_service(
	trigger_json: &str,
) -> MockTriggerExecutionService<MockTriggerRepository> {
	let trigger_map: HashMap<String, Trigger> = read_and_parse_json(trigger_json);
	let mut mock_trigger_repository = MockTriggerRepository::new();

	let triggers = trigger_map.clone();

	// mock_trigger_repository load all with triggers
	MockTriggerRepository::load_all_context()
		.expect()
		.return_once(move |_| Ok(triggers.clone()));

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
