use alloy::{
	primitives::{Address, U64},
	rpc::types::Index,
};
use mockall::predicate;
use openzeppelin_monitor::services::blockchain::{
	BlockChainClient, EvmClient, EvmClientTrait, TransportError,
};
use serde_json::{json, Value};

use crate::integration::mocks::MockEVMTransportClient;

fn create_mock_block(number: u64) -> Value {
	json!({
		"number": format!("0x{:x}", number),
		"hash": format!("0x{:064x}", number),  // 32 bytes
		"parentHash": format!("0x{:064x}", number.wrapping_sub(1)),  // 32 bytes
		"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",  // 32 bytes
		"miner": format!("0x{:040x}", number),  // 20 bytes
		"stateRoot": format!("0x{:064x}", number),  // 32 bytes
		"transactionsRoot": format!("0x{:064x}", number),  // 32 bytes
		"receiptsRoot": "0xda7db7fb15f4f721422a529e5b60705d4bc920d396e4de6c9576f48a211262fa",
		"gasUsed": "0xd3f56e",
		"gasLimit": "0x1c9c380",
		"baseFeePerGas": "0x1c9a6d183",
		"extraData": "0x6265617665726275696c642e6f7267",
		"logsBloom": "0x1165d3fc10c76b56f2d09257f1e816195bf060be2c841105be9f737a81fbcc270592016f9b6032388f8357a43f05e7d44a3900f8aa67ff2c6f753d40432cbda1e8f6cfeec35809eff9da6b7e928cd8b8acf5a8830774cad4615eec648264efffdf0bdf65b700647aa667c8ba8fbde80bb419240ebb17f6e61afb7c569f5dd86406cdca5fa3dae5ed28dcb3cb1b30042663734ff1eb35a6fd4e65137769bb652bb7dd27f2e68272186ff213c308175432e49ed5e77defb476b9746e2f0feba1661f98373f080e57d7438ed07eeaefd8a784dc2614de28587673dfb07f32cbf4d60d772d0b01209caa08d4c2afe42486e3077cf4b05fffa9d13dcb8de4611875df",
		"timestamp": "0x674c0aef",
		"difficulty": "0x0",
		"totalDifficulty": "0xc70d815d562d3cfa955",
		"sealFields": [],
		"uncles": [],
		"transactions": [],
		"size": "0xffa5",
		"mixHash": format!("0x{:064x}", number),  // 32 bytes
		"nonce": format!("0x{:016x}", number),  // 8 bytes
	})
}

#[tokio::test]
async fn test_get_logs_for_blocks_implementation() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Expected request parameters
	let expected_params = json!([{
		"fromBlock": "0x1",
		"toBlock": "0xa",
		"address": vec!["0x1234567890123456789012345678901234567890"]
	}]);

	// Mock response with some test logs
	let mock_response = json!({
		"result": [{
			"address": "0x1234567890123456789012345678901234567890",
			"topics": [],
			"data": "0x",
			"blockNumber": "0x1",
			"blockHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
			"transactionIndex": "0x0",
			"logIndex": "0x0",
			"transactionLogIndex": "0x0",
			"removed": false
		}]
	});

	mock_evm
		.expect_send_raw_request()
		.with(
			predicate::eq("eth_getLogs"),
			predicate::eq(Some(expected_params.as_array().unwrap().to_vec())),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client
		.get_logs_for_blocks(
			1,
			10,
			Some(vec![
				"0x1234567890123456789012345678901234567890".to_string()
			]),
		)
		.await;

	assert!(result.is_ok());
	let logs = result.unwrap();
	assert_eq!(logs.len(), 1);
	assert_eq!(logs[0].block_number.unwrap(), U64::from(1));
	assert_eq!(
		logs[0].address,
		"0x1234567890123456789012345678901234567890"
			.parse::<Address>()
			.unwrap()
	);
}

#[tokio::test]
async fn test_get_logs_for_blocks_missing_result() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_logs_for_blocks(1, 10, None).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_logs_for_blocks_invalid_format() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response with invalid log format
	let mock_response = json!({
		"result": [{
			"invalid_field": "this should fail parsing"
		}]
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_logs_for_blocks(1, 10, None).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse logs"));
}

#[tokio::test]
async fn test_get_logs_for_blocks_alloy_error() {
	let mut mock_evm = MockEVMTransportClient::new();

	mock_evm
		.expect_send_raw_request()
		.returning(|_: &str, _: Option<Vec<Value>>| {
			Err(TransportError::request_serialization(
				"Alloy error",
				None,
				None,
			))
		});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_logs_for_blocks(1, 10, None).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_get_transaction_receipt_success() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Expected request parameters for a transaction hash
	let expected_params =
		json!(["0x0000000000000000000000000000000000000000000000000000000000000001"]);

	// Mock response with a valid transaction receipt
	let mock_response = json!({
		"result": {
			"transactionHash": "0x0000000000000000000000000000000000000000000000000000000000000001",
			"transactionIndex": "0x1",
			"blockHash": "0x0000000000000000000000000000000000000000000000000000000000000002",
			"blockNumber": "0x1",
			"from": "0x1234567890123456789012345678901234567890",
			"to": "0x1234567890123456789012345678901234567891",
			"cumulativeGasUsed": "0x1",
			"gasUsed": "0x1",
			"contractAddress": null,
			"logs": [],
			"status": "0x1",
			"logsBloom": "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			"effectiveGasPrice": "0x1",
			"type": "0x0"
		}
	});

	mock_evm
		.expect_send_raw_request()
		.with(
			predicate::eq("eth_getTransactionReceipt"),
			predicate::eq(Some(expected_params.as_array().unwrap().to_vec())),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_ok());
	let receipt = result.unwrap();
	assert_eq!(receipt.block_number.unwrap(), U64::from(1));
	assert_eq!(receipt.transaction_index, Index::from(1));
}

#[tokio::test]
async fn test_get_transaction_receipt_not_found() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response for a non-existent transaction
	let mock_response = json!({
		"result": null
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Transaction receipt not found"));
}

#[tokio::test]
async fn test_get_transaction_receipt_invalid_hash() {
	let mock_evm = MockEVMTransportClient::new();
	// We don't need to mock any response since the validation will fail before making the
	// request
	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	// Test with an invalid hash format
	let result = client
		.get_transaction_receipt("invalid_hash".to_string())
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Invalid transaction hash"));
	assert!(err.to_string().contains("Invalid character"));
}

#[tokio::test]
async fn test_get_transaction_receipt_missing_result() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_transaction_receipt_parse_failure() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response with malformed receipt data
	let mock_response = json!({
		"result": {
			"transactionHash": "invalid_hash",
			"blockNumber": "not_a_hex",
			// Missing required fields
		}
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_err());

	let err = result.unwrap_err();
	assert!(err
		.to_string()
		.contains("Failed to parse transaction receipt"));
}

#[tokio::test]
async fn test_get_latest_block_number_success() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response with a block number
	let mock_response = json!({
		"result": "0x1234"
	});

	mock_evm
		.expect_send_raw_request()
		.with(predicate::eq("eth_blockNumber"), predicate::always())
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 0x1234);
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_response() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response with invalid format
	let mock_response = json!({
		"result": "invalid_hex"
	});

	mock_evm
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block number"));
}

#[tokio::test]
async fn test_get_latest_block_number_missing_result() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_evm
		.expect_send_raw_request()
		.with(predicate::eq("eth_blockNumber"), predicate::always())
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_single_block() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	mock_evm.expect_clone().times(1).returning(|| {
		let mut new_mock = MockEVMTransportClient::new();
		// Mock successful block response
		let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"result": create_mock_block(1)
		});
		new_mock
			.expect_send_raw_request()
			.with(
				predicate::eq("eth_getBlockByNumber"),
				predicate::function(|params: &Option<Vec<Value>>| match params {
					Some(p) => p == &vec![json!("0x1"), json!(true)],
					None => false,
				}),
			)
			.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

		new_mock
			.expect_clone()
			.returning(MockEVMTransportClient::new);
		new_mock
	});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_multiple_blocks() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	mock_evm.expect_clone().times(3).returning(|| {
		let mut new_mock = MockEVMTransportClient::new();
		new_mock.expect_send_raw_request().times(1).returning(
			move |_: &str, params: Option<Vec<Value>>| {
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
					"result": create_mock_block(block_num)
				}))
			},
		);

		new_mock
			.expect_clone()
			.returning(MockEVMTransportClient::new);
		new_mock
	});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	let result = client.get_blocks(1, Some(3)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);
}

#[tokio::test]
async fn test_get_blocks_missing_result() {
	let mut mock_evm = MockEVMTransportClient::new();

	// Mock response without result field
	mock_evm.expect_clone().returning(|| {
		let mut new_mock = MockEVMTransportClient::new();
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
			.returning(MockEVMTransportClient::new);
		new_mock
	});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Missing 'result' field"));
}

#[tokio::test]
async fn test_get_blocks_null_result() {
	let mut mock_evm = MockEVMTransportClient::new();

	mock_evm.expect_clone().returning(|| {
		let mut new_mock = MockEVMTransportClient::new();
		// Mock response with null result
		let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"result": null
		});
		new_mock
			.expect_send_raw_request()
			.returning(move |_, _| Ok(mock_response.clone()));
		new_mock
			.expect_clone()
			.returning(MockEVMTransportClient::new);
		new_mock
	});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Block not found"));
}

#[tokio::test]
async fn test_get_blocks_parse_failure() {
	let mut mock_evm = MockEVMTransportClient::new();

	mock_evm.expect_clone().returning(|| {
		let mut new_mock = MockEVMTransportClient::new();
		// Mock response with malformed block data
		let mock_response = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"result": {
				"number": "not_a_hex_number",
				"hash": "invalid_hash",
				// Missing required fields
			}
		});
		new_mock
			.expect_send_raw_request()
			.returning(move |_, _| Ok(mock_response.clone()));
		new_mock
			.expect_clone()
			.returning(MockEVMTransportClient::new);
		new_mock
	});

	let client = EvmClient::<MockEVMTransportClient>::new_with_transport(mock_evm);

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to parse block"));
}
