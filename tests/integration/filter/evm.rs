//! Integration tests for EVM chain monitoring.
//!
//! Tests the monitoring functionality for EVM-compatible blockchains,
//! including event and transaction filtering.

use log::info;
use openzeppelin_monitor::{
	models::MonitorMatch,
	services::{
		blockchain::create_blockchain_client,
		filter::{handle_match, FilterError, FilterService},
	},
};

use crate::integration::filter::common::{load_test_data, setup_trigger_execution_service};

#[tokio::test]
async fn test_monitor_should_detect_token_transfer() -> Result<(), FilterError> {
	let _ = env_logger::builder().is_test(true).try_init();

	// Load test data using common utility
	let test_data = load_test_data("evm");
	let filter_service = FilterService::new();
	let client = create_blockchain_client(&test_data.network).await.unwrap();

	// Run filter_block with the test data
	let matches = filter_service
		.filter_block(
			&client,
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
		setup_trigger_execution_service("tests/integration/fixtures/evm/triggers/trigger.json");

	for matching_monitor in matches {
		match matching_monitor.clone() {
			MonitorMatch::EVM(evm_monitor_match) => {
				info!(
					"EVM monitor match: {:?}",
					evm_monitor_match.transaction.hash()
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
