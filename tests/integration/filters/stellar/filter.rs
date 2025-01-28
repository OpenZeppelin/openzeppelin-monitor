//! Integration tests for Stellar chain monitoring.
//!
//! Tests the monitoring functionality for the Stellar blockchain,
//! including contract invocations and transaction filtering.

use openzeppelin_monitor::{
	models::{
		BlockType, EventCondition, FunctionCondition, Monitor, MonitorMatch, StellarEvent,
		StellarTransaction, StellarTransactionInfo, TransactionCondition, TransactionStatus,
	},
	services::filter::{handle_match, FilterError, FilterService},
};

use crate::integration::{
	filters::common::{load_test_data, read_and_parse_json, setup_trigger_execution_service},
	mocks::MockStellarClientTrait,
};

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
	monitor.match_conditions.functions = vec![];
	monitor.match_conditions.functions.push(FunctionCondition {
		signature: "transfer(Address,Address,I128)".to_string(),
		expression: if include_expression {
			Some("2 >= 2240".to_string())
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
async fn test_monitor_events_with_no_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_events(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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
async fn test_monitor_events_with_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_events(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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
async fn test_monitor_functions_with_no_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_functions(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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
				matched_on_args.functions.as_ref().unwrap().is_empty(),
				"Expected no functions arguments to be matched"
			);
		}
		_ => {
			panic!("Expected Stellar match");
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_functions_with_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_functions(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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

			assert_eq!(args[0].name, "0");
			assert_eq!(
				args[0].value,
				"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY"
			);
			assert_eq!(args[0].kind, "Address");
			assert!(!args[0].indexed);

			assert_eq!(args[1].name, "1");
			assert_eq!(
				args[1].value,
				"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT"
			);
			assert_eq!(args[1].kind, "Address");
			assert!(!args[1].indexed);

			assert_eq!(args[2].name, "2");
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
async fn test_monitor_transactions_with_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_transactions(test_data.monitor, true);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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
async fn test_monitor_transactions_with_no_expressions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	let monitor = make_monitor_with_transactions(test_data.monitor, false);

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[monitor],
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
async fn test_monitor_with_multiple_conditions() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
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
				assert_eq!(function.signature, "upsert_data(Map)");
			}
		}
	}

	Ok(())
}

#[tokio::test]
async fn test_monitor_error_cases() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	let mock_client = MockStellarClientTrait::new();

	// Create an invalid block type
	let invalid_block = BlockType::EVM(Box::default());

	let result = filter_service
		.filter_block(
			&mock_client,
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
async fn test_handle_match() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	let test_data = load_test_data("stellar");
	let filter_service = FilterService::new();

	// Load Stellar-specific test data
	let events: Vec<StellarEvent> =
		read_and_parse_json("tests/integration/fixtures/stellar/events.json");
	let transactions: Vec<StellarTransactionInfo> =
		read_and_parse_json("tests/integration/fixtures/stellar/transactions.json");

	let mut mock_client = MockStellarClientTrait::new();
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

	let mut trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/stellar/triggers/trigger.json");

	trigger_execution_service
		.expect_execute()
		.withf(|trigger_name, variables| {
			trigger_name == ["large_transfer_slack"]
				// Monitor metadata
				&& variables.get("monitor_name") == Some(&"Large Transfer of USDC Token".to_string())
				// Transaction variables
				&& variables.get("transaction_hash")
					== Some(&"2c89fc3311bc275415ed6a764c77d7b0349cb9f4ce37fd2bbfc6604920811503".to_string())
				// Function arguments
				&& variables.get("function_0_signature") == Some(&"transfer(Address,Address,I128)".to_string())
				&& variables.get("function_0_0") == Some(&"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY".to_string())
				&& variables.get("function_0_1") == Some(&"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT".to_string())
				&& variables.get("function_0_2") == Some(&"2240".to_string())
				// Event arguments
				&& variables.get("event_0_signature") == Some(&"transfer(Address,Address,String,I128)".to_string())
				&& variables.get("event_0_0") == Some(&"GDF32CQINROD3E2LMCGZUDVMWTXCJFR5SBYVRJ7WAAIAS3P7DCVWZEFY".to_string())
				&& variables.get("event_0_1") == Some(&"CC7YMFMYZM2HE6O3JT5CNTFBHVXCZTV7CEYT56IGBHR4XFNTGTN62CPT".to_string())
				&& variables.get("event_0_2") == Some(&"USDC:GBBD47IF6LWK7P7MDEVSCWR7DPUWV3NY3DTQEVFL4NAT4AQH3ZLLFLA5".to_string())
				&& variables.get("event_0_3") == Some(&"2240".to_string())
		})
		.once()
		.returning(|_, _| Ok(()));

	trigger_execution_service
		.expect_execute()
		.withf(|trigger_name, variables| {
			trigger_name == ["large_transfer_slack"]
				// Monitor metadata
				&& variables.get("monitor_name") == Some(&"Large Transfer of USDC Token".to_string())
				// Transaction variables
				&& variables.get("transaction_hash")
					== Some(&"FAKE5a3a9153e19002517935a5df291b81a341b98ccd80f0919d78cea5ed29d8".to_string())
				// Function arguments
				&& variables.get("function_0_signature") == Some(&"upsert_data(Map)".to_string())
				&& variables.get("function_0_0") == Some(&"{\"\\\"myKey1\\\"\":1234,\"\\\"myKey2\\\"\":\"Hello, world!\"}".to_string())
		})
		.once()
		.returning(|_, _| Ok(()));

	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matches to handle");

	for matching_monitor in matches {
		let result = handle_match(matching_monitor.clone(), &trigger_execution_service).await;
		assert!(result.is_ok(), "Handle match should succeed");
	}

	Ok(())
}
