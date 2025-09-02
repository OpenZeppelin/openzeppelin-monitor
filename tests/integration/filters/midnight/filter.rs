//! Integration tests for Midnight chain monitoring.
//!
//! Tests the monitoring functionality for Midnight-compatible blockchains.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use openzeppelin_monitor::{
	models::{BlockChainType, BlockType, MonitorMatch, TransactionStatus},
	services::filter::{handle_match, FilterError, FilterService},
	utils::tests::{
		midnight::{
			block::BlockBuilder, event::EventBuilder, monitor::MonitorBuilder,
			transaction::TransactionBuilder,
		},
		network::NetworkBuilder,
	},
};

use crate::integration::{
	filters::common::setup_trigger_execution_service,
	mocks::{MockMidnightClientTrait, MockMidnightWsTransportClient},
};

// Helper function to check function signatures in a match
fn check_function_signature(midnight_match: &MonitorMatch, expected_sig: &str) {
	match midnight_match {
		MonitorMatch::Midnight(midnight_match) => {
			assert_eq!(midnight_match.matched_on.functions.len(), 1);
			let sigs: Vec<_> = midnight_match
				.matched_on
				.functions
				.iter()
				.map(|f| f.signature.as_str())
				.collect();
			assert!(sigs.contains(&expected_sig));
		}
		_ => panic!("Expected Midnight match"),
	}
}

#[tokio::test]
async fn test_monitor_functions_with_no_expressions() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();

	// Create a monitor that watches for a specific function call
	let monitor = MonitorBuilder::new()
		.name("Test Function Monitor")
		.address("020200bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919")
		.function("main()", None)
		.build();

	// Create a test transaction with a function call
	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"020200bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919".to_string(),
			"main".to_string(),
		)
		.build();

	// Create a block containing our transaction
	let block = BlockBuilder::new()
		.number(1)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Set up expectations for the mock client
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.functions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.transactions.is_empty());
			assert!(midnight_match.matched_on.functions[0].signature == "main");

			let matched_on_args = midnight_match.matched_on_args.as_ref().unwrap();
			assert!(
				matched_on_args.functions.as_ref().unwrap().len() == 1,
				"Expected one function match"
			);
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_expressions() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();

	// Create a monitor that watches for a function call with an expression
	let monitor = MonitorBuilder::new()
		.name("Test Function Monitor With Expression")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("main(amount)", Some("amount >= 1000".to_string()))
		.build();

	// Create a test transaction with a function call and argument that matches the expression
	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main(amount)".to_string(),
		)
		.build();

	// Create a block containing our transaction
	let block = BlockBuilder::new()
		.number(2)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching functions with expression"
	);
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.functions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.transactions.is_empty());
			assert_eq!(midnight_match.matched_on.functions[0].signature, "main");
			assert_eq!(
				midnight_match.matched_on.functions[0].expression.as_deref(),
				Some("amount >= 1000")
			);

			let matched_on_args = midnight_match.matched_on_args.as_ref().unwrap();
			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];
			assert_eq!(function_args.signature, "main");
			assert!(function_args.args.is_none());
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_with_multiple_conditions() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();

	// Create a monitor that watches for two different function calls
	let monitor = MonitorBuilder::new()
		.name("Test Multi-Function Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("main(amount)", Some("amount >= 1000".to_string()))
		.function("secondary(note)", None)
		.build();

	// Create two transactions, one for each function
	let tx1 = TransactionBuilder::new()
		.hash("0x1000000000000000000000000000000000000000000000000000000000000000".to_string())
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main(amount)".to_string(),
		)
		.build();
	let tx2 = TransactionBuilder::new()
		.hash("0x2000000000000000000000000000000000000000000000000000000000000000".to_string())
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"secondary(note)".to_string(),
		)
		.build();

	// Create a block containing both transactions
	let block = BlockBuilder::new()
		.number(3)
		.add_rpc_transaction(tx1.into())
		.add_rpc_transaction(tx2.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matches for multiple functions"
	);

	// We should expect 2 matches because we are not processing function arguments/expressions since these are private
	assert_eq!(matches.len(), 2, "Expected exactly two matches");

	// Check both matches
	check_function_signature(&matches[0], "main");
	check_function_signature(&matches[1], "secondary");

	Ok(())
}

#[tokio::test]
async fn test_monitor_error_cases() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();

	let monitor = MonitorBuilder::new()
		.name("Test Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("main(uint64 amount)", None)
		.build();

	let mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Create an invalid block type (e.g., EVM block for Midnight monitor)
	let invalid_block = BlockType::EVM(Box::default());

	let result = filter_service
		.filter_block(
			&mock_client,
			&network,
			&invalid_block,
			&[monitor.clone()],
			None,
		)
		.await;

	assert!(
		result.is_err() || result.as_ref().map(|v| v.is_empty()).unwrap_or(false),
		"Should return error or empty result for invalid block type"
	);
	Ok(())
}

#[tokio::test]
async fn test_handle_match() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();
	let trigger_scripts = HashMap::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();

	// Create a monitor and a matching transaction
	let monitor = MonitorBuilder::new()
		.name("Test Handle Match Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("main(amount)", Some("amount >= 1000".to_string()))
		.build();

	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main(amount)".to_string(),
		)
		.build();

	// Create a block containing our transaction
	let block = BlockBuilder::new()
		.number(4)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	// Set up a mock trigger execution service
	let mut trigger_execution_service = setup_trigger_execution_service(
		"tests/integration/fixtures/midnight/triggers/trigger.json",
	)
	.await;
	trigger_execution_service
		.expect_execute()
		.returning(|_, _, _, _| Ok(()));

	// Process the match using handle_match
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
	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();
	let filter_service = FilterService::new();
	let trigger_scripts = HashMap::new();

	// Create a monitor for a function with no arguments
	let monitor = MonitorBuilder::new()
		.name("Test No-Args Function Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("increment()", None)
		.build();

	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"increment".to_string(),
		)
		.build();

	let block = BlockBuilder::new()
		.number(5)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block to get a match
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found a match to handle");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	// Set up a mock trigger execution service
	let mut trigger_execution_service = setup_trigger_execution_service(
		"tests/integration/fixtures/midnight/triggers/trigger.json",
	)
	.await;
	trigger_execution_service
		.expect_execute()
		.returning(|_, _, _, _| Ok(()));

	// Process the match using handle_match
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
async fn test_handle_match_with_key_collision() -> Result<(), Box<FilterError>> {
	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.build();
	let filter_service = FilterService::new();
	let trigger_scripts = HashMap::new();

	// Create a monitor for a function with an argument named 'signature'
	let monitor = MonitorBuilder::new()
		.name("Test Key Collision Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.function("riskyFunction(signature, amount)", None)
		.build();

	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"riskyFunction(signature, amount)".to_string(),
		)
		.build();

	let block = BlockBuilder::new()
		.number(6)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block to get a match
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found a match to handle");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	// Set up a mock trigger execution service that captures the data map
	let data_capture = Arc::new(Mutex::new(HashMap::new()));
	let data_capture_clone = data_capture.clone();
	let mut trigger_execution_service = setup_trigger_execution_service(
		"tests/integration/fixtures/midnight/triggers/trigger.json",
	)
	.await;
	trigger_execution_service
		.expect_execute()
		.withf(
			move |_trigger_name, variables, _monitor_match, _trigger_scripts| {
				let mut captured = data_capture_clone.lock().unwrap();
				*captured = variables.clone();
				true
			},
		)
		.returning(|_, _, _, _| Ok(()));

	// Process the match using handle_match
	for matching_monitor in matches {
		let result = handle_match(
			matching_monitor.clone(),
			&trigger_execution_service,
			&trigger_scripts,
		)
		.await;
		assert!(result.is_ok(), "Handle match should succeed");
	}

	// Verify that both the function signature and argument are present and distinct
	let captured_data = data_capture.lock().unwrap();
	assert!(
		captured_data.contains_key("functions.0.signature"),
		"functions.0.signature should exist in the data structure"
	);
	assert_eq!(
		captured_data.get("functions.0.signature").unwrap(),
		"riskyFunction",
		"Function signature value should be preserved"
	);
	Ok(())
}

#[tokio::test]
async fn test_monitor_transaction_status_success() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.add_rpc_url("wss://any-websocket-url", "ws_rpc", 100)
		.build();

	// Create a monitor that watches for successful transactions
	let monitor = MonitorBuilder::new()
		.name("Test Success Transaction Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.transaction(TransactionStatus::Success, None)
		.build();

	// Create a test transaction
	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main".to_string(),
		)
		.build();

	// Create a block containing our transaction
	let block = BlockBuilder::new()
		.number(7)
		.add_rpc_transaction(transaction.clone().into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	// Create a success event for the transaction
	let success_event = EventBuilder::new()
		.tx_applied(transaction.hash().to_string())
		.build();

	// Mock successful event for the transaction
	mock_client
		.expect_get_events()
		.returning(move |_, _| Ok(vec![success_event.clone()]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching transactions"
	);
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.transactions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.functions.is_empty());
			assert!(midnight_match.matched_on.transactions[0].status == TransactionStatus::Success);
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transaction_status_failure() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.add_rpc_url("wss://any-websocket-url", "ws_rpc", 100)
		.build();

	// Create a monitor that watches for failed transactions
	let monitor = MonitorBuilder::new()
		.name("Test Failure Transaction Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.transaction(TransactionStatus::Failure, None)
		.build();

	// Create a test transaction
	let transaction = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main".to_string(),
		)
		.build();

	// Create a block containing our transaction
	let block = BlockBuilder::new()
		.number(8)
		.add_rpc_transaction(transaction.into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	// Mock no events (which means failure)
	mock_client.expect_get_events().returning(|_, _| Ok(vec![]));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching transactions"
	);
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.transactions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.functions.is_empty());
			assert!(midnight_match.matched_on.transactions[0].status == TransactionStatus::Failure);
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transaction_status_any() -> Result<(), Box<FilterError>> {
	let filter_service = FilterService::new();

	let network = NetworkBuilder::new()
		.network_type(BlockChainType::Midnight)
		.add_rpc_url("wss://any-websocket-url", "ws_rpc", 100)
		.build();

	// Create a monitor that watches for any transaction status
	let monitor = MonitorBuilder::new()
		.name("Test Any Transaction Monitor")
		.address("0202000000000000000000000000000000000000000000000000000000000000000000")
		.transaction(TransactionStatus::Any, None)
		.build();

	// Create two test transactions
	let success_tx = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main".to_string(),
		)
		.hash("0x1000000000000000000000000000000000000000000000000000000000000000".to_string())
		.build();

	let failure_tx = TransactionBuilder::new()
		.add_call_operation(
			"0202000000000000000000000000000000000000000000000000000000000000000000".to_string(),
			"main".to_string(),
		)
		.hash("0x2000000000000000000000000000000000000000000000000000000000000000".to_string())
		.build();

	// Create a block containing both transactions
	let block = BlockBuilder::new()
		.number(9)
		.add_rpc_transaction(success_tx.clone().into())
		.add_rpc_transaction(failure_tx.clone().into())
		.build();

	let mut mock_client = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock_client
		.expect_get_chain_type()
		.returning(|| Ok("testnet-02-1".to_string()));

	// Create a success event for the first transaction
	let success_event = EventBuilder::new()
		.tx_applied(success_tx.hash().to_string())
		.build();

	// Mock events to return success for first tx, nothing for second tx
	mock_client
		.expect_get_events()
		.returning(move |_, _| Ok(vec![success_event.clone()]));

	let matches = filter_service
		.filter_block(
			&mock_client,
			&network,
			&BlockType::Midnight(Box::new(block)),
			&[monitor.clone()],
			None,
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching transactions"
	);
	assert_eq!(matches.len(), 2, "Expected exactly two matches");

	// First match should be the success transaction
	match &matches[0] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.transactions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.functions.is_empty());
			assert!(midnight_match.matched_on.transactions[0].status == TransactionStatus::Success);
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	// Second match should be the failure transaction
	match &matches[1] {
		MonitorMatch::Midnight(midnight_match) => {
			assert!(midnight_match.matched_on.transactions.len() == 1);
			assert!(midnight_match.matched_on.events.is_empty());
			assert!(midnight_match.matched_on.functions.is_empty());
			assert!(midnight_match.matched_on.transactions[0].status == TransactionStatus::Failure);
		}
		_ => {
			panic!("Expected Midnight match");
		}
	}

	Ok(())
}
