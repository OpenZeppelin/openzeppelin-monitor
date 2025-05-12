use crate::properties::strategies::network_strategy;

use openzeppelin_monitor::{
	models::{ConfigLoader, SecretString, SecretValue},
	repositories::{NetworkRepository, NetworkRepositoryTrait},
};
use proptest::{prelude::*, test_runner::Config};

const MIN_TEST_CASES: usize = 1;
const MAX_TEST_CASES: usize = 10;

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	// Data Consistency & Round-trip Tests
	#[test]
	fn test_roundtrip(
		networks in proptest::collection::hash_map(
			"[a-z0-9_]{1,10}",
			network_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	){
		// Simulate saving and reloading from a repository
		let repo = NetworkRepository { networks: networks.clone() };
		let reloaded_networks = repo.get_all();

		prop_assert_eq!(networks, reloaded_networks); // Ensure roundtrip consistency
	}

	// Query Operations Tests
	#[test]
	fn test_query_operations(
		networks in proptest::collection::hash_map(
			"[a-z0-9_]{1,10}",
			network_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let repo = NetworkRepository { networks: networks.clone() };

		// Test get by slug
		for (slug, network) in &networks {
			let retrieved = repo.get(slug);
			prop_assert_eq!(Some(network), retrieved.as_ref());
		}

		// Test get_all consistency
		let all_networks = repo.get_all();
		prop_assert_eq!(networks, all_networks);

		// Test non-existent slug
		prop_assert_eq!(None, repo.get("non_existent_slug"));
	}

	// Empty/Null Handling Tests
	#[test]
	fn test_empty_repository(
		_networks in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			network_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let empty_repo = NetworkRepository { networks: std::collections::HashMap::new() };
		// Test empty repository operations
		prop_assert!(empty_repo.get_all().is_empty());
		prop_assert_eq!(None, empty_repo.get("any_id"));
	}

	// Configuration Validation Tests
	#[test]
	fn test_config_validation(
		networks in proptest::collection::vec(
			network_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		for network in networks {
			// Valid network should pass validation
			prop_assert!(network.validate().is_ok());

			// Test invalid cases
			let mut invalid_network = network.clone();
			invalid_network.block_time_ms = 50; // Too low block time
			prop_assert!(invalid_network.validate().is_err());

			invalid_network = network.clone();
			invalid_network.confirmation_blocks = 0; // Invalid confirmation blocks
			prop_assert!(invalid_network.validate().is_err());

			invalid_network = network.clone();
			invalid_network.rpc_urls[0].url = SecretValue::Plain(SecretString::new("invalid-url".to_string())); // Invalid RPC URL
			prop_assert!(invalid_network.validate().is_err());

			invalid_network = network.clone();
			invalid_network.slug = "INVALID_SLUG".to_string(); // Invalid slug with uppercase
			prop_assert!(invalid_network.validate().is_err());

			invalid_network = network.clone();
			invalid_network.name = "".to_string(); // Empty name
			prop_assert!(invalid_network.validate().is_err());

			invalid_network = network.clone();
			invalid_network.cron_schedule = "0 */1 * * *".to_string(); // Invalid cron schedule
			prop_assert!(invalid_network.validate().is_err());
		}
	}
}
