//! Property-based tests for Solana filter functionality.

use openzeppelin_monitor::models::{
	AddressWithSpec, ChainConfiguration, EventCondition, FunctionCondition, MatchConditions,
	Monitor, SolanaMonitorConfig, TransactionCondition, TransactionStatus,
};
use openzeppelin_monitor::services::filter::solana_helpers::{
	are_same_address, encode_base58, normalize_address,
};
use proptest::{prelude::*, test_runner::Config};

// Strategy to generate valid Solana public keys (base58-encoded 32 bytes)
fn arb_solana_pubkey() -> impl Strategy<Value = String> {
	prop::collection::vec(any::<u8>(), 32).prop_map(|bytes| encode_base58(&bytes))
}

// Strategy to generate a list of addresses
fn arb_address_list(min: usize, max: usize) -> impl Strategy<Value = Vec<String>> {
	prop::collection::vec(arb_solana_pubkey(), min..=max)
}

// Helper to create a Solana monitor
fn create_solana_monitor(
	name: &str,
	addresses: Vec<String>,
	functions: Vec<FunctionCondition>,
	events: Vec<EventCondition>,
	transactions: Vec<TransactionCondition>,
) -> Monitor {
	Monitor {
		name: name.to_string(),
		networks: vec!["solana_mainnet".to_string()],
		paused: false,
		addresses: addresses
			.into_iter()
			.map(|addr| AddressWithSpec {
				address: addr,
				contract_spec: None,
			})
			.collect(),
		match_conditions: MatchConditions {
			functions,
			events,
			transactions,
		},
		trigger_conditions: vec![],
		triggers: vec![],
		chain_configurations: vec![ChainConfiguration {
			solana: Some(SolanaMonitorConfig::default()),
			..Default::default()
		}],
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn prop_monitor_address_normalized_matching(
		raw_address in arb_solana_pubkey(),
		leading_spaces in 0usize..3usize,
		trailing_spaces in 0usize..3usize
	) {
		let padded_address = format!(
			"{}{}{}",
			" ".repeat(leading_spaces),
			raw_address,
			" ".repeat(trailing_spaces)
		);

		let normalized = normalize_address(&padded_address);
		prop_assert_eq!(normalized, raw_address);
	}

	#[test]
	fn prop_monitor_address_list_contains_check(
		addresses in arb_address_list(1, 5),
		check_index in any::<usize>()
	) {
		if !addresses.is_empty() {
			let index = check_index % addresses.len();
			let target_address = &addresses[index];

			let found = addresses.iter().any(|addr| are_same_address(addr, target_address));
			prop_assert!(found);
		}
	}

	#[test]
	fn prop_monitor_creation_with_addresses(
		name in "[a-zA-Z][a-zA-Z0-9_]*",
		addresses in arb_address_list(1, 10)
	) {
		let monitor = create_solana_monitor(
			&name,
			addresses.clone(),
			vec![],
			vec![],
			vec![],
		);

		prop_assert_eq!(&monitor.name, &name);
		prop_assert_eq!(monitor.addresses.len(), addresses.len());
		prop_assert!(monitor.chain_configurations[0].solana.is_some());
	}

	#[test]
	fn prop_monitor_with_multiple_conditions(
		address in arb_solana_pubkey(),
		num_functions in 0usize..5usize,
		num_events in 0usize..5usize,
		num_transactions in 0usize..3usize
	) {
		let functions: Vec<FunctionCondition> = (0..num_functions)
			.map(|i| FunctionCondition {
				signature: format!("function_{}", i),
				expression: None,
			})
			.collect();

		let events: Vec<EventCondition> = (0..num_events)
			.map(|i| EventCondition {
				signature: format!("Event{}", i),
				expression: None,
			})
			.collect();

		let transactions: Vec<TransactionCondition> = (0..num_transactions)
			.map(|_| TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			})
			.collect();

		let monitor = create_solana_monitor(
			"test_monitor",
			vec![address],
			functions.clone(),
			events.clone(),
			transactions.clone(),
		);

		prop_assert_eq!(monitor.match_conditions.functions.len(), num_functions);
		prop_assert_eq!(monitor.match_conditions.events.len(), num_events);
		prop_assert_eq!(monitor.match_conditions.transactions.len(), num_transactions);
	}

	#[test]
	fn prop_monitor_paused_state(
		address in arb_solana_pubkey(),
		paused in any::<bool>()
	) {
		let mut monitor = create_solana_monitor(
			"test_monitor",
			vec![address],
			vec![],
			vec![],
			vec![],
		);
		monitor.paused = paused;

		prop_assert_eq!(monitor.paused, paused);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_monitor_with_token_program() {
		let token_program = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let monitor = create_solana_monitor(
			"token_monitor",
			vec![token_program.to_string()],
			vec![FunctionCondition {
				signature: "transfer".to_string(),
				expression: Some("amount > 1000000".to_string()),
			}],
			vec![],
			vec![],
		);

		assert_eq!(monitor.addresses[0].address, token_program);
		assert_eq!(monitor.match_conditions.functions[0].signature, "transfer");
	}

	#[test]
	fn test_monitor_with_system_program() {
		let system_program = "11111111111111111111111111111111";
		let monitor = create_solana_monitor(
			"system_monitor",
			vec![system_program.to_string()],
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			}],
		);

		assert_eq!(monitor.addresses[0].address, system_program);
		assert_eq!(
			monitor.match_conditions.transactions[0].status,
			TransactionStatus::Success
		);
	}

	#[test]
	fn test_empty_monitor_conditions() {
		let monitor = create_solana_monitor(
			"empty_monitor",
			vec!["SomeRandomPubkey1111111111111111111111111".to_string()],
			vec![],
			vec![],
			vec![],
		);

		assert!(monitor.match_conditions.functions.is_empty());
		assert!(monitor.match_conditions.events.is_empty());
		assert!(monitor.match_conditions.transactions.is_empty());
	}
}
