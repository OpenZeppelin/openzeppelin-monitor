//! Integration tests for EVM chain monitoring.
//!
//! Tests the monitoring functionality for EVM-compatible blockchains,
//! including event and transaction filtering.

use alloy::primitives::Uint;
use serde_json::json;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{
		BlockType, ContractSpec, EVMReceiptLog, EVMTransactionReceipt, EventCondition,
		FunctionCondition, Monitor, MonitorMatch, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::{EvmClient, TransportError},
		filter::{handle_match, FilterError, FilterService},
	},
	utils::tests::evm::{receipt::ReceiptBuilder, transaction::TransactionBuilder},
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
		.returning(move |method, _params| {
			let current = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			match (method, current) {
				("net_version", _) => Ok(json!({"result": "1"})),
				("eth_getTransactionReceipt", i) => Ok(json!({
					"result": &receipts[i]
				})),
				("eth_getLogs", i) => Ok(json!({
					"result": &receipts[i].logs
				})),
				_ => Err(TransportError::http(
					reqwest::StatusCode::METHOD_NOT_ALLOWED,
					"random.url".to_string(),
					"Unexpected method call".to_string(),
					None,
					None,
				)),
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
			None,
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
			None,
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

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
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

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
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
			None,
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
			None,
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

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
			Some(&[contract_with_spec]),
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
			None,
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
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json")
			.await;

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	// Set up expectations for execute()
	trigger_execution_service.expect_execute()
		.withf(|trigger_name, variables, _monitor_match, _trigger_scripts| {
			trigger_name == ["example_trigger_slack"]
				// Event variables
				&& variables.get("events.0.signature") == Some(&"Transfer(address,address,uint256)".to_string())
				&& variables.get("events.0.args.from") == Some(&"0x58b704065b7aff3ed351052f8560019e05925023".to_string())
				&& variables.get("events.0.args.to") == Some(&"0xf423d9c1ffeb6386639d024f3b241dab2331b635".to_string())
				&& variables.get("events.0.args.value") == Some(&"8181710000".to_string())
				// Function variables
				&& variables.get("functions.0.signature") == Some(&"transfer(address,uint256)".to_string())
				&& variables.get("functions.0.args.to") == Some(&"0xf423d9c1ffeb6386639d024f3b241dab2331b635".to_string())
				&& variables.get("functions.0.args.value") == Some(&"8181710000".to_string())
				// Transaction variables
				&& variables.get("transaction.hash") == Some(&"0xd5069b22a3a89a36d592d5a1f72a281bc5d11d6d0bac6f0a878c13abb764b6d8".to_string())
				&& variables.get("transaction.from") == Some(&"0x58b704065b7aff3ed351052f8560019e05925023".to_string())
				&& variables.get("transaction.to") == Some(&"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string())
				&& variables.get("transaction.value") == Some(&"0".to_string())
				// Monitor metadata
				&& variables.get("monitor.name") == Some(&"Mint USDC Token".to_string())
		})
		.once()
		.returning(|_, _, _, _| Ok(()));

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
			Some(&[contract_with_spec]),
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

#[tokio::test]
async fn test_handle_match_with_no_args() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let mut test_data = load_test_data("evm");
	let filter_service = FilterService::new();

	// only keep the last receipt with increment() transaction
	test_data.receipts = vec![test_data.receipts.last().unwrap().clone()];

	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());
	let client = EvmClient::new_with_transport(mock_transport);

	let mut monitor = test_data.monitor;
	// Clear existing conditions and add functions without arguments
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "increment()".to_string(),
		expression: None,
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xf18206b2289cf6ce15cddbee9c6f6a0f059efb56".to_string(),
		contract_spec.clone(),
	);

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			test_data.blocks.last().unwrap(), // last block contains increment() transaction
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(evm_match.matched_on.functions.len() == 1);
			assert!(evm_match.matched_on.events.is_empty());
			assert!(evm_match.matched_on.transactions.is_empty());
			assert!(evm_match.matched_on.functions[0].signature == "increment()");

			// Now test handle_match to verify the data map contains signatures
			let trigger_scripts = HashMap::new();
			let mut trigger_execution_service = setup_trigger_execution_service(
				"tests/integration/fixtures/evm/triggers/trigger.json",
			)
			.await;

			// Set up expectations for execute()
			trigger_execution_service
				.expect_execute()
				.withf(|trigger_name, variables, _monitor_match, _trigger_scripts| {
					trigger_name == ["example_trigger_slack"]
						// Monitor metadata
						&& variables.get("monitor.name") == Some(&"Mint USDC Token".to_string())
						// Transaction variables
						&& variables.get("transaction.hash") == Some(&"0x6fb716f3fc4e2edec31f01c8bb67e565e3efacba965090a38835d3f297232bf6".to_string())
						&& variables.get("transaction.from") == Some(&"0x6b9501462d48f7e78ba11c98508ee16d29a03411".to_string())
						&& variables.get("transaction.to") == Some(&"0xf18206b2289cf6ce15cddbee9c6f6a0f059efb56".to_string())
						&& variables.get("transaction.value") == Some(&"0".to_string())
						// Function signature should be present even without args
						&& variables.get("functions.0.signature") == Some(&"increment()".to_string())
				})
				.once()
				.returning(|_, _, _, _| Ok(()));

			let result = handle_match(
				matches[0].clone(),
				&trigger_execution_service,
				&trigger_scripts,
			)
			.await;
			assert!(result.is_ok(), "Handle match should succeed");
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_handle_match_with_key_collision() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");

	// Setup trigger execution service and capture the data structure
	let data_capture = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
	let data_capture_clone = data_capture.clone();

	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json")
			.await;

	// Set up expectations for execute() with custom function to capture and verify data
	trigger_execution_service
		.expect_execute()
		.withf(
			move |_triggers, variables, _monitor_match, _trigger_scripts| {
				let mut captured = data_capture_clone.lock().unwrap();
				*captured = variables.clone();
				true
			},
		)
		.returning(|_, _, _, _| Ok(()));

	// Create a monitor match with an argument named "signature"
	use openzeppelin_monitor::models::{
		EVMMatchArguments, EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorMatch,
		FunctionCondition, MatchConditions,
	};

	// Create test monitor with a function that has an argument called "signature"
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "dangerousFunc(bytes32 signature, uint256 value)".to_string(),
		expression: None,
	}];

	fn create_test_evm_transaction_receipt() -> EVMTransactionReceipt {
		ReceiptBuilder::new().build()
	}

	fn create_test_evm_logs() -> Vec<EVMReceiptLog> {
		ReceiptBuilder::new().build().logs.clone()
	}

	// Create a match object
	let evm_match = EVMMonitorMatch {
		monitor,
		transaction: TransactionBuilder::new().build(),
		receipt: Some(create_test_evm_transaction_receipt()),
		logs: Some(create_test_evm_logs()),
		network_slug: "ethereum_mainnet".to_string(),
		matched_on: MatchConditions {
			functions: vec![FunctionCondition {
				signature: "dangerousFunc(bytes32 signature, uint256 value)".to_string(),
				expression: None,
			}],
			events: vec![],
			transactions: vec![],
		},
		matched_on_args: Some(EVMMatchArguments {
			functions: Some(vec![EVMMatchParamsMap {
				signature: "dangerousFunc(bytes32 signature, uint256 value)".to_string(),
				args: Some(vec![
					EVMMatchParamEntry {
						name: "signature".to_string(),
						value: "0xabcdef1234567890".to_string(),
						kind: "bytes32".to_string(),
						indexed: false,
					},
					EVMMatchParamEntry {
						name: "value".to_string(),
						value: "123456789".to_string(),
						kind: "uint256".to_string(),
						indexed: false,
					},
				]),
				hex_signature: Some("0xdeadbeef".to_string()),
			}]),
			events: None,
		}),
	};

	let match_wrapper = MonitorMatch::EVM(Box::new(evm_match));

	// Process the match directly using handle_match
	let result = handle_match(match_wrapper, &trigger_execution_service, &HashMap::new()).await;
	assert!(result.is_ok(), "Handle match should succeed");

	// Verify that data structure preserves both function signature and argument
	let captured_data = data_capture.lock().unwrap();

	// The key for the function signature should exist
	assert!(
		captured_data.contains_key("functions.0.signature"),
		"functions.0.signature should exist in the data structure"
	);

	// Check the value is correct
	assert_eq!(
		captured_data.get("functions.0.signature").unwrap(),
		"dangerousFunc(bytes32 signature, uint256 value)",
		"Function signature value should be preserved"
	);

	// The key for the argument should also exist
	assert!(
		captured_data.contains_key("functions.0.args.signature"),
		"functions.0.args.signature should exist in the data structure"
	);

	// Check that the argument value is correct
	assert_eq!(
		captured_data.get("functions.0.args.signature").unwrap(),
		"0xabcdef1234567890",
		"Function argument value should be correct"
	);

	// Verify that the values are different - no collision
	assert_ne!(
		captured_data.get("functions.0.signature").unwrap(),
		captured_data.get("functions.0.args.signature").unwrap(),
		"Function signature and argument values should be distinct"
	);

	// Also check for other expected fields
	assert!(
		captured_data.contains_key("transaction.hash"),
		"Transaction hash should be present"
	);
	assert!(
		captured_data.contains_key("monitor.name"),
		"Monitor name should be present"
	);

	Ok(())
}

#[tokio::test]
async fn test_filter_block_with_receipt_and_logs() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	let mock_transport = setup_mock_transport(test_data.clone());

	let client = EvmClient::new_with_transport(mock_transport);

	let mut monitor = test_data.monitor;
	monitor.match_conditions.events = vec![EventCondition {
		signature: "Transfer(address,address,uint256)".to_string(),
		expression: None,
	}];
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "transfer(address,uint256)".to_string(),
		expression: None,
	}];
	monitor.match_conditions.transactions = vec![TransactionCondition {
		status: TransactionStatus::Any,
		expression: Some("gas_used > 0".to_string()), // This is a test to ensure that the receipt is required
	}];

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(
				evm_match.receipt.is_some(),
				"Transaction receipt should be present"
			);
			let receipt = evm_match.receipt.as_ref().unwrap();
			assert!(
				receipt.status.is_some(),
				"Transaction status should be present"
			);
			assert_eq!(
				receipt.status.unwrap(),
				Uint::from(1),
				"Transaction should be successful"
			);
			assert!(receipt.gas_used.is_some(), "Gas used should be present");
			assert!(
				receipt.gas_used.unwrap() > Uint::from(0),
				"Gas used should be greater than 0"
			);

			assert!(
				evm_match.logs.is_some(),
				"Transaction logs should be present"
			);
			let logs = evm_match.logs.as_ref().unwrap();
			assert!(!logs.is_empty(), "Should have at least one log entry");

			assert!(
				!evm_match.matched_on.events.is_empty(),
				"Should have matched events"
			);
			assert!(
				!evm_match.matched_on.functions.is_empty(),
				"Should have matched functions"
			);

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();

			let event_args = &matched_on_args.events.as_ref().unwrap()[0];
			assert_eq!(event_args.signature, "Transfer(address,address,uint256)");
			assert_eq!(
				event_args.hex_signature.as_ref().unwrap(),
				"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
			);

			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];
			assert_eq!(function_args.signature, "transfer(address,uint256)");
			assert_eq!(function_args.hex_signature.as_ref().unwrap(), "a9059cbb");
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_filter_block_with_only_logs() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	// Create mock transport
	let mock_transport = setup_mock_transport(test_data.clone());

	let client = EvmClient::new_with_transport(mock_transport);

	let mut monitor = test_data.monitor;
	monitor.match_conditions.events = vec![EventCondition {
		signature: "Transfer(address,address,uint256)".to_string(),
		expression: None,
	}];
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "transfer(address,uint256)".to_string(),
		expression: None,
	}];
	monitor.match_conditions.transactions = vec![]; // This ensures we do not need a receipt

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(
				evm_match.receipt.is_none(),
				"Transaction receipt should not be present"
			);

			assert!(
				evm_match.logs.is_some(),
				"Transaction logs should be present"
			);
			let logs = evm_match.logs.as_ref().unwrap();
			assert!(!logs.is_empty(), "Should have at least one log entry");

			assert!(
				!evm_match.matched_on.events.is_empty(),
				"Should have matched events"
			);
			assert!(
				!evm_match.matched_on.functions.is_empty(),
				"Should have matched functions"
			);

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();

			let event_args = &matched_on_args.events.as_ref().unwrap()[0];
			assert_eq!(event_args.signature, "Transfer(address,address,uint256)");
			assert_eq!(
				event_args.hex_signature.as_ref().unwrap(),
				"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
			);

			// Check function arguments
			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];
			assert_eq!(function_args.signature, "transfer(address,uint256)");
			assert_eq!(function_args.hex_signature.as_ref().unwrap(), "a9059cbb");
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_filter_block_needs_receipt_for_status() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();

	let mut mock_transport = MockEVMTransportClient::new();
	let counter = std::sync::atomic::AtomicUsize::new(0);
	let receipts = test_data.receipts;

	mock_transport
		.expect_send_raw_request()
		.returning(move |method, _params| {
			let current = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			match (method, current) {
				("net_version", _) => Ok(json!({"result": "1"})),
				("eth_getTransactionReceipt", i) => Ok(json!({
					"result": &receipts[i]
				})),
				("eth_getLogs", _) => Ok(json!({
					"result": Vec::<EVMReceiptLog>::new()
				})),
				_ => Err(TransportError::http(
					reqwest::StatusCode::METHOD_NOT_ALLOWED,
					"random.url".to_string(),
					"Unexpected method call".to_string(),
					None,
					None,
				)),
			}
		});

	let client = EvmClient::new_with_transport(mock_transport);

	let mut monitor = test_data.monitor;
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "transfer(address,uint256)".to_string(),
		expression: None,
	}];
	monitor.match_conditions.transactions = vec![TransactionCondition {
		status: TransactionStatus::Success, // This is to ensure that the receipt is required
		expression: None,
	}];

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48".to_string(),
		contract_spec.clone(),
	);

	let matches = filter_service
		.filter_block(
			&client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::EVM(evm_match) => {
			assert!(
				evm_match.receipt.is_some(),
				"Transaction receipt should be present"
			);
			let receipt = evm_match.receipt.as_ref().unwrap();
			assert!(
				receipt.status.is_some(),
				"Transaction status should be present"
			);
			assert_eq!(
				receipt.status.unwrap(),
				Uint::from(1),
				"Transaction should be successful"
			);
			assert!(receipt.gas_used.is_some(), "Gas used should be present");
			assert!(
				receipt.gas_used.unwrap() > Uint::from(0),
				"Gas used should be greater than 0"
			);

			assert!(
				evm_match.logs.as_ref().unwrap().is_empty(),
				"Transaction logs should be empty"
			);

			assert!(
				evm_match.matched_on.events.is_empty(),
				"Should not have matched events"
			);
			assert!(
				!evm_match.matched_on.functions.is_empty(),
				"Should have matched functions"
			);

			let matched_on_args = evm_match.matched_on_args.as_ref().unwrap();

			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];
			assert_eq!(function_args.signature, "transfer(address,uint256)");
			assert_eq!(function_args.hex_signature.as_ref().unwrap(), "a9059cbb");
		}
		_ => {
			panic!("Expected EVM match");
		}
	}

	Ok(())
}
