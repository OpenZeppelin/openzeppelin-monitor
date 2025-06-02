use crate::integration::mocks::{
	mock_empty_events, MockMidnightClientTrait, MockMidnightWsTransportClient, MockSubstrateClient,
};
use mockall::predicate;
use openzeppelin_monitor::{
	models::{BlockType, MidnightEventType},
	services::blockchain::{BlockChainClient, MidnightClient, MidnightClientTrait},
	utils::tests::midnight::{block::BlockBuilder, event::EventBuilder},
};
use serde_json::json;

#[tokio::test]
async fn test_get_events() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	mock.expect_get_events()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(vec![]));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 0);
}

// Helper function to create a configured mock substrate client
fn create_mock_substrate_client() -> MockSubstrateClient {
	let mut mock = MockSubstrateClient::new();
	mock.expect_get_events_at()
		.returning(|_| Ok(mock_empty_events()));
	mock
}

#[tokio::test]
async fn test_get_chain_type_error_cases() {
	let test_cases = vec![
		(
			json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": "testnet-02-1"
			}),
			true,
			"testnet-02-1",
			"",
		),
		(
			json!({
				"jsonrpc": "2.0",
				"id": 1
			}),
			false,
			"",
			"Missing or invalid 'result' field",
		),
		(
			json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": 123
			}),
			false,
			"",
			"Missing or invalid 'result' field",
		),
	];

	for (mock_response, should_succeed, expected_value, error_contains) in test_cases {
		let mut mock_midnight = MockMidnightWsTransportClient::new();
		let mock_substrate = MockSubstrateClient::new();

		mock_midnight
			.expect_send_raw_request()
			.with(predicate::eq("system_chain"), predicate::always())
			.returning(move |_, _| Ok(mock_response.clone()));

		let client = MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

		let result = client.get_chain_type().await;
		if should_succeed {
			assert!(result.is_ok());
			assert_eq!(result.unwrap(), expected_value);
		} else {
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(err.to_string().contains(error_contains));
		}
	}

	// Test case for send_raw_request failure
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("system_chain"), predicate::always())
		.returning(|_, _| Err(anyhow::anyhow!("Failed to connect")));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get chain type"));
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 100u64);
}

#[tokio::test]
async fn test_get_blocks() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	let block = BlockBuilder::new()
		.parent_hash("0xabc123".to_string())
		.number(74565)
		.build();

	let blocks = vec![BlockType::Midnight(Box::new(block))];

	mock.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(blocks.clone()));

	let result = mock.get_blocks(1, Some(2)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
	match &blocks[0] {
		BlockType::Midnight(block) => assert_eq!(block.number(), Some(74565)),
		_ => panic!("Expected Midnight block"),
	}
}

#[tokio::test]
async fn test_new_client() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = create_mock_substrate_client();

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("system_chain"), predicate::always())
		.returning(|_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": "testnet-02-1"
			}))
		});

	mock_midnight
		.expect_get_current_url()
		.returning(|| "ws://dummy".to_string());

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "testnet-02-1");
}

#[tokio::test]
async fn test_get_events_error_cases() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Test case 1: Invalid block range
	mock.expect_get_events()
		.with(predicate::eq(10u64), predicate::eq(Some(5u64)))
		.times(1)
		.returning(move |_, _| Err(anyhow::anyhow!("Invalid block range")));

	let result = mock.get_events(10, Some(5)).await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Invalid block range"));

	// Test case 2: Network error
	mock.expect_get_events()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Err(anyhow::anyhow!("Network error")));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Network error"));
}

#[tokio::test]
async fn test_get_events_with_different_types() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	let events = vec![
		EventBuilder::new()
			.event_type(MidnightEventType::Unknown("unknown".to_string()))
			.build(),
		EventBuilder::new().tx_applied("0x123".to_string()).build(),
	];

	mock.expect_get_events()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(events.clone()));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	let events = result.unwrap();
	assert_eq!(events.len(), 2);
	assert!(matches!(events[0].0, MidnightEventType::Unknown(_)));
	assert!(matches!(
		events[1].0,
		MidnightEventType::MidnightTxApplied(_)
	));
}

#[tokio::test]
async fn test_get_blocks_error_cases() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Test case 1: Invalid block range
	mock.expect_get_blocks()
		.with(predicate::eq(10u64), predicate::eq(Some(5u64)))
		.times(1)
		.returning(move |_, _| Err(anyhow::anyhow!("Invalid block range")));

	let result = mock.get_blocks(10, Some(5)).await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Invalid block range"));

	// Test case 2: Network error
	mock.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Err(anyhow::anyhow!("Network error")));

	let result = mock.get_blocks(1, Some(2)).await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Network error"));
}

#[tokio::test]
async fn test_get_latest_block_number_error_cases() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Test case 1: Network error
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Err(anyhow::anyhow!("Network error")));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Network error"));

	// Test case 2: Invalid response format
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Err(anyhow::anyhow!("Invalid response format")));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Invalid response format"));
}

#[tokio::test]
async fn test_concurrent_operations() {
	let mut mock = MockMidnightClientTrait::<MockMidnightWsTransportClient>::new();

	// Set up expectations for concurrent operations
	mock.expect_get_events()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(vec![]));

	mock.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| {
			Ok(vec![BlockType::Midnight(Box::new(
				BlockBuilder::new().build(),
			))])
		});

	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	// Execute operations concurrently
	let (events_result, blocks_result, block_number_result) = tokio::join!(
		mock.get_events(1, Some(2)),
		mock.get_blocks(1, Some(2)),
		mock.get_latest_block_number()
	);

	assert!(events_result.is_ok());
	assert!(blocks_result.is_ok());
	assert!(block_number_result.is_ok());
	assert_eq!(block_number_result.unwrap(), 100u64);
}
