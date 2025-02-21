use mockall::predicate;
use openzeppelin_monitor::{
	models::{BlockChainType, Network, RpcUrl},
	services::blockchain::{BlockChainClient, BlockChainError, EvmClient, EvmClientTrait},
};
use serde_json::json;
use web3::types::H160;

use crate::integration::mocks::MockWeb3TransportClient;

fn create_mock_network() -> Network {
	Network {
		name: "mock".to_string(),
		slug: "mock".to_string(),
		network_type: BlockChainType::EVM,
		rpc_urls: vec![RpcUrl {
			url: "http://localhost:8545".to_string(),
			type_: "rpc".to_string(),
			weight: 100,
		}],
		cron_schedule: "*/5 * * * * *".to_string(),
		confirmation_blocks: 1,
		store_blocks: Some(false),
		chain_id: Some(1),
		network_passphrase: None,
		block_time_ms: 1000,
		max_past_blocks: None,
	}
}

#[tokio::test]
async fn test_get_logs_for_blocks_implementation() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Expected request parameters
	let expected_params = json!([{
		"fromBlock": "0x1",
		"toBlock": "0xa"
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

	mock_web3
		.expect_send_raw_request()
		.with(
			predicate::eq("eth_getLogs"),
			predicate::eq(expected_params.as_array().unwrap().to_vec()),
		)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_logs_for_blocks(1, 10).await;

	assert!(result.is_ok());
	let logs = result.unwrap();
	assert_eq!(logs.len(), 1);
	assert_eq!(logs[0].block_number.unwrap().as_u64(), 1);
	assert_eq!(
		logs[0].address,
		"0x1234567890123456789012345678901234567890"
			.parse::<H160>()
			.unwrap()
	);
}

#[tokio::test]
async fn test_get_logs_for_blocks_missing_result() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_logs_for_blocks(1, 10).await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => assert!(msg.contains("Missing 'result' field")),
		_ => panic!("Expected RequestError"),
	}
}

#[tokio::test]
async fn test_get_logs_for_blocks_invalid_format() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response with invalid log format
	let mock_response = json!({
		"result": [{
			"invalid_field": "this should fail parsing"
		}]
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_logs_for_blocks(1, 10).await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => assert!(msg.contains("Failed to parse logs")),
		_ => panic!("Expected RequestError"),
	}
}

#[tokio::test]
async fn test_get_logs_for_blocks_web3_error() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	mock_web3
		.expect_send_raw_request()
		.returning(|_, _| Err(BlockChainError::RequestError("Web3 error".into())));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_logs_for_blocks(1, 10).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_get_transaction_receipt_success() {
	let mut mock_web3 = MockWeb3TransportClient::new();

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

	mock_web3
		.expect_send_raw_request()
		.with(
			predicate::eq("eth_getTransactionReceipt"),
			predicate::eq(expected_params.as_array().unwrap().to_vec()),
		)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_ok());
	let receipt = result.unwrap();
	assert_eq!(receipt.block_number.unwrap().as_u64(), 1);
	assert_eq!(receipt.transaction_index.as_u64(), 1);
}

#[tokio::test]
async fn test_get_transaction_receipt_not_found() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response for a non-existent transaction
	let mock_response = json!({
		"result": null
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client
		.get_transaction_receipt(
			"0000000000000000000000000000000000000000000000000000000000000001".to_string(),
		)
		.await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => {
			assert!(msg.contains("Transaction receipt not found"))
		}
		_ => panic!("Expected RequestError"),
	}
}

#[tokio::test]
async fn test_get_transaction_receipt_invalid_hash() {
	let mock_web3 = MockWeb3TransportClient::new();
	// We don't need to mock any response since the validation will fail before making the
	// request
	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());

	// Test with an invalid hash format
	let result = client
		.get_transaction_receipt("invalid_hash".to_string())
		.await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::InternalError(msg) => {
			assert!(msg.contains("Invalid transaction hash"));
			assert!(msg.contains("invalid_hash"));
		}
		err => panic!("Expected InternalError, got {:?}", err),
	}
}

#[tokio::test]
async fn test_get_latest_block_number_success() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response with a block number
	let mock_response = json!({
		"result": "0x1234"
	});

	mock_web3
		.expect_send_raw_request()
		.with(predicate::eq("eth_blockNumber"), predicate::eq(vec![]))
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 0x1234);
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_response() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response with invalid format
	let mock_response = json!({
		"result": "invalid_hex"
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => assert!(msg.contains("Failed to parse block number")),
		_ => panic!("Expected RequestError"),
	}
}

#[tokio::test]
async fn test_get_latest_block_number_missing_result() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"id": 1,
		"jsonrpc": "2.0"
	});

	mock_web3
		.expect_send_raw_request()
		.with(predicate::eq("eth_blockNumber"), predicate::eq(vec![]))
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => {
			assert_eq!(msg, "Invalid response format: missing 'result' field");
		}
		err => panic!("Expected RequestError, got {:?}", err),
	}
}

#[tokio::test]
async fn test_get_single_block() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock successful block response
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"result": {
			"hash": "0x9432868b7fc57e85f0435ca3047f6a76add86f804b3c1af85647520061e30f80",
			"parentHash": "0x347f3343ea70f601585ce2ef2bda6aec23294533e921791b120610a40f387596",
			"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			"miner": "0x95222290dd7278aa3ddd389cc1e1d165cc4bafe5",
			"stateRoot": "0x5c002593439b2ea77587f67a2b6666ac2572ccf47ac40a181cb19cf7ceaf01a4",
			"transactionsRoot": "0xd7d0e9f64ca965a082f159ac45bae4fe4a4d13af1fa0cfffbe4bf2f2357d153d",
			"receiptsRoot": "0xda7db7fb15f4f721422a529e5b60705d4bc920d396e4de6c9576f48a211262fa",
			"number": "0x1451aca",
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
			"mixHash": "0x0bcd81326a16494c90dbb91a56c9760b698794ac8cfa13ddb62bd8b34ed8aa2a",
			"nonce": "0x0000000000000000",
		}
	});

	mock_web3
		.expect_send_raw_request()
		.with(
			predicate::eq("eth_getBlockByNumber"),
			predicate::function(|params: &Vec<serde_json::Value>| {
				params.len() == 2 && params[0] == json!("0x1") && params[1] == json!(true)
			}),
		)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());

	let result = client.get_blocks(1, None).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_multiple_blocks() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Create a mock response builder
	let create_mock_response = |block_num: u64| {
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"result": {
				"number": format!("0x{:x}", block_num),
				"hash": format!("0x{:064x}", block_num),  // 32 bytes
				"parentHash": format!("0x{:064x}", block_num.wrapping_sub(1)),  // 32 bytes
				"sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",  // 32 bytes
				"miner": format!("0x{:040x}", block_num),  // 20 bytes
				"stateRoot": format!("0x{:064x}", block_num),  // 32 bytes
				"transactionsRoot": format!("0x{:064x}", block_num),  // 32 bytes
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
				"mixHash": format!("0x{:064x}", block_num),  // 32 bytes
				"nonce": format!("0x{:016x}", block_num),  // 8 bytes
			}
		})
	};

	// Expect calls for blocks 1, 2, and 3
	mock_web3
		.expect_send_raw_request()
		.times(3)
		.returning(move |_, params| {
			let block_num =
				u64::from_str_radix(params[0].as_str().unwrap().trim_start_matches("0x"), 16)
					.unwrap();
			Ok(create_mock_response(block_num))
		});

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());

	let result = client.get_blocks(1, Some(3)).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);
}

#[tokio::test]
async fn test_get_blocks_missing_result() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response without result field
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::RequestError(msg) => {
			assert_eq!(msg, "Missing 'result' field");
		}
		err => panic!("Expected RequestError, got {:?}", err),
	}
}

#[tokio::test]
async fn test_get_blocks_null_result() {
	let mut mock_web3 = MockWeb3TransportClient::new();

	// Mock response with null result
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1,
		"result": null
	});

	mock_web3
		.expect_send_raw_request()
		.returning(move |_, _| Ok(mock_response.clone()));

	let client =
		EvmClient::<MockWeb3TransportClient>::new_with_transport(mock_web3, &create_mock_network());

	let result = client.get_blocks(1, None).await;
	assert!(result.is_err());
	match result.unwrap_err() {
		BlockChainError::BlockNotFound(block_num) => {
			assert_eq!(block_num, 1);
		}
		err => panic!("Expected BlockNotFound, got {:?}", err),
	}
}
