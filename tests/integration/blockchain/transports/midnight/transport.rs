use mockall::predicate;
use serde_json::{json, Value};

use openzeppelin_monitor::{
	models::MidnightEventType,
	services::blockchain::{BlockChainClient, MidnightClient, MidnightClientTrait, TransportError},
	utils::tests::midnight::event::EventBuilder,
};

use crate::integration::mocks::{
	mock_empty_events, MockMidnightWsTransportClient, MockSubstrateClient,
};

fn create_mock_block(number: u64) -> Value {
	json!({
	  "header": {
		"parentHash": "0x413ea570cf4a1f5eaf5ee06132c91364825fb855df1b187567a10245e3f9a814",
		"number": format!("0x{:x}", number),
		"stateRoot": "0x18f3b75b61e23d3943102738cf031855a75c8e0092713b0a5498ecbabd0edd17",
		"extrinsicsRoot": "0x36525083024b7f46a251a7f0722cc1f1dce4988dbb362678f39ccb2832cdc423",
		"digest": {
		  "logs": [
			"0x0661757261204390561100000000",
			"0x066d637368809651b8379ef4bfbfdaf2639aab753df3260bfd6e96e6c21818dec0c28d185eff",
			"0x044d4e535610401f0000",
			"0x05617572610101a863b83f12e71ad0af022cd899ff98225553d9507ef66dcba1f3349687f59c085b5c2f60551a1501b344118d109e0bde9540fcaadea57ad3c4dd037cebc3d688"
		  ]
		}
	  },
	  "body": [
		{
			"Timestamp": 1744631658000u64
		},
		"UnknownTransaction"
	  ],
	  "transactions_index": []
	})
}

fn create_mock_midnight_clients(
	block_hash: Option<String>,
	events: Option<Vec<Value>>,
) -> (MockMidnightWsTransportClient, MockSubstrateClient) {
	let mut midnight_mock = MockMidnightWsTransportClient::new();
	let mut substrate_mock = MockSubstrateClient::new();
	let block_hash = block_hash.clone();
	let events = events.clone();

	// Mock chain_getBlockHash response
	let block_hash_clone = block_hash.clone();
	midnight_mock
		.expect_send_raw_request()
		.with(predicate::eq("chain_getBlockHash"), predicate::always())
		.returning(move |_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": block_hash_clone.clone()
			}))
		});

	midnight_mock
		.expect_get_current_url()
		.returning(move || "ws://dummy".to_string());

	// Mock midnight_decodeEvents response
	let events_clone = events.clone();
	midnight_mock
		.expect_send_raw_request()
		.with(predicate::eq("midnight_decodeEvents"), predicate::always())
		.returning(move |_, _| {
			Ok(json!({
				"jsonrpc": "2.0",
				"id": 1,
				"result": events_clone.clone()
			}))
		});

	midnight_mock.expect_clone().returning(move || {
		let mut new_mock = MockMidnightWsTransportClient::new();
		let block_hash_clone = block_hash.clone();
		let events_clone = events.clone();

		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(move |_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": block_hash_clone.clone()
				}))
			});

		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_decodeEvents"), predicate::always())
			.returning(move |_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": events_clone.clone()
				}))
			});

		new_mock
	});

	// Set up the substrate mock to return a new mock with get_events expectation when cloned
	substrate_mock.expect_clone().returning(|| {
		let mut new_mock = MockSubstrateClient::new();
		new_mock
			.expect_get_events_at()
			.returning(|_| Ok(mock_empty_events()));
		new_mock.expect_clone().returning(|| {
			let mut new_mock = MockSubstrateClient::new();
			new_mock
				.expect_get_events_at()
				.returning(|_| Ok(mock_empty_events()));
			new_mock.expect_clone().returning(MockSubstrateClient::new);
			new_mock
		});
		new_mock
	});

	(midnight_mock, substrate_mock)
}

#[tokio::test]
async fn test_get_events_implementation() {
	let (mock_midnight, mock_substrate) = create_mock_midnight_clients(
		Some("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string()),
		Some(vec![
			serde_json::to_value(EventBuilder::new().build()).unwrap()
		]),
	);

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_ok());
	let events = result.unwrap();
	assert_eq!(events.len(), 10);
}

#[tokio::test]
async fn test_get_events_missing_result() {
	let mut mock_midnight: MockMidnightWsTransportClient = MockMidnightWsTransportClient::new();
	let mut mock_substrate = MockSubstrateClient::new();

	mock_substrate.expect_clone().returning(|| {
		let mut new_mock = MockSubstrateClient::new();
		new_mock.expect_get_events_at().returning(|_| {
			let mut events_mock = MockSubstrateClient::new();
			events_mock
				.expect_clone()
				.returning(MockSubstrateClient::new);
			Ok(mock_empty_events())
		});
		new_mock
	});

	mock_midnight.expect_clone().returning(move || {
		let mut new_mock: MockMidnightWsTransportClient = MockMidnightWsTransportClient::new();
		// Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(move |_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
				}))
			});

		// Mock midnight_decodeEvents response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_decodeEvents"), predicate::always())
			.returning(move |_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
				}))
			});
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_latest_block_number_success() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	// Mock response with a finalized block hash
	let mock_get_finalised_head_response = json!({
		"result": "0xfinalised_block_hash"
	});

	// Mock response with a block number
	let mock_get_header_response = json!({
		"result": {
			"number": "0x12345"
		}
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getFinalisedHead"), predicate::always())
		.returning(move |_, _| Ok(mock_get_finalised_head_response.clone()));

	mock_midnight
		.expect_send_raw_request()
		.with(
			predicate::eq("chain_getHeader"),
			predicate::function(|params: &Option<Vec<Value>>| match params {
				Some(p) => p == &vec![json!("0xfinalised_block_hash")],
				None => false,
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_get_header_response.clone()));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 74565);
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_response() {
	// Test case 1: Invalid finalized block hash response
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	let mock_get_finalised_head_response = json!({
		"some": "invalid_response"
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getFinalisedHead"), predicate::always())
		.returning(move |_, _| Ok(mock_get_finalised_head_response.clone()));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));

	// Test case 2: Invalid get header response
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	let mock_get_finalised_head_response = json!({
		"result": "0xfinalised_block_hash"
	});

	let mock_get_header_response = json!({
		"result": {
			"number": "invalid_hex"
		}
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getFinalisedHead"), predicate::always())
		.returning(move |_, _| Ok(mock_get_finalised_head_response.clone()));

	mock_midnight
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_get_header_response.clone()));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block number"));

	// Test case 3: Missing result field for get header response
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	let mock_get_finalised_head_response = json!({
		"result": "0xfinalised_block_hash"
	});

	let mock_get_header_response = json!({
		"some": "invalid_response"
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getFinalisedHead"), predicate::always())
		.returning(move |_, _| Ok(mock_get_finalised_head_response.clone()));

	mock_midnight
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_get_header_response.clone()));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing block number in response"));
}

#[tokio::test]
async fn test_get_single_block() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	// Mock response without result field
	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// First call: Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(
				predicate::eq("chain_getBlockHash"),
				predicate::function(|params: &Option<Vec<Value>>| match params {
					Some(p) => p == &vec![json!("0x1")],
					None => false,
				}),
			)
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": "0xmocked_block_hash"
				}))
			});

		// Second call: Mock midnight_jsonBlock response
		new_mock
			.expect_send_raw_request()
			.with(
				predicate::eq("midnight_jsonBlock"),
				predicate::function(|params: &Option<Vec<Value>>| match params {
					Some(p) => p == &vec![json!("0xmocked_block_hash")],
					None => false,
				}),
			)
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": create_mock_block(1).to_string()
				}))
			});

		new_mock
			.expect_clone()
			.returning(MockMidnightWsTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_multiple_blocks() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	// Mock response for 3 blocks
	mock_midnight.expect_clone().times(3).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// First call: Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_num = u64::from_str_radix(
					params.unwrap()[0]
						.as_str()
						.unwrap()
						.trim_start_matches("0x"),
					16,
				)
				.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": format!("0xmocked_block_hash_{}", block_num)
				}))
			});

		// Second call: Mock midnight_jsonBlock response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_jsonBlock"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_hash = params.unwrap()[0].as_str().unwrap().to_string();
				let block_num = block_hash
					.trim_start_matches("0xmocked_block_hash_")
					.parse::<u64>()
					.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": create_mock_block(block_num).to_string()
				}))
			});

		new_mock
			.expect_clone()
			.returning(MockMidnightWsTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(1, Some(3)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);
}

#[tokio::test]
async fn test_get_blocks_missing_result() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	// Mock response without result field
	mock_midnight.expect_clone().returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();
		let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1
		});

		new_mock
			.expect_send_raw_request()
			.times(1)
			.returning(move |_, _| Ok(mock_response.clone()));
		new_mock
			.expect_clone()
			.returning(MockMidnightWsTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_blocks_null_result() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// First call: Mock chain_getBlockHash to return a hash
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": "0xmocked_block_hash"
				}))
			});

		// Second call: Mock midnight_jsonBlock to return null result
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_jsonBlock"), predicate::always())
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": null
				}))
			});

		new_mock
			.expect_clone()
			.returning(MockMidnightWsTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Result is not a string"));
}

#[tokio::test]
async fn test_get_blocks_parse_failure() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// First call: Mock chain_getBlockHash to return a hash
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": "0xmocked_block_hash"
				}))
			});

		// Second call: Mock midnight_jsonBlock with malformed block data
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_jsonBlock"), predicate::always())
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": json!({
						"header": {
							"number": "not_a_hex_number",
							"hash": "invalid_hash"
						}
					}).to_string()
				}))
			});

		new_mock
			.expect_clone()
			.returning(MockMidnightWsTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block"));
}

#[tokio::test]
async fn test_get_events_failed_block_hash() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	// Mock chain_getBlockHash to return an error
	mock_midnight.expect_clone().returning(move || {
		let mut new_mock = MockMidnightWsTransportClient::new();
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(move |_, _| {
				Err(TransportError::network(
					"Failed to get block hash",
					None,
					None,
				))
			});
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to get block hash"));
}

#[tokio::test]
async fn test_get_events_invalid_block_hash() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();
	// Mock chain_getBlockHash to return an invalid hash
	mock_midnight.expect_clone().returning(move || {
		let mut new_mock = MockMidnightWsTransportClient::new();
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(move |_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": "invalid_hash"
				}))
			});
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block hash"));
}

#[tokio::test]
async fn test_get_events_default_event_type() {
	let (mock_midnight, mock_substrate) = create_mock_midnight_clients(
		Some("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string()),
		Some(vec![json!({
			"result": "invalid_event_data"
		})]),
	);

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_ok());

	let events = result.unwrap();
	assert_eq!(events.len(), 10);
	match &events[0].0 {
		MidnightEventType::Unknown(_) => (),
		_ => panic!("Expected Unknown event type"),
	}
}

#[tokio::test]
async fn test_range_validation() {
	let mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	// Test invalid sequence range for blocks
	let result = client.get_blocks(10, Some(5)).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err
		.to_string()
		.contains("start_block 10 cannot be greater than end_block 5"));

	// Test invalid sequence range for events
	let result = client.get_events(10, Some(5)).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err
		.to_string()
		.contains("start_block 10 cannot be greater than end_block 5"));
}

#[tokio::test]
async fn test_single_block_retrieval() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	// Mock responses for block operations
	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_num = u64::from_str_radix(
					params.as_ref().unwrap()[0]
						.as_str()
						.unwrap()
						.trim_start_matches("0x"),
					16,
				)
				.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": format!("0x{}000000000000000000000000000000000000000000000000000000000000000", block_num)
				}))
			});

		// Mock midnight_jsonBlock response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_jsonBlock"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_hash = params.as_ref().unwrap()[0].as_str().unwrap().to_string();
				let block_num = block_hash.chars().nth(2)
					.unwrap()
					.to_string()
					.parse::<u64>()
					.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": create_mock_block(block_num).to_string()
				}))
			});

		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_blocks(5, Some(5)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_single_block_events() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mut mock_substrate = MockSubstrateClient::new();

	// Set up substrate mock expectations
	mock_substrate.expect_clone().returning(|| {
		let mut new_mock = MockSubstrateClient::new();
		new_mock
			.expect_get_events_at()
			.returning(|_| Ok(mock_empty_events()));
		new_mock.expect_clone().returning(|| {
			let mut new_mock = MockSubstrateClient::new();
			new_mock
				.expect_get_events_at()
				.returning(|_| Ok(mock_empty_events()));
			new_mock.expect_clone().returning(MockSubstrateClient::new);
			new_mock
		});
		new_mock
	});

	// Mock responses for event operations
	mock_midnight.expect_clone().times(2).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_num = u64::from_str_radix(
					params.as_ref().unwrap()[0]
						.as_str()
						.unwrap()
						.trim_start_matches("0x"),
					16,
				)
				.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": format!("0x{}000000000000000000000000000000000000000000000000000000000000000", block_num)
				}))
			});

		// Mock midnight_decodeEvents response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_decodeEvents"), predicate::always())
			.returning(|_, _| {
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": vec![
						serde_json::to_value(EventBuilder::new().build()).unwrap()
					]
				}))
			});

		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_events(5, Some(5)).await;
	assert!(result.is_ok());
	let events = result.unwrap();
	assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn test_chain_type_scenarios() {
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
		.returning(|_, _| Err(TransportError::network("Network error", None, None)));

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	let result = client.get_chain_type().await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to get chain type"));
}

#[tokio::test]
async fn test_block_range_operations() {
	let mut mock_midnight = MockMidnightWsTransportClient::new();
	let mock_substrate = MockSubstrateClient::new();

	// Mock responses for block range operations
	mock_midnight.expect_clone().times(103).returning(|| {
		let mut new_mock = MockMidnightWsTransportClient::new();

		// Mock chain_getBlockHash response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("chain_getBlockHash"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_num = u64::from_str_radix(
					params.as_ref().unwrap()[0]
						.as_str()
						.unwrap()
						.trim_start_matches("0x"),
					16,
				)
				.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": format!("0xmocked_block_hash_{}", block_num)
				}))
			});

		// Mock midnight_jsonBlock response
		new_mock
			.expect_send_raw_request()
			.with(predicate::eq("midnight_jsonBlock"), predicate::always())
			.returning(|_, params: Option<Vec<Value>>| {
				let block_hash = params.as_ref().unwrap()[0].as_str().unwrap();
				let block_num = block_hash
					.trim_start_matches("0xmocked_block_hash_")
					.parse::<u64>()
					.unwrap();
				Ok(json!({
					"jsonrpc": "2.0",
					"id": 1,
					"result": create_mock_block(block_num).to_string()
				}))
			});

		new_mock
	});

	let client =
		MidnightClient::<MockMidnightWsTransportClient, MockSubstrateClient>::new_with_transport(
			mock_midnight,
			mock_substrate,
		);

	// Test multiple blocks
	let result = client.get_blocks(1, Some(3)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);

	// Test large range of blocks
	let result = client.get_blocks(1, Some(100)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 100);
}
