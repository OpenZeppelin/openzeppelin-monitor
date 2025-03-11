use crate::properties::strategies::{monitor_strategy, network_strategy, trigger_strategy};

use openzeppelin_monitor::{
	models::{ConfigLoader, ScriptLanguage},
	repositories::{
		MonitorRepository, MonitorRepositoryTrait, NetworkRepository, TriggerRepository,
	},
};
use prop::strategy::ValueTree;
use proptest::{prelude::*, test_runner::Config};

const MIN_TEST_CASES: usize = 1;
const MAX_TEST_CASES: usize = 10;

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_roundtrip(
		monitors in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			monitor_strategy(
				vec!["network1".to_string()],
				vec!["trigger1".to_string()]
			),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		// Simulate saving and reloading from a repository
		let repo = MonitorRepository::<NetworkRepository, TriggerRepository>::new_with_monitors(monitors.clone());
		let reloaded_monitors = repo.get_all();

		prop_assert_eq!(monitors, reloaded_monitors); // Ensure roundtrip consistency
	}

	#[test]
	fn test_reference_integrity(
		triggers in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			trigger_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		),
		networks in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			network_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		),
	) {
		let network_names: Vec<String> = networks.keys().cloned().collect();
		let trigger_names: Vec<String> = triggers.keys().cloned().collect();

		// Generate monitors with valid references
		let monitors = proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			monitor_strategy(network_names, trigger_names),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
		.new_tree(&mut proptest::test_runner::TestRunner::default())
		.unwrap()
		.current();

		// Test valid references
		let result = MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
			&monitors,
			&triggers,
			&networks,
		);
		prop_assert!(result.is_ok());

		// Test invalid references
		let mut invalid_monitors = monitors.clone();
		for monitor in invalid_monitors.values_mut() {
			monitor.triggers.push("non_existent_trigger".to_string());
			monitor.networks.push("non_existent_network".to_string());
		}

		let invalid_result = MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
			&invalid_monitors,
			&triggers,
			&networks,
		);
		prop_assert!(invalid_result.is_err());
	}

	// Query Operations Tests
	#[test]
	fn test_query_operations(
		monitors in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			monitor_strategy(
				vec!["network1".to_string()],
				vec!["trigger1".to_string()]
			),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let repo = MonitorRepository::<NetworkRepository, TriggerRepository>::new_with_monitors(monitors.clone());

		// Test get by ID
		for (id, monitor) in &monitors {
			let retrieved = repo.get(id);
			prop_assert_eq!(Some(monitor), retrieved.as_ref());
		}

		// Test get_all consistency
		let all_monitors = repo.get_all();
		prop_assert_eq!(monitors, all_monitors);

		// Test non-existent ID
		prop_assert_eq!(None, repo.get("non_existent_id"));
	}

	// Empty/Null Handling Tests
	#[test]
	fn test_empty_repository(
		_monitors in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			monitor_strategy(
				vec!["network1".to_string()],
				vec!["trigger1".to_string()]
			),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let empty_repo = MonitorRepository::<NetworkRepository, TriggerRepository>::new_with_monitors(std::collections::HashMap::new());

		// Test empty repository operations
		prop_assert!(empty_repo.get_all().is_empty());
		prop_assert_eq!(None, empty_repo.get("any_id"));
	}

	// Configuration Validation Tests
	#[test]
	fn test_config_validation(
		monitors in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			monitor_strategy(
				vec!["network1".to_string()],
				vec!["trigger1".to_string()]
			),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		// Validate each monitor configuration
		for monitor in monitors.values() {
			prop_assert!(monitor.validate().is_ok());

			let mut invalid_monitor = monitor.clone();

			// Test invalid monitor name
			invalid_monitor.name = "".to_string();
			prop_assert!(invalid_monitor.validate().is_err());

			// Test invalid function signature
			if let Some(func) = invalid_monitor.match_conditions.functions.first_mut() {
				func.signature = "invalid_signature".to_string(); // Missing parentheses
				prop_assert!(invalid_monitor.validate().is_err());
			}

			// Test invalid event signature
			invalid_monitor = monitor.clone();
			if let Some(event) = invalid_monitor.match_conditions.events.first_mut() {
				event.signature = "invalid_signature".to_string(); // Missing parentheses
				prop_assert!(invalid_monitor.validate().is_err());
			}

			// Test invalid script path
			invalid_monitor = monitor.clone();
			if let Some(condition) = invalid_monitor.trigger_conditions.first_mut() {
				condition.script_path = "invalid_path".to_string();
				prop_assert!(invalid_monitor.validate().is_err());
			}

			// Test invalid script extension
			invalid_monitor = monitor.clone();
			if let Some(condition) = invalid_monitor.trigger_conditions.first_mut() {
				// Test Python script with wrong extension
				condition.language = ScriptLanguage::Python;
				condition.script_path = "test_script.js".to_string();
				prop_assert!(invalid_monitor.validate().is_err());
			}

			invalid_monitor = monitor.clone();
			if let Some(condition) = invalid_monitor.trigger_conditions.first_mut() {
				condition.timeout_ms = 0;
				prop_assert!(invalid_monitor.validate().is_err());
			}
		}
	}
}
