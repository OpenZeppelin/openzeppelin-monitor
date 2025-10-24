use mockall::predicate;
use openzeppelin_monitor::{
	models::{BlockType, ContractSpec, StellarFormattedContractSpec},
	services::blockchain::{
		BlockChainClient, StellarClient, StellarClientError, StellarClientTrait, TransportError,
	},
};
use serde_json::{json, Value};

use crate::integration::mocks::MockStellarTransportClient;

#[tokio::test]
async fn test_get_transactions_success() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Expected request parameters
	let expected_params = json!({
		"startLedger": 1,
		"pagination": {
			"limit": 200
		}
	});

	// Mock response with test transactions
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"result": {
			"transactions": [{
				"status": "SUCCESS",
				"txHash": "7723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d",
				"applicationOrder": 1,
				"feeBump": false,
				"envelopeXdr": "AAAAAgAAAADAOBymX0ZJwn9QK/DaEVn8ORqk2YY9rqeml1EB58K5VgAAAZAAAAZtAAApYgAAAAEAAAAAAAAAAAAAAABncLrSAAAAAAAAAAQAAAABAAAAAMA4HKZfRknCf1Ar8NoRWfw5GqTZhj2up6aXUQHnwrlWAAAAEAAAAABznvJY/fwyslMnvuqlsscHjdlIIQL+rX9MZaiv6Ts0nQAAAAAAAAAAAAAAAHOe8lj9/DKyUye+6qWyxweN2UghAv6tf0xlqK/pOzSdAAAAAAAAAAAAAAABAAAAAHOe8lj9/DKyUye+6qWyxweN2UghAv6tf0xlqK/pOzSdAAAABgAAAAFNMkMAAAAAAINfUvmtjRByO4EqNbNZp5pZ1jJZd7ama2bmwFpPcPF4f/////////8AAAABAAAAAHOe8lj9/DKyUye+6qWyxweN2UghAv6tf0xlqK/pOzSdAAAAEQAAAAAAAAAC58K5VgAAAEA9hjt1n+PjXr/7KoRFA4at1DfEkPrI6DEL2NCmhhhAPpxKgI0G/ZWEZYC2V7aF9S5iXxzH6VpIWOQRR4avm6oM6Ts0nQAAAEAaCYFGYFJQx6N5Zb0aM1v00Z1qPyHv88hfY0BjZzm/G0l4tugbKMXzEzh7YkcaIs8pf+KJ97tGEAhok5aTmDMH",
				"resultXdr": "AAAAAAAAAZAAAAAAAAAABAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGAAAAAAAAAAAAAAARAAAAAAAAAAA=",
				"resultMetaXdr": "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",
				"ledger": 1,
				"createdAt": 1735440610
			}],
			"latestLedger": 1,
			"latestLedgerCloseTimestamp": 1740134418,
			"oldestLedger": 1,
			"oldestLedgerCloseTimestamp": 1740033435,
			"cursor": null
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(
			predicate::eq("getTransactions"),
			predicate::function(move |params: &Option<Value>| {
				params.as_ref().unwrap() == &expected_params
			}),
		)
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_transactions(1, Some(2)).await;

	assert!(result.is_ok());
	let transactions = result.unwrap();
	assert_eq!(transactions.len(), 1);
	assert_eq!(
		transactions[0].hash(),
		"7723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d"
	);
}

#[tokio::test]
async fn test_get_transactions_invalid_sequence_range() {
	let mock_stellar = MockStellarTransportClient::new();
	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_transactions(2, Some(1)).await;
	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Invalid input parameters for Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::InvalidInput(ctx)) if ctx.to_string().contains("start_sequence 2 cannot be greater than end_sequence 1")
	),);
}

#[tokio::test]
async fn test_get_transactions_failed_to_parse_transaction() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with invalid transaction data
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"result": {
			"transactions": [{
				"status": "SUCCESS",
				// Missing required fields like txHash, envelopeXdr, etc.
				"ledger": 1
			}],
			"latestLedger": 1
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getTransactions"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_transactions(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Failed to parse transaction response"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::ResponseParseError { .. })
	),);
}

#[tokio::test]
async fn test_get_transactions_outside_of_rpc_retention_window() {
	let mut mock_stellar = MockStellarTransportClient::new();

	let start_block = 57317319; // Example start block outside retention window
	let end_block = 57369158; // Example end block within retention window
	const ERROR_CODE: i64 = -32600;

	let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"error": {
				"code": ERROR_CODE,
				"message": format!("start ledger must be between the oldest ledger: {} and the latest ledger: {} for this rpc instance",
					start_block, end_block),
			}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getTransactions"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_transactions(start_block, Some(end_block)).await;

	assert!(result.is_err());

	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getTransactions"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::OutsideRetentionWindow { rpc_code, rpc_message, .. }) if
			*rpc_code == ERROR_CODE &&
			rpc_message.contains("must be between the oldest ledger")
	),);
}

#[tokio::test]
async fn test_get_transactions_generic_rpc_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with a generic RPC error
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"error": {
			"code": -32603,
			"message": "Internal error"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getTransactions"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_transactions(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getTransactions"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::RpcError { .. })
	),);
}

#[tokio::test]
async fn test_get_transactions_unexpected_response_structure() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with unexpected structure
	let mock_response = json!({
		"result": {
			"unexpectedField": "This is not a valid transaction response"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getTransactions"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_transactions(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err
		.to_string()
		.contains("Failed to parse transaction response"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::UnexpectedResponseStructure { .. })
	),);
}

#[tokio::test]
async fn test_get_transactions_transport_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock a transport error
	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getTransactions"), predicate::always())
		.times(1)
		.returning(move |_, _| {
			Err(TransportError::network(
				"Network error".to_string(),
				None,
				None,
			))
		});

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_transactions(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Failed to getTransactions from Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<TransportError>()),
		Some(TransportError::Network { .. })
	),);
}

#[tokio::test]
async fn test_get_events_success() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Expected request parameters
	let expected_params = json!({
		"startLedger": 1,
		"filters": [{
			"type": "contract",
		}],
		"pagination": {
			"limit": 200
		}
	});

	// Mock response with test events
	let mock_response = json!({
		"result": {
			"events": [{
				"type": "contract",
				"ledger": 1,
				"ledgerClosedAt": "2024-12-29T02:50:10Z",
				"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
				"id": "0001364073023291392-0000000001",
				"pagingToken": "0001364073023291392-0000000001",
				"inSuccessfulContractCall": true,
				"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
				"topic": [
					"AAAADwAAAA9jb250cmFjdF9jYWxsZWQA",
					"AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
					"AAAADgAAAAlnYW5hY2hlLTAAAAA=",
					"AAAADgAAACoweDY4QjkzMDQ1ZmU3RDg3OTRhN2NBRjMyN2U3Zjg1NUNENkNkMDNCQjgAAA==",
					"AAAADQAAACAaemkIzyqB6sH3VVev7iSjYHderf04InYUVZQLYhCsdg=="
				],
				"value": "AAAADQAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAiY2FsbCBmcm9tIHN0ZWxsYXIgYXQgMTczNTQ0MDYwNjk3NwAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=="
			}],
			"cursor": null
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(
			predicate::eq("getEvents"),
			predicate::function(move |params: &Option<Value>| {
				params.as_ref().unwrap() == &expected_params
			}),
		)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_events(1, Some(2)).await;

	assert!(result.is_ok());
	let events = result.unwrap();
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].contract_id,
		"CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK"
	);
}

#[tokio::test]
async fn test_get_events_invalid_sequence_range() {
	let mock_stellar = MockStellarTransportClient::new();
	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_events(2, Some(1)).await;
	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Invalid input parameters for Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::InvalidInput(ctx)) if ctx.to_string().contains("start_sequence 2 cannot be greater than end_sequence 1")
	),);
}

#[tokio::test]
async fn test_get_events_failed_to_parse_event() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with invalid event data
	let mock_response = json!({
		"result": {
			"events": [{
				"type": "contract",
				"ledger": 1,
				// Missing required fields like contractId, txHash, topic, etc.
			}]
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getEvents"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_events(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err.to_string().contains("Failed to parse event response"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::ResponseParseError { .. })
	),);
}

#[tokio::test]
async fn test_get_events_outside_of_rpc_retention_window() {
	let mut mock_stellar = MockStellarTransportClient::new();

	let start_block = 57317319; // Example start block outside retention window
	let end_block = 57369158; // Example end block within retention window
	const ERROR_CODE: i64 = -32600;

	let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"error": {
				"code": ERROR_CODE,
				"message": format!("startLedger must be within the ledger range: {} - {}",
					start_block, end_block),
			}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getEvents"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_events(start_block, Some(end_block)).await;

	assert!(result.is_err());

	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getEvents"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::OutsideRetentionWindow { rpc_code, rpc_message, .. }) if
			*rpc_code == ERROR_CODE  &&
			rpc_message.contains("must be within the ledger range")
	),);
}

#[tokio::test]
async fn test_get_events_generic_rpc_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with a generic RPC error
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"error": {
			"code": -32603,
			"message": "Internal error"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getEvents"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_events(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getEvents"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::RpcError { .. })
	),);
}

#[tokio::test]
async fn test_get_events_unexpected_response_structure() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with unexpected structure
	let mock_response = json!({
		"result": {
			"unexpectedField": "This is not a valid event response"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getEvents"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_events(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err.to_string().contains("Failed to parse event response"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::UnexpectedResponseStructure { .. })
	),);
}

#[tokio::test]
async fn test_get_events_transport_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock a transport error
	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getEvents"), predicate::always())
		.times(1)
		.returning(move |_, _| {
			Err(TransportError::network(
				"Network error".to_string(),
				None,
				None,
			))
		});

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_events(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Failed to getEvents from Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<TransportError>()),
		Some(TransportError::Network { .. })
	),);
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with a sequence number
	let mock_response = json!({
		"result": {
			"sequence": 12345
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLatestLedger"), predicate::eq(None))
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 12345);
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_sequence() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with invalid sequence data
	let mock_response = json!({
		"result": {
			"sequence": null  // Invalid sequence number
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLatestLedger"), predicate::eq(None))
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Invalid sequence number"),
		"Expected RequestError, got: {}",
		err
	);
}

#[tokio::test]
async fn test_get_blocks_success() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Expected request parameters
	let expected_params = json!({
		"startLedger": 1,
		"pagination": {
			"limit": 200
		}
	});

	// Mock response with test blocks
	let mock_response = json!({
		"result": {
			"ledgers": [{
				"hash": "eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f",
				"sequence": 1,
				"ledgerCloseTime": "1734715051",
				"headerXdr": "7rdLzf1N4aCydT7zftdqX2lqbyLVvmi019t6lytyjI8AAAAWQPixe1qujAngNN6juk4FUzWVRLtWYq1j6JfrWHtZrgZASbkSU0LcZcZcBmgJQToSL1jBErbE19yJbWsdvvV3lgAAAABnZaarAAAAAAAAAAEAAAAA1XJp2WJQ90ltB5vi6DUSNu//6NOcLga/q7FCHxf8ZxwAAABA8zxSDB7+pvF+oUfEXtkotNHcDap2E9twRpF2BFNPBtpjsQyHSdyibgO/Zol3dZ2DowRXqCDKfmyO3WcFhg4dDnSfbgFDmEblrTO8E4aZ813/jb/Jr8CGCJsw3w+DZXntDVWSi3471pDUgw9R3o22ihLr2wuJqnKIojDJOMP4TD4AAqJUDeC2s6dkAAAAAAAVcNXL9AAAAAAAAAAAAAAFxAAAAGQATEtAAAAAyJpvZ4eRACQmdDVhS1+hh7f5TRUfAwLJ8C6jA9pOwfMCTemSRlpagztU58RRpLkY7L+bZMUfDYxXuOqzEqA2RpnVKQFrYckWSJM7MLYpY8tunqC8rbmY2zy4CGxO8imyDgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
				"metadataXdr": "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"
			}],
			"cursor": null
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(
			predicate::eq("getLedgers"),
			predicate::function(move |params: &Option<Value>| {
				params.as_ref().unwrap() == &expected_params
			}),
		)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_blocks(1, Some(2)).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
	match &blocks[0] {
		BlockType::Stellar(block) => {
			assert_eq!(block.sequence, 1);
			assert_eq!(
				block.hash,
				"eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f"
			);
		}
		_ => panic!("Expected Stellar block"),
	}
}

#[tokio::test]
async fn test_get_blocks_failed_to_parse() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with invalid block data
	let mock_response = json!({
		"result": {
			"ledgers": [{
				"sequence": 1,
				// Missing required fields like hash, headerXdr, etc.
			}]
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgers"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_blocks(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err.to_string().contains("Failed to parse ledger response"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::ResponseParseError { .. })
	),);
}

#[tokio::test]
async fn test_get_blocks_invalid_sequence_range() {
	let mock_stellar = MockStellarTransportClient::new();
	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_blocks(2, Some(1)).await;
	assert!(result.is_err());

	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Invalid input parameters for Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::InvalidInput(ctx)) if ctx.to_string().contains("start_block 2 cannot be greater than end_block 1")
	),);
}

#[tokio::test]
async fn test_get_blocks_outside_of_rpc_retention_window() {
	let mut mock_stellar = MockStellarTransportClient::new();

	let start_block = 57317319; // Example start block outside retention window
	let end_block = 57369158; // Example end block within retention window
	const ERROR_CODE: i64 = -32600;

	let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"error": {
				"code": ERROR_CODE,
				"message": format!("startLedger must be within the ledger range: {} - {}",
					start_block, end_block),
			}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgers"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);

	let result = client.get_blocks(start_block, Some(end_block)).await;

	assert!(result.is_err());

	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getLedgers"));

	// Check source error
	assert!(matches!(
		err.source().and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::OutsideRetentionWindow { rpc_code, rpc_message, .. }) if
			*rpc_code == ERROR_CODE  &&
			rpc_message.contains("must be within the ledger range")
	),);
}

#[tokio::test]
async fn test_get_blocks_generic_rpc_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with a generic RPC error
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"error": {
			"code": -32603,
			"message": "Internal error"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgers"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_blocks(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err
		.to_string()
		.contains("Soroban RPC reported an error during getLedgers"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::RpcError { .. })
	),);
}

#[tokio::test]
async fn test_get_blocks_unexpected_response_structure() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock response with unexpected structure
	let mock_response = json!({
		"result": {
			"unexpectedField": "This is not a valid ledger response"
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgers"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_blocks(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	assert!(err.to_string().contains("Failed to parse ledger response"));
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<StellarClientError>()),
		Some(StellarClientError::UnexpectedResponseStructure { .. })
	),);
}

#[tokio::test]
async fn test_get_blocks_transport_error() {
	let mut mock_stellar = MockStellarTransportClient::new();

	// Mock a transport error
	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgers"), predicate::always())
		.times(1)
		.returning(move |_, _| {
			Err(TransportError::network(
				"Network error".to_string(),
				None,
				None,
			))
		});

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client.get_blocks(1, Some(2)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();

	// Check anyhow context message
	assert!(err
		.to_string()
		.contains("Failed to getLedgers from Stellar RPC"));

	// Check source error
	assert!(matches!(
		err.source()
			.and_then(|e| e.downcast_ref::<TransportError>()),
		Some(TransportError::Network { .. })
	),);
}

#[tokio::test]
async fn test_get_contract_spec_success() {
	let mut mock_stellar = MockStellarTransportClient::new();

	let contract_data_xdr = "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";
	let contract_code_xdr = "AAAABwAAAAEAAAAAAAAAAAAAAEAAAAAFAAAAAwAAAAAAAAAEAAAAAAAAAAAAAAAEAAAABQAAAAAK2r5DjlOc9ad6/YGX+OJcgiyi0nupnY4OMbgLdADJAwAAAkYAYXNtAQAAAAEVBGACfn4BfmADfn5+AX5gAAF+YAAAAhkEAWwBMAAAAWwBMQAAAWwBXwABAWwBOAAAAwYFAgIDAwMFAwEAEAYZA38BQYCAwAALfwBBgIDAAAt/AEGAgMAACwc1BQZtZW1vcnkCAAlpbmNyZW1lbnQABQFfAAgKX19kYXRhX2VuZAMBC19faGVhcF9iYXNlAwIKpAEFCgBCjrrQr4bUOQuFAQIBfwJ+QQAhAAJAAkACQBCEgICAACIBQgIQgICAgABCAVINACABQgIQgYCAgAAiAkL/AYNCBFINASACQiCIpyEACyAAQQFqIgBFDQEgASAArUIghkIEhCICQgIQgoCAgAAaQoSAgICgBkKEgICAwAwQg4CAgAAaIAIPCwALEIaAgIAAAAsJABCHgICAAAALAwAACwIACwBzDmNvbnRyYWN0c3BlY3YwAAAAAAAAAEBJbmNyZW1lbnQgaW5jcmVtZW50cyBhbiBpbnRlcm5hbCBjb3VudGVyLCBhbmQgcmV0dXJucyB0aGUgdmFsdWUuAAAACWluY3JlbWVudAAAAAAAAAAAAAABAAAABAAeEWNvbnRyYWN0ZW52bWV0YXYwAAAAAAAAABYAAAAAAG8OY29udHJhY3RtZXRhdjAAAAAAAAAABXJzdmVyAAAAAAAABjEuODYuMAAAAAAAAAAAAAhyc3Nka3ZlcgAAAC8yMi4wLjcjMjExNTY5YWE0OWM4ZDg5Njg3N2RmY2ExZjJlYjRmZTkwNzExMjFjOAAAAA==";

	// First request for contract instance
	let first_expected_params = json!({
		"keys": ["AAAABgAAAAG7Z/F6Fegc3zjeTq1eka8P+svGVgv1fsg8JMJQacQ0TgAAABQAAAAB"],  // Replace with actual ledger key XDR
		"xdrFormat": "base64"
	});

	let first_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_data_xdr
			}]
		}
	});

	// Second request for contract code
	let second_expected_params = json!({
		"keys": ["AAAAB7VLo3t7t91pp3Wcqp7scOnhNhW6OwCfwjxGJq6d/6J/"],  // Replace with actual ledger key XDR
		"xdrFormat": "base64"
	});

	let second_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_code_xdr
			}]
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(
			predicate::eq("getLedgerEntries"),
			predicate::function(move |params: &Option<Value>| {
				params.as_ref().unwrap() == &first_expected_params
			}),
		)
		.times(1)
		.returning(move |_, _| Ok(first_response.clone()));

	mock_stellar
		.expect_send_raw_request()
		.with(
			predicate::eq("getLedgerEntries"),
			predicate::function(move |params: &Option<Value>| {
				params.as_ref().unwrap() == &second_expected_params
			}),
		)
		.times(1)
		.returning(move |_, _| Ok(second_response.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_ok());
	let spec = result.unwrap();
	match spec {
		ContractSpec::Stellar(stellar_spec) => {
			let stellar_spec = StellarFormattedContractSpec::from(stellar_spec);
			assert!(
				!stellar_spec.functions.is_empty(),
				"Should have at least one function in spec"
			);
		}
		_ => panic!("Expected Stellar contract spec"),
	}
}

#[tokio::test]
async fn test_get_contract_spec_invalid_response() {
	let mut mock_stellar = MockStellarTransportClient::new();
	let contract_data_xdr = "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";

	// Mock invalid response
	let invalid_response = json!({
		"result": {
			"entries": []
		}
	});

	let valid_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_data_xdr
			}]
		}
	});

	let invalid_response_code = json!({
		"result": {
			"entries": []
		}
	});

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgerEntries"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(invalid_response.clone()));

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgerEntries"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(valid_response.clone()));

	mock_stellar
		.expect_send_raw_request()
		.with(predicate::eq("getLedgerEntries"), predicate::always())
		.times(1)
		.returning(move |_, _| Ok(invalid_response_code.clone()));

	let client = StellarClient::new_with_transport(mock_stellar);
	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Failed to get contract data XDR"),
		"Expected XDR error, got: {}",
		err
	);

	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		err.to_string().contains("Failed to get contract code XDR"),
		"Expected XDR error, got: {}",
		err
	);
}
