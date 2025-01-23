//! Integration tests for Stellar chain monitoring.
//!
//! Tests the monitoring functionality for the Stellar blockchain,
//! including contract invocations and transaction filtering.

use openzeppelin_monitor::{
	models::{
		EventCondition, FunctionCondition, Monitor, MonitorMatch, StellarEvent, StellarTransaction,
		StellarTransactionInfo,
	},
	services::filter::{FilterError, FilterService},
};

use crate::integration::{
	filter::common::{load_test_data, read_and_parse_json},
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
	assert_eq!(
		matches.len(),
		1,
		"Expected exactly one match for the token transfer"
	);

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
	assert_eq!(
		matches.len(),
		1,
		"Expected exactly one match for the token transfer"
	);
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
