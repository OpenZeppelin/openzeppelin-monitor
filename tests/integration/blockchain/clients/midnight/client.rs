use crate::integration::mocks::{
	subxt_utils::mock_empty_events, MockMidnightClientTrait, MockMidnightWsTransportClient,
	MockSubstrateClient,
};
use mockall::predicate;
use openzeppelin_monitor::{
	models::BlockType,
	services::blockchain::{BlockChainClient, MidnightClient, MidnightClientTrait},
	utils::tests::midnight::block::BlockBuilder,
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
async fn test_get_chain_type() {
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
async fn test_get_chain_type_error_cases() {
	// Test case 1: Missing result field
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = create_mock_substrate_client();

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("system_chain"), predicate::always())
		.returning(|_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1
			}))
		});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");

	// Test case 2: Null result
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = create_mock_substrate_client();

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("system_chain"), predicate::always())
		.returning(|_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": null
			}))
		});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");

	// Test case 3: Non-string result
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = create_mock_substrate_client();

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("system_chain"), predicate::always())
		.returning(|_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": 123
			}))
		});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");
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
