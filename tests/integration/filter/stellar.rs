//! Integration tests for Stellar chain monitoring.
//!
//! Tests the monitoring functionality for the Stellar blockchain,
//! including contract invocations and transaction filtering.

use log::info;
use openzeppelin_monitor::{
	models::{MonitorMatch, StellarEvent, StellarTransaction, StellarTransactionInfo},
	services::filter::{handle_match, FilterError, FilterService},
};

use crate::integration::{
	filter::common::{load_test_data, read_and_parse_json, setup_trigger_execution_service},
	mocks::MockStellarClientTrait,
};

#[tokio::test]
async fn test_monitor_should_detect_token_transfer() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	// Load test data using common utility
	let test_data = load_test_data("stellar");

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

	let filter_service = FilterService::new();

	let matches = filter_service
		.filter_block(
			&mock_client,
			&test_data.network,
			&test_data.blocks[0],
			&[test_data.monitor],
		)
		.await?;

	assert!(!matches.is_empty(), "Should have found matching events");
	assert_eq!(
		matches.len(),
		1,
		"Expected exactly one match for the token transfer"
	);

	let trigger_execution_service =
		setup_trigger_execution_service("tests/integration/fixtures/stellar/triggers/trigger.json");

	for matching_monitor in matches {
		match matching_monitor.clone() {
			MonitorMatch::Stellar(stellar_monitor_match) => {
				info!(
					"Stellar monitor match: {:?}",
					stellar_monitor_match.transaction.hash()
				);
			}
			_ => {
				info!("Unknown monitor match");
			}
		}
		let _ = handle_match(matching_monitor, &trigger_execution_service).await;
	}
	Ok(())
}
