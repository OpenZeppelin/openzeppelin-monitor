use mockall::predicate;
use serde_json::{json, Value};

use openzeppelin_monitor::services::blockchain::{
	BlockChainClient, MidnightClient, MidnightClientTrait,
};

use crate::integration::mocks::{MockMidnightTransportClient, MockWsTransportClient};

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

#[tokio::test]
async fn test_get_events_implementation() {
	let mock_midnight = MockMidnightTransportClient::new();
	let mock_ws = MockWsTransportClient::new();

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			Some(mock_ws),
		);
	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_ok());
	let logs = result.unwrap();
	assert_eq!(logs.len(), 0);
}

#[tokio::test]
#[ignore]
// TODO: Remove ignore once we have an actual implementation for this
async fn test_get_events_missing_result() {
	let mut mock_midnight = MockMidnightTransportClient::new();
	let mock_ws = MockWsTransportClient::new();
	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_midnight
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			Some(mock_ws),
		);
	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
#[ignore]
// TODO: Remove ignore once we have an actual implementation for this
async fn test_events_invalid_format() {
	let mut mock_midnight = MockMidnightTransportClient::new();
	let mock_ws = MockWsTransportClient::new();
	// Mock response with invalid event format
	let mock_response = json!({
		"result": [{
			"invalid_field": "this should fail parsing"
		}]
	});

	mock_midnight
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			Some(mock_ws),
		);
	let result = client.get_events(1, Some(10)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse events"));
}

#[tokio::test]
async fn test_get_latest_block_number_success() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response with a block number
	let mock_response = json!({
		"result": {
			"number": "0x12345"
		}
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getHeader"), predicate::always())
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 74565);
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_response() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response with invalid format
	let mock_response = json!({
		"result": {
			"number": "invalid_hex"
		}
	});

	mock_midnight
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block number"));
}

#[tokio::test]
async fn test_get_latest_block_number_missing_result() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_midnight
		.expect_send_raw_request()
		.with(predicate::eq("chain_getHeader"), predicate::always())
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing block number in response"));
}

#[tokio::test]
async fn test_get_single_block() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response without result field
	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightTransportClient::new();

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
			.returning(MockMidnightTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_multiple_blocks() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response for 3 blocks
	mock_midnight.expect_clone().times(3).returning(|| {
		let mut new_mock = MockMidnightTransportClient::new();

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
			.returning(MockMidnightTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);

	let result = client.get_blocks(1, Some(3)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);
}

#[tokio::test]
async fn test_get_blocks_missing_result() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	// Mock response without result field
	mock_midnight.expect_clone().returning(|| {
		let mut new_mock = MockMidnightTransportClient::new();
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
			.returning(MockMidnightTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_blocks_null_result() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightTransportClient::new();

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
			.returning(MockMidnightTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Result is not a string"));
}

#[tokio::test]
async fn test_get_blocks_parse_failure() {
	let mut mock_midnight = MockMidnightTransportClient::new();

	mock_midnight.expect_clone().times(1).returning(|| {
		let mut new_mock = MockMidnightTransportClient::new();

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
							// Missing required fields
						}
					}).to_string()
				}))
			});

		new_mock
			.expect_clone()
			.returning(MockMidnightTransportClient::new);
		new_mock
	});

	let client =
		MidnightClient::<MockMidnightTransportClient, MockWsTransportClient>::new_with_transport(
			mock_midnight,
			None,
		);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block"));
}
