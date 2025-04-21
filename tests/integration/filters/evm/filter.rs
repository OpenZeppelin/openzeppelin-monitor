//! Integration tests for EVM chain monitoring.
//!
//! Tests the monitoring functionality for EVM-compatible blockchains,
//! including event and transaction filtering.

use serde_json::json;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{
		BlockType, EventCondition, FunctionCondition, Monitor, MonitorMatch, TransactionCondition,
		TransactionStatus,
	},
	services::{
		blockchain::EvmClient,
		filter::{handle_match, FilterError, FilterService},
	},
};

use crate::integration::{
	filters::common::{load_test_data, setup_trigger_execution_service, TestData},
	mocks::MockEVMTransportClient,
};

fn setup_mock_transport(test_data: TestData) -> MockEVMTransportClient {
	let mut mock_transport = MockEVMTransportClient::new();
	let counter = std::sync::atomic::AtomicUsize::new(0);
	let receipts = test_data.receipts;

	mock_transport
		.expect_send_raw_request()
		.times(3)
		.returning(move |method, _params| {
			let current = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			match (method, current) {
				("net_version", _) => Ok(json!({"result": "1"})),
				("eth_getTransactionReceipt", i) => Ok(json!({
					"result": &receipts[i]
				})),
				_ => Err(anyhow::anyhow!("Unexpected method call")),
			}
		});

	mock_transport
}

fn make_monitor_with_events(mut monitor: Monitor, include_expression: bool) -> Monitor {
	monitor.match_conditions.functions = vec![];
	monitor.match_conditions.transactions = vec![];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.events.push(EventCondition {
		signature: "Transfer(address,address,uint256)".to_string(),
		expression: if include_expression {
			Some(
				"to == 0xf423d9c1ffeb6386639d024f3b241dab2331b635 AND from == \
				 0x58b704065b7aff3ed351052f8560019e05925023 AND value > 8000000000"
					.to_string(),
			)
		} else {
			None
		},
	});
	monitor
}

fn make_monitor_with_functions(mut monitor: Monitor, include_expression: bool) -> Monitor {
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];
	monitor.match_conditions.functions = vec![];
	monitor.match_conditions.functions.push(FunctionCondition {
		signature: "transfer(address,uint256)".to_string(),
		expression: if include_expression {
			Some("value > 0".to_string())
		} else {
			None
		},
	});
	monitor
}

fn make_monitor_with_transactions(mut monitor: Monitor, include_expression: bool) -> Monitor {
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.functions = vec![];
	monitor.match_conditions.transactions = vec![];
	monitor
		.match_conditions
		.transactions
		.push(TransactionCondition {
			status: TransactionStatus::Success,
			expression: if include_expression {
				Some("value == 0".to_string())
			} else {
				None
			},
		});
	monitor
}

#[tokio::test]
async fn test_monitor_events_with_no_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_events(test_data.monitor, false);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching events");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.events.len() == 1);
			assert!(evm_match.matched_on.functions.is_empty());
			assert!(evm_match.matched_on.transactions.is_empty());
			assert!(
				evm_match.matched_on.events[0].signature == "Transfer(address,address,uint256)"
			);

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();
			assert!(
				!matched_on_args.events.as_ref().unwrap().is_empty(),
				"Expected events arguments to be matched"
			);
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_events_with_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_events(test_data.monitor, true);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching events");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.events.len() == 1);
			assert!(evm_match.matched_on.functions.is_empty());
			assert!(evm_match.matched_on.transactions.is_empty());
			assert!(
				evm_match.matched_on.events[0].signature == "Transfer(address,address,uint256)"
			);

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();
			let event_args = &matched_on_args.events.as_ref().unwrap()[0];

			assert_eq!(event_args.signature, "Transfer(address,address,uint256)");
			assert_eq!(
				event_args.hex_signature.as_ref().unwrap(),
				"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
			);

			// Assert the argument values
			let args = event_args.args.as_ref().unwrap();
			assert_eq!(args[0].name, "from");
			assert_eq!(args[0].value, "0x58b704065b7aff3ed351052f8560019e05925023");
			assert_eq!(args[0].kind, "address");
			assert!(args[0].indexed);

			assert_eq!(args[1].name, "to");
			assert_eq!(args[1].value, "0xf423d9c1ffeb6386639d024f3b241dab2331b635");
			assert_eq!(args[1].kind, "address");
			assert!(args[1].indexed);

			assert_eq!(args[2].name, "value");
			assert_eq!(args[2].value, "8181710000");
			assert_eq!(args[2].kind, "uint256");
			assert!(!args[2].indexed);
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_no_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_functions(test_data.monitor, false);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.functions.len() == 1);
			assert!(evm_match.matched_on.events.is_empty());
			assert!(evm_match.matched_on.transactions.is_empty());
			assert!(evm_match.matched_on.functions[0].signature == "transfer(address,uint256)");

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();
			assert!(
				!matched_on_args.functions.as_ref().unwrap().is_empty(),
				"Expected functions arguments to be matched"
			);
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_functions(test_data.monitor, true);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.functions.len() == 1);
			assert!(evm_match.matched_on.events.is_empty());
			assert!(evm_match.matched_on.transactions.is_empty());
			assert!(evm_match.matched_on.functions[0].signature == "transfer(address,uint256)");

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();
			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];

			assert_eq!(function_args.signature, "transfer(address,uint256)");
			assert_eq!(function_args.hex_signature.as_ref().unwrap(), "0xa9059cbb");

			// Assert the argument values
			let args = function_args.args.as_ref().unwrap();

			assert_eq!(args[0].name, "to");
			assert_eq!(args[0].value, "0xf423d9c1ffeb6386639d024f3b241dab2331b635");
			assert_eq!(args[0].kind, "address");
			assert!(!args[0].indexed);

			assert_eq!(args[1].name, "value");
			assert_eq!(args[1].value, "8181710000");
			assert_eq!(args[1].kind, "uint256");
			assert!(!args[1].indexed);
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transactions_with_no_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_transactions(test_data.monitor, false);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching transactions"
	);
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.transactions.len() == 1);
			assert!(evm_match.matched_on.events.is_empty());
			assert!(evm_match.matched_on.functions.is_empty());
			assert!(evm_match.matched_on.transactions[0].status == TransactionStatus::Success);
			assert!(evm_match.matched_on.transactions[0].expression.is_none());
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transactions_with_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let monitor = make_monitor_with_transactions(test_data.monitor, true);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching transactions"
	);
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.transactions.len() == 1);
			assert!(evm_match.matched_on.events.is_empty());
			assert!(evm_match.matched_on.functions.is_empty());
			assert!(evm_match.matched_on.transactions[0].status == TransactionStatus::Success);
			assert!(
				evm_match.matched_on.transactions[0].expression == Some("value == 0".to_string())
			);
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_with_multiple_conditions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
		)
		.await?;

	assert!(!matches.is_empty());

	if let MonitorMatch::EVM(evm_match) = &matches[0] {
		assert!(
			!evm_match.matched_on.events.is_empty(),
			"Should have matched events"
		);
		assert!(
			!evm_match.matched_on.functions.is_empty(),
			"Should have matched functions"
		);

		assert!(
			!evm_match.matched_on.transactions.is_empty(),
			"Should have matched transactions"
		);

		if let Some(args) = &evm_match.matched_on_args {
			if let Some(events) = &args.events {
				assert!(!events.is_empty(), "Should have event arguments");
				let event = &events[0];
				assert_eq!(event.signature, "Transfer(address,address,uint256)");
				assert_eq!(
					event.hex_signature.as_ref().unwrap(),
					"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
				);
			}

			if let Some(functions) = &args.functions {
				assert!(!functions.is_empty(), "Should have function arguments");
				let function = &functions[0];
				assert_eq!(function.signature, "transfer(address,uint256)");
				assert_eq!(function.hex_signature.as_ref().unwrap(), "0xa9059cbb");
			}
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_error_cases() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	let client = EvmClient::new(&test_data.network).await.unwrap();

	// Create an invalid block type
	let invalid_block = BlockType::Stellar(Box::default());

	let result = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&invalid_block,
			&[test_data.monitor],
		)
		.await;

	assert!(result.is_err());
	assert!(matches!(
		result.unwrap_err(),
		FilterError::BlockTypeMismatch { .. }
	));

	Ok(())
}

#[tokio::test]
async fn test_handle_match() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);
	let trigger_scripts = HashMap::new();

	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

	// Set up expectations for execute()
	trigger_execution_service.expect_execute()
		.withf(|trigger_name, variables, _monitor_match, _trigger_scripts| {
			trigger_name == ["example_trigger_slack"]
				// Event variables
				&& variables.get("event_0_signature") == Some(&"Transfer(address,address,uint256)".to_string())
				&& variables.get("event_0_from") == Some(&"0x58b704065b7aff3ed351052f8560019e05925023".to_string())
				&& variables.get("event_0_to") == Some(&"0xf423d9c1ffeb6386639d024f3b241dab2331b635".to_string())
				&& variables.get("event_0_value") == Some(&"8181710000".to_string())
				// Function variables
				&& variables.get("function_0_signature") == Some(&"transfer(address,uint256)".to_string())
				&& variables.get("function_0_to") == Some(&"0xf423d9c1ffeb6386639d024f3b241dab2331b635".to_string())
				&& variables.get("function_0_value") == Some(&"8181710000".to_string())
				// Transaction variables
				&& variables.get("transaction_hash") == Some(&"0xd5069b22a3a89a36d592d5a1f72a281bc5d11d6d0bac6f0a878c13abb764b6d8".to_string())
				&& variables.get("transaction_from") == Some(&"0x58b704065b7aff3ed351052f8560019e05925023".to_string())
				&& variables.get("transaction_to") == Some(&"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string())
				&& variables.get("transaction_value") == Some(&"0".to_string())
				// Monitor metadata
				&& variables.get("monitor_name") == Some(&"Mint USDC Token".to_string())
		})
		.once()
		.returning(|_, _, _, _| Ok(()));

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches to handle");

	for matching_monitor in matches {
		let result = handle_match(
			matching_monitor.clone(),
			&trigger_execution_service,
			&trigger_scripts,
		)
		.await;
		assert!(result.is_ok(), "Handle match should succeed");
	}

	Ok(())
}
