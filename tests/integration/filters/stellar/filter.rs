//! Integration tests for Stellar chain monitoring.
//!
//! Tests the monitoring functionality for the Stellar blockchain,
//! including contract invocations and transaction filtering.

use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{
		AddressWithSpec, BlockChainType, BlockType, ContractSpec, EventCondition,
		FunctionCondition, MatchConditions, Monitor, MonitorMatch, StellarBlock,
		StellarContractSpec, StellarEvent, StellarMatchArguments, StellarMatchParamEntry,
		StellarMatchParamsMap, StellarMonitorMatch, StellarTransaction, StellarTransactionInfo,
		TransactionCondition, TransactionStatus, TransactionType,
	},
	services::filter::{handle_match, FilterError, FilterService},
};

use crate::integration::{
	filters::common::{load_test_data, read_and_parse_json, setup_trigger_execution_service},
	mocks::{
		create_test_block, create_test_transaction, MockStellarClientTrait,
		MockStellarTransportClient,
	},
};

use serde_json::{json, Value};

fn make_monitor_with_events(mut monitor: Monitor, include_expression: bool) -> Monitor {
	monitor.match_conditions.functions = vec![];
	monitor.match_conditions.transactions = vec![];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.events.push(EventCondition {
		signature: "transfer(Address,Address,String,I128)".to_string(),
		expression: if include_expression {
			Some(
				"0 == GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY AND 3 >= 2240"
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
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "transfer(Address,Address,I128)".to_string(),
		expression: if include_expression {
			Some("amount >= 2240".to_string())
		} else {
			None
		},
	}];
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
			status: TransactionStatus::Failure,
			expression: if include_expression {
				Some("value >= 498000000".to_string())
			} else {
				None
			},
		});
	monitor
}

#[tokio::test]
async fn test_monitor_events_with_no_expressions() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_events(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching events");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.events.len() == 1);
			assert!(stellar_match.matched_on.functions.is_empty());
			assert!(stellar_match.matched_on.transactions.is_empty());
			assert!(
				stellar_match.matched_on.events[0].signature
					== "transfer(Address,Address,String,I128)"
			);

			let matched_on_args = stellar_match.matched_on_args.as_ref().unwrap();
			assert!(
				matched_on_args.events.as_ref().unwrap().is_empty(),
				"Expected no events arguments to be matched"
			);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_events_with_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_events(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			None,
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching events");
	assert_eq!(matches.len(), 1, "Expected exactly one match");
	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.events.len() == 1);
			assert!(stellar_match.matched_on.functions.is_empty());
			assert!(stellar_match.matched_on.transactions.is_empty());
			assert!(
				stellar_match.matched_on.events[0].signature
					== "transfer(Address,Address,String,I128)"
			);

			let matched_on_args = stellar_match.matched_on_args.as_ref().unwrap();
			let event_args = &matched_on_args.events.as_ref().unwrap()[0];

			assert_eq!(
				event_args.signature,
				"transfer(Address,Address,String,I128)"
			);

			// Assert the argument values
			let args = event_args.args.as_ref().unwrap();
			assert_eq!(args[0].name, "0");
			assert_eq!(
				args[0].value,
				"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY"
			);
			assert_eq!(args[0].kind, "Address");
			assert!(args[0].indexed);

			assert_eq!(args[1].name, "1");
			assert_eq!(
				args[1].value,
				"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT"
			);
			assert_eq!(args[1].kind, "Address");
			assert!(args[1].indexed);

			assert_eq!(args[2].name, "2");
			assert_eq!(
				args[2].value,
				"USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5"
			);
			assert_eq!(args[2].kind, "String");
			assert!(args[2].indexed);

			assert_eq!(args[3].name, "3");
			assert_eq!(args[3].value, "2240");
			assert_eq!(args[3].kind, "I128");
			assert!(!args[3].indexed);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_no_expressions() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_functions(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.functions.len() == 1);
			assert!(stellar_match.matched_on.events.is_empty());
			assert!(stellar_match.matched_on.transactions.is_empty());
			assert!(
				stellar_match.matched_on.functions[0].signature == "transfer(Address,Address,I128)"
			);

			let matched_on_args = stellar_match.matched_on_args.as_ref().unwrap();
			assert!(
				stellar_match.matched_on.functions[0].expression.is_none(),
				"Expected no function expression"
			);
			assert!(
				matched_on_args.functions.as_ref().unwrap().len() == 1,
				"Expected one function match"
			);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_expressions() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_functions(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching functions");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.functions.len() == 1);
			assert!(stellar_match.matched_on.events.is_empty());
			assert!(stellar_match.matched_on.transactions.is_empty());
			assert!(
				stellar_match.matched_on.functions[0].signature == "transfer(Address,Address,I128)"
			);

			let matched_on_args = stellar_match.matched_on_args.as_ref().unwrap();
			let function_args = &matched_on_args.functions.as_ref().unwrap()[0];

			assert_eq!(function_args.signature, "transfer(Address,Address,I128)");

			// Assert the argument values
			let args = function_args.args.as_ref().unwrap();

			assert_eq!(args[0].name, "from");
			assert_eq!(
				args[0].value,
				"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY"
			);
			assert_eq!(args[0].kind, "Address");
			assert!(!args[0].indexed);

			assert_eq!(args[1].name, "to");
			assert_eq!(
				args[1].value,
				"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT"
			);
			assert_eq!(args[1].kind, "Address");
			assert!(!args[1].indexed);

			assert_eq!(args[2].name, "amount");
			assert_eq!(args[2].value, "2240");
			assert_eq!(args[2].kind, "I128");
			assert!(!args[2].indexed);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transactions_with_expressions() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_transactions(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
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

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.transactions.len() == 1);
			assert!(stellar_match.matched_on.events.is_empty());
			assert!(stellar_match.matched_on.functions.is_empty());
			assert!(stellar_match.matched_on.transactions[0].status == TransactionStatus::Failure);
			assert!(
				stellar_match.matched_on.transactions[0]
					.expression
					.clone()
					.unwrap() == "value >= 498000000"
			);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_transactions_with_no_expressions() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_transactions(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(test_data.contract_spec.clone().unwrap()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
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

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.transactions.len() == 1);
			assert!(stellar_match.matched_on.events.is_empty());
			assert!(stellar_match.matched_on.functions.is_empty());
			assert!(stellar_match.matched_on.transactions[0].status == TransactionStatus::Failure);
			assert!(stellar_match.matched_on.transactions[0]
				.expression
				.is_none());
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_with_multiple_conditions() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = test_data.contract_spec.unwrap();
	let transfer_contract_with_spec: (String, ContractSpec) = (
		"CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA".to_string(),
		contract_spec.clone(),
	);
	let upsert_contract_with_spec: (String, ContractSpec) = (
		"CBWRWC2IFNRXKAW2HG5473V5U25OMUKVIE3BFZBIWOOD3VLEIBUIOQG6".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
			Some(&[transfer_contract_with_spec, upsert_contract_with_spec]),
		)
		.await?;

	assert!(
		!matches.is_empty(),
		"Should have found matching functions and events"
	);

	if let MonitorMatch::Stellar(stellar_match) = &matches[0] {
		assert!(
			!stellar_match.matched_on.events.is_empty(),
			"Should have matched events"
		);
		assert!(
			!stellar_match.matched_on.functions.is_empty(),
			"Should have matched functions"
		);

		assert!(
			!stellar_match.matched_on.transactions.is_empty(),
			"Should have matched transactions"
		);

		if let Some(args) = &stellar_match.matched_on_args {
			if let Some(events) = &args.events {
				assert!(!events.is_empty(), "Should have event arguments");
				let event = &events[0];
				assert_eq!(event.signature, "transfer(Address,Address,String,I128)");
			}

			if let Some(functions) = &args.functions {
				assert!(!functions.is_empty(), "Should have function arguments");
				let function = &functions[0];
				assert_eq!(function.signature, "transfer(Address,Address,I128)");
			}
		}
	}

	if let MonitorMatch::Stellar(stellar_match) = &matches[1] {
		assert!(
			stellar_match.matched_on.events.is_empty(),
			"Should not have matched events"
		);
		assert!(
			!stellar_match.matched_on.functions.is_empty(),
			"Should have matched functions"
		);
		assert!(
			!stellar_match.matched_on.transactions.is_empty(),
			"Should have matched transactions"
		);

		if let Some(args) = &stellar_match.matched_on_args {
			if let Some(functions) = &args.functions {
				assert!(!functions.is_empty(), "Should have function arguments");
				let function = &functions[0];
				assert_eq!(function.signature, "upsert_data(Map<String,String>)");
			}
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_error_cases() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	let mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();

	// Create an invalid block type
	let invalid_block = BlockType::EVM(Box::default());

	let result = filter_service
		.filter_block(
			&mock_client,
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
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();
	let trigger_scripts = HashMap::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CBWRWC2IFNRXKAW2HG5473V5U25OMUKVIE3BFZBIWOOD3VLEIBUIOQG6".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/stellar/triggers/trigger.json")
			.await;

	// First expectation for the events-only match
	trigger_execution_service
		.expect_execute()
		.withf(
			|trigger_name, variables, _monitor_match, _trigger_scripts| {
				trigger_name == ["example_trigger_slack"]
				// Monitor metadata
				&& variables.get("monitor.name") == Some(&"Large Transfer of USDC Token".to_string())
				// Transaction variables
				&& variables.get("transaction.hash")
					== Some(&"2c89fc3311bc275415ed6a764c77d7b0349cb9f4ce37fd2bbfc6604920811503".to_string())
				// Event arguments
				&& variables.get("events.0.signature") == Some(&"transfer(Address,Address,String,I128)".to_string())
				&& variables.get("events.0.args.0") == Some(&"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY".to_string())
				&& variables.get("events.0.args.1") == Some(&"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT".to_string())
				&& variables.get("events.0.args.2") == Some(&"USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5".to_string())
				&& variables.get("events.0.args.3") == Some(&"2240".to_string())
			},
		)
		.once()
		.returning(|_, _, _, _| Ok(()));

	// Second expectation for the upsert_data function match
	trigger_execution_service
		.expect_execute()
		.withf(
			|trigger_name, variables, _monitor_match, _trigger_scripts| {
				let expected_json: Value =
					serde_json::from_str("{\"myKey1\":1234,\"myKey2\":\"Hello, world!\"}").unwrap();
				let actual_json: Value =
					serde_json::from_str(variables.get("functions.0.args.data").unwrap()).unwrap();

				trigger_name == ["example_trigger_slack"]
				// Monitor metadata
				&& variables.get("monitor.name") == Some(&"Large Transfer of USDC Token".to_string())
				// Transaction variables
				&& variables.get("transaction.hash")
					== Some(&"FAKE5a3a9153e19002517935a5df291b81a341b98ccd80f0919d78cea5ed29d8".to_string())
				// Function arguments
				&& variables.get("functions.0.signature") == Some(&"upsert_data(Map<String,String>)".to_string())
				&& expected_json == actual_json
			},
		)
		.once()
		.returning(|_, _, _, _| Ok(()));

	let matches = filter_service
		.filter_block(
			&mock_client,
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
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let mut monitor = test_data.monitor;
	// Clear existing conditions and add functions without arguments
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "increment()".to_string(),
		expression: None,
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CDMZ6LU66KEMLKI3EJBIGXTZ4KZ2CRTSHZETMY3QQZBWRKVKB5EIOHTX".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches");
	assert_eq!(matches.len(), 1, "Expected exactly one match");

	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(stellar_match.matched_on.functions.len() == 1);
			assert!(stellar_match.matched_on.events.is_empty());
			assert!(stellar_match.matched_on.transactions.is_empty());
			assert!(stellar_match.matched_on.functions[0].signature == "increment()");

			// Now test handle_match to verify the data map contains signatures
			let trigger_scripts = HashMap::new();
			let mut trigger_execution_service = setup_trigger_execution_service(
				"tests/integration/fixtures/stellar/triggers/trigger.json",
			)
			.await;

			// Set up expectations for execute()
			trigger_execution_service
				.expect_execute()
				.withf(|trigger_name, variables, _monitor_match, _trigger_scripts| {
					trigger_name == ["example_trigger_slack"]
						// Monitor metadata
						&& variables.get("monitor.name") == Some(&"Large Transfer of USDC Token".to_string())
						// Transaction variables
						&& variables.get("transaction.hash") == Some(&"80fec04b989895a4222d9985fbf153d253e3e2cbc1da45ef414db96a277b99be".to_string())
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
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_handle_match_with_key_collision() -> Result<(), Box<FilterError>> {
	// Load test data using common utility
	let test_data = load_test_data("stellar");

	// Setup trigger execution service and capture the data structure
	let data_capture = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
	let data_capture_clone = data_capture.clone();

	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/stellar/triggers/trigger.json")
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

	// Create test monitor with a function that has an argument called "signature"
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "riskyFunction(String signature, I128 amount)".to_string(),
		expression: None,
	}];

	fn create_test_stellar_transaction() -> StellarTransaction {
		match create_test_transaction(BlockChainType::Stellar) {
			TransactionType::Stellar(transaction) => *transaction,
			_ => panic!("Expected Stellar transaction"),
		}
	}

	fn create_test_stellar_block() -> StellarBlock {
		match create_test_block(BlockChainType::Stellar, 1) {
			BlockType::Stellar(block) => *block,
			_ => panic!("Expected Stellar block"),
		}
	}

	// Create a match object
	let stellar_match = StellarMonitorMatch {
		monitor,
		transaction: create_test_stellar_transaction(),
		network_slug: "stellar_testnet".to_string(),
		ledger: create_test_stellar_block(),
		matched_on: MatchConditions {
			functions: vec![FunctionCondition {
				signature: "riskyFunction(String signature, I128 amount)".to_string(),
				expression: None,
			}],
			events: vec![],
			transactions: vec![],
		},
		matched_on_args: Some(StellarMatchArguments {
			functions: Some(vec![StellarMatchParamsMap {
				signature: "riskyFunction(String signature, I128 amount)".to_string(),
				args: Some(vec![
					StellarMatchParamEntry {
						name: "signature".to_string(),
						value: "test_signature_value".to_string(),
						kind: "String".to_string(),
						indexed: false,
					},
					StellarMatchParamEntry {
						name: "amount".to_string(),
						value: "500000".to_string(),
						kind: "I128".to_string(),
						indexed: false,
					},
				]),
			}]),
			events: None,
		}),
	};

	let match_wrapper = MonitorMatch::Stellar(Box::new(stellar_match));

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
		"riskyFunction(String signature, I128 amount)",
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
		"test_signature_value",
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
async fn test_filter_with_contract_spec() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CDMZ6LU66KEMLKI3EJBIGXTZ4KZ2CRTSHZETMY3QQZBWRKVKB5EIOHTX".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	// Expect contract spec to be called
	mock_client
		.expect_get_contract_spec()
		.returning(move |_| Ok(contract_spec.clone()));

	// Create a monitor that requires contract spec validation
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "increment()".to_string(),
		expression: None,
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(matches.len() == 1, "Should have found exactly 1 match");

	// Verify that the matches contain the expected function
	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(!stellar_match.matched_on.functions.is_empty());
			assert_eq!(
				stellar_match.matched_on.functions[0].signature,
				"increment()"
			);
		}
		_ => panic!("Expected Stellar match"),
	}

	Ok(())
}

#[tokio::test]
async fn test_filter_with_invalid_contract_spec() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	// Setup mock to return error for contract spec
	mock_client
		.expect_get_contract_spec()
		.returning(|_| Err(anyhow::anyhow!("Failed to get contract spec")));

	// Create a monitor that requires contract spec validation
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "increment()".to_string(),
		expression: None,
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	// Run filter_block with the test data
	let result = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			None,
		)
		.await;

	// When the contract spec is not found, the filter should return no matches
	assert!(result.is_ok());
	assert!(result.unwrap().is_empty());

	Ok(())
}

#[tokio::test]
async fn test_filter_with_abi_in_config() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = test_data.contract_spec.unwrap();
	let contract_with_spec: (String, ContractSpec) = (
		"CDMZ6LU66KEMLKI3EJBIGXTZ4KZ2CRTSHZETMY3QQZBWRKVKB5EIOHTX".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	// get_contract_spec should NOT be called since we provide the ABI in config
	mock_client.expect_get_contract_spec().times(0);

	// Create a monitor with ABI in config
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "increment()".to_string(),
		expression: None,
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	// Add ABI to the monitor's address configuration
	monitor.addresses = vec![AddressWithSpec {
		address: contract_with_spec.0.clone(),
		contract_spec: Some(contract_with_spec.1.clone()),
	}];

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(matches.len() == 1, "Should have found exactly 1 match");

	// Verify that the matches contain the expected function
	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(!stellar_match.matched_on.functions.is_empty());
			assert_eq!(
				stellar_match.matched_on.functions[0].signature,
				"increment()"
			);
		}
		_ => panic!("Expected Stellar match"),
	}

	Ok(())
}

#[tokio::test]
async fn test_filter_with_udt_expression() -> Result<(), Box<FilterError>> {
	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");
	let contract_spec = ContractSpec::Stellar(StellarContractSpec::from(json!([{
		"function_v0": {
			"doc": "",
			"name": "submit",
			"inputs": [
				{
					"doc": "",
					"name": "from",
					"type_": "address"
				},
				{
					"doc": "",
					"name": "spender",
					"type_": "address"
				},
				{
					"doc": "",
					"name": "to",
					"type_": "address"
				},
				{
					"doc": "",
					"name": "requests",
					"type_": {
						"vec": {
							"element_type": {
								"udt": {
									"name": "Request"
								}
							}
						}
					}
				}
			],
			"outputs": [
				{
					"udt": {
						"name": "Positions"
					}
				}
			]
		}
	}])));

	let contract_with_spec: (String, ContractSpec) = (
		"CAJJZSGMMM3PD7N33TAPHGBUGTB43OC73HVIK2L2G6BNGGGYOSSYBXBD".to_string(),
		contract_spec.clone(),
	);

	let mut mock_client = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let decoded_transactions: Vec<StellarTransaction> = transactions
		.iter()
		.map(|tx| StellarTransaction::from(tx.clone()))
		.collect();

	// Setup mock expectations
	mock_client
		.expect_get_transactions()
		.times(1)
		.returning(move |_, _| Ok(decoded_transactions.clone()));

	mock_client
		.expect_get_events()
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	// get_contract_spec should NOT be called since we provide the ABI in config
	mock_client.expect_get_contract_spec().times(0);

	// Create a monitor with ABI in config
	let mut monitor = test_data.monitor.clone();
	monitor.match_conditions.functions = vec![FunctionCondition {
		signature: "submit(Address,Address,Address,Vec<Request>)".to_string(),
		expression: Some(
			"requests contains CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
				.to_string(),
		),
	}];
	monitor.match_conditions.events = vec![];
	monitor.match_conditions.transactions = vec![];

	// Add ABI to the monitor's address configuration
	monitor.addresses = vec![AddressWithSpec {
		address: contract_with_spec.0.clone(),
		contract_spec: Some(contract_with_spec.1.clone()),
	}];

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
			Some(&[contract_with_spec]),
		)
		.await?;

	assert!(matches.len() == 1, "Should have found exactly 1 match");

	// Verify that the matches contain the expected function
	match &matches[0] {
		MonitorMatch::Stellar(stellar_match) => {
			assert!(!stellar_match.matched_on.functions.is_empty());
			assert_eq!(
				stellar_match.matched_on.functions[0].signature,
				"submit(Address,Address,Address,Vec<Request>)"
			);
		}
		_ => panic!("Expected Stellar match"),
	}

	Ok(())
}
