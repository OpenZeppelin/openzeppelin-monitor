use openzeppelin_monitor::{
    models::{BlockType, Monitor, Network, Trigger},
    repositories::{TriggerRepositoryTrait, TriggerService},
    services::{notification::NotificationService, trigger::TriggerExecutionService},
};
use std::collections::HashMap;
use std::fs;

use crate::mocks::MockTriggerRepository;

pub const TEST_FIXTURES_BASE: &str = "tests/fixtures";

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

pub fn setup_trigger_execution_service<T: TriggerRepositoryTrait>(
    trigger_json: &str,
) -> TriggerExecutionService<MockTriggerRepository> {
    let trigger_map: HashMap<String, Trigger> = read_and_parse_json(trigger_json);
    let mut mock_trigger_repository = MockTriggerRepository::new();

    let triggers = trigger_map.clone();
    mock_trigger_repository
        .expect_load_all()
        .returning(move |_| Ok(triggers.clone()));

    mock_trigger_repository
        .expect_get()
        .returning(move |id| trigger_map.get(id).cloned());

    let trigger_service = TriggerService::new_with_repository(mock_trigger_repository).unwrap();
    let notification_service = NotificationService::new();

    TriggerExecutionService::new(trigger_service, notification_service)
}
