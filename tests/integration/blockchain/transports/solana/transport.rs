use mockall::predicate;
use openzeppelin_monitor::services::blockchain::{
	BlockChainClient, SolanaClient, SolanaClientTrait,
};
use serde_json::{json, Value};

use crate::integration::mocks::MockSolanaTransportClient;

/// Helper function to create a mock Solana transaction
fn create_mock_transaction(slot: u64, signature: &str) -> Value {
	json!({
		"slot": slot,
		"transaction": {
			"signatures": [signature],
			"message": {
				"accountKeys": [
					"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
					"11111111111111111111111111111111"
				],
				"recentBlockhash": "EkSnNWid2cvwEVnVx9aBqawnmiCNiDgp3gUdkDPTKN1N",
				"instructions": [
					{
						"programIdIndex": 0,
						"accounts": [1],
						"data": "3Bxs3zrfFUZbEPqZ"
					}
				]
			}
		},
		"meta": {
			"err": null,
			"fee": 5000,
			"preBalances": [1000000, 2000000],
			"postBalances": [995000, 2000000],
			"innerInstructions": [],
			"logMessages": [
				"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]",
				"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success"
			],
			"preTokenBalances": [],
			"postTokenBalances": [],
			"rewards": []
		},
		"blockTime": 1234567890
	})
}

/// Helper function to create a mock Solana block
fn create_mock_block(slot: u64) -> Value {
	json!({
		"blockhash": format!("{}mock", slot),
		"previousBlockhash": format!("{}prev", slot - 1),
		"parentSlot": slot - 1,
		"transactions": [
			{
				"transaction": {
					"signatures": [format!("sig{}", slot)],
					"message": {
						"accountKeys": ["key1", "key2"],
						"recentBlockhash": format!("{}mock", slot),
						"instructions": []
					}
				},
				"meta": {
					"err": null,
					"fee": 5000,
					"preBalances": [1000000],
					"postBalances": [995000],
					"innerInstructions": [],
					"logMessages": [],
					"preTokenBalances": [],
					"postTokenBalances": [],
					"rewards": []
				}
			}
		],
		"blockTime": 1234567890 + slot as i64,
		"blockHeight": slot
	})
}

/// Helper function to create mock signature info
fn create_mock_signature_info(slot: u64, signature: &str) -> Value {
	json!({
		"signature": signature,
		"slot": slot,
		"err": null,
		"blockTime": 1234567890 + slot as i64,
		"confirmationStatus": "finalized"
	})
}

/// Helper function to create mock account info
fn create_mock_account_info() -> Value {
	json!({
		"lamports": 1000000,
		"owner": "11111111111111111111111111111111",
		"data": ["", "base64"],
		"executable": false,
		"rentEpoch": 361
	})
}

// ============================================================================
// get_latest_block_number tests
// ============================================================================

#[tokio::test]
async fn test_get_latest_block_number_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": 123456789,
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(predicate::eq("getSlot"), predicate::always())
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_latest_block_number().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 123456789);
}

#[tokio::test]
async fn test_get_latest_block_number_missing_result() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Invalid slot number"));
}

#[tokio::test]
async fn test_get_latest_block_number_invalid_response() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": "not_a_number",
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_latest_block_number().await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Invalid slot number"));
}

// ============================================================================
// get_transaction tests
// ============================================================================

#[tokio::test]
async fn test_get_transaction_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let signature = "5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F";
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": create_mock_transaction(123456789, signature),
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(
			predicate::eq("getTransaction"),
			predicate::function(move |params: &Option<Vec<Value>>| {
				if let Some(p) = params {
					p.len() == 2 && p[0].as_str() == Some(signature)
				} else {
					false
				}
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction(signature.to_string()).await;

	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_some());
	assert_eq!(tx.unwrap().signature(), signature);
}

#[tokio::test]
async fn test_get_transaction_not_found() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": null,
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction("nonexistent".to_string()).await;

	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_none());
}

#[tokio::test]
async fn test_get_transaction_missing_result() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction("test_sig".to_string()).await;

	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_get_transaction_parse_failure() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"slot": 123,
			"transaction": {
				"signatures": [],
				"message": {
					"accountKeys": [],
					"recentBlockhash": "",
					"instructions": []
				}
			},
			"meta": null
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction("test_sig".to_string()).await;

	// Transaction with empty signature defaults to "" and succeeds
	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_some());
	assert_eq!(tx.unwrap().signature(), "");
}

// ============================================================================
// get_transactions tests
// ============================================================================

#[tokio::test]
async fn test_get_transactions_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let slot = 123456789u64;
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": create_mock_block(slot),
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(
			predicate::eq("getBlock"),
			predicate::function(move |params: &Option<Vec<Value>>| {
				if let Some(p) = params {
					!p.is_empty() && p[0].as_u64() == Some(slot)
				} else {
					false
				}
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(slot).await;

	assert!(result.is_ok());
	let transactions = result.unwrap();
	assert_eq!(transactions.len(), 1);
}

#[tokio::test]
async fn test_get_transactions_empty_block() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let slot = 123456789u64;
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"blockhash": "test",
			"previousBlockhash": "prev",
			"parentSlot": slot - 1,
			"transactions": [],
			"blockTime": 1234567890,
			"blockHeight": slot
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(slot).await;

	assert!(result.is_ok());
	let transactions = result.unwrap();
	assert_eq!(transactions.len(), 0);
}

#[tokio::test]
async fn test_get_transactions_parse_failure() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"blockhash": "test",
			"previousBlockhash": "prev",
			"parentSlot": 123456788,
			"transactions": [
				{
					// This transaction has the transaction field but it's empty
					"transaction": {
						"signatures": ["somesig"],
						"message": {
							"accountKeys": [],
							"recentBlockhash": "",
							"instructions": []
						}
					},
					"meta": null
				}
			],
			"blockTime": 1234567890,
			"blockHeight": 123456789
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	// Even with minimal fields, the transaction should parse successfully
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

// ============================================================================
// get_blocks tests
// ============================================================================

#[tokio::test]
async fn test_get_single_block() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let slot = 123456789u64;
	// For single block, get_blocks doesn't call getBlocks, it directly fetches the block using a for loop
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": create_mock_block(slot),
		"id": 1
	});
	mock_solana
		.expect_send_raw_request()
		.times(1)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_blocks(slot, None).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_multiple_blocks() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let start_slot = 123456789u64;
	let end_slot = 123456791u64;

	// get_blocks uses a for loop, so all calls are on the parent mock
	// Call 1: getBlocks to get list of slots
	// Calls 2-4: getBlock for each of the 3 slots
	mock_solana
		.expect_send_raw_request()
		.times(4) // 1 for getBlocks + 3 for getBlock
		.returning(move |method: &str, params: Option<Vec<Value>>| {
			if method == "getBlocks" {
				Ok(json!({
					"jsonrpc": "2.0",
					"result": [start_slot, start_slot + 1, end_slot],
					"id": 1
				}))
			} else {
				// getBlock - extract slot from params
				let slot = params
					.as_ref()
					.and_then(|p| p.first())
					.and_then(|v| v.as_u64())
					.unwrap_or(start_slot);
				Ok(json!({
					"jsonrpc": "2.0",
					"result": create_mock_block(slot),
					"id": 1
				}))
			}
		});

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_blocks(start_slot, Some(end_slot)).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 3);
}

#[tokio::test]
async fn test_get_blocks_missing_result() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Single block doesn't call getBlocks, goes straight to getBlock
	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1
	});
	mock_solana
		.expect_send_raw_request()
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_blocks(123456789, None).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	let err_str = err.to_string();
	assert!(err_str.contains("Failed to parse block"));
}

#[tokio::test]
async fn test_get_blocks_parse_failure() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"blockhash": "test",
			// Missing required fields
		},
		"id": 1
	});
	mock_solana
		.expect_send_raw_request()
		.times(1)
		.returning(move |_, _| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_blocks(123456789, None).await;

	// Missing previousBlockhash is fine (defaults to empty string)
	// Missing parentSlot is fine (defaults to 0)
	// Block should parse successfully
	assert!(result.is_ok());
}

// ============================================================================
// get_signatures_for_address_with_info tests
// ============================================================================

#[tokio::test]
async fn test_get_signatures_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": [
			create_mock_signature_info(123456789, "sig1"),
			create_mock_signature_info(123456790, "sig2")
		],
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(
			predicate::eq("getSignaturesForAddress"),
			predicate::function(move |params: &Option<Vec<Value>>| {
				if let Some(p) = params {
					!p.is_empty() && p[0].as_str() == Some(address)
				} else {
					false
				}
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client
		.get_signatures_for_address_with_info(address.to_string(), Some(10), Some(123456789), None)
		.await;

	assert!(result.is_ok());
	let signatures = result.unwrap();
	assert_eq!(signatures.len(), 2);
	assert_eq!(signatures[0].signature, "sig1");
	assert_eq!(signatures[0].slot, 123456789);
}

#[tokio::test]
async fn test_get_signatures_empty() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": [],
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client
		.get_signatures_for_address_with_info("test_address".to_string(), None, None, None)
		.await;

	assert!(result.is_ok());
	let signatures = result.unwrap();
	assert_eq!(signatures.len(), 0);
}

#[tokio::test]
async fn test_get_signatures_parse_failure() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": [
			{
				"invalid": "signature format"
			}
		],
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client
		.get_signatures_for_address_with_info("test_address".to_string(), None, None, None)
		.await;

	// filter_map silently filters out items that fail to parse
	assert!(result.is_ok());
	let signatures = result.unwrap();
	assert_eq!(signatures.len(), 0);
}

// ============================================================================
// get_account_info tests
// ============================================================================

#[tokio::test]
async fn test_get_account_info_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let pubkey = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"context": {
				"slot": 123456789
			},
			"value": create_mock_account_info()
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(
			predicate::eq("getAccountInfo"),
			predicate::function(move |params: &Option<Vec<Value>>| {
				if let Some(p) = params {
					!p.is_empty() && p[0].as_str() == Some(pubkey)
				} else {
					false
				}
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_account_info(pubkey.to_string()).await;

	assert!(result.is_ok());
	let account_result = result.unwrap();
	// get_account_info returns the whole result object (context + value)
	assert_eq!(account_result["value"]["lamports"], 1000000);
	assert_eq!(
		account_result["value"]["owner"],
		"11111111111111111111111111111111"
	);
}

#[tokio::test]
async fn test_get_account_info_not_found() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"context": {
				"slot": 123456789
			},
			"value": null
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_account_info("nonexistent".to_string()).await;

	// get_account_info returns the result with null value, doesn't error
	assert!(result.is_ok());
	let account_result = result.unwrap();
	assert!(account_result["value"].is_null());
}

#[tokio::test]
async fn test_get_account_info_parse_failure() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_account_info("test_pubkey".to_string()).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Invalid response structure"));
}

// ============================================================================
// Error handling tests
// ============================================================================

#[tokio::test]
async fn test_slot_unavailable_error() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Test with error code -32007 (slot skipped)
	let mock_response = json!({
		"jsonrpc": "2.0",
		"error": {
			"code": -32007,
			"message": "Slot 123456789 was skipped, or missing in long-term storage"
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	// The error message comes from SolanaClientError
	let err_str = err.to_string();
	assert!(err_str.contains("Solana RPC error") || err_str.contains("slot"));
}

#[tokio::test]
async fn test_block_not_available_error() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Test with error code -32004 (block cleaned up)
	let mock_response = json!({
		"jsonrpc": "2.0",
		"error": {
			"code": -32004,
			"message": "Block not available for slot 123456789"
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	// The error message comes from SolanaClientError
	let err_str = err.to_string();
	assert!(
		err_str.contains("Solana RPC error")
			|| err_str.contains("block")
			|| err_str.contains("Block")
	);
}

// ============================================================================
// Monitored addresses tests
// ============================================================================

#[tokio::test]
async fn test_monitored_addresses_builder_pattern() {
	let mock_solana = MockSolanaTransportClient::new();

	// Test builder pattern
	let addresses = vec![
		"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
		"11111111111111111111111111111111".to_string(),
	];
	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana)
		.with_monitored_addresses(addresses.clone());

	assert_eq!(client.monitored_addresses(), addresses.as_slice());
}

#[tokio::test]
async fn test_monitored_addresses_setter() {
	let mock_solana = MockSolanaTransportClient::new();

	let mut client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);

	// Initially empty
	assert!(client.monitored_addresses().is_empty());

	// Set addresses
	let addresses = vec!["TestAddress123".to_string()];
	client.set_monitored_addresses(addresses.clone());
	assert_eq!(client.monitored_addresses(), addresses.as_slice());

	// Update addresses
	let new_addresses = vec!["NewAddress456".to_string(), "AnotherAddress789".to_string()];
	client.set_monitored_addresses(new_addresses.clone());
	assert_eq!(client.monitored_addresses(), new_addresses.as_slice());

	// Clear addresses
	client.set_monitored_addresses(Vec::new());
	assert!(client.monitored_addresses().is_empty());
}

#[tokio::test]
async fn test_get_blocks_uses_optimized_mode_with_monitored_addresses() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let address = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
	let start_slot = 100u64;
	let end_slot = 105u64;

	// When monitored_addresses is set, get_blocks should call getSignaturesForAddress
	// instead of getBlocks + getBlock
	mock_solana
		.expect_send_raw_request()
		.withf(|method: &str, _params: &Option<Vec<Value>>| method == "getSignaturesForAddress")
		.times(1)
		.returning(move |_: &str, _: Option<Vec<Value>>| {
			Ok(json!({
				"jsonrpc": "2.0",
				"result": [
					create_mock_signature_info(start_slot, "sig1"),
					create_mock_signature_info(end_slot, "sig2")
				],
				"id": 1
			}))
		});

	// Then it should fetch individual transactions
	mock_solana
		.expect_send_raw_request()
		.withf(|method: &str, _params: &Option<Vec<Value>>| method == "getTransaction")
		.times(2)
		.returning(move |_: &str, params: Option<Vec<Value>>| {
			let sig = params
				.as_ref()
				.and_then(|p| p.first())
				.and_then(|v| v.as_str())
				.unwrap_or("unknown");
			let slot = if sig == "sig1" { start_slot } else { end_slot };
			Ok(json!({
				"jsonrpc": "2.0",
				"result": create_mock_transaction(slot, sig),
				"id": 1
			}))
		});

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana)
		.with_monitored_addresses(vec![address.to_string()]);

	let result = client.get_blocks(start_slot, Some(end_slot)).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	// Should have 2 virtual blocks (one for each slot with transactions)
	assert_eq!(blocks.len(), 2);
}

#[tokio::test]
async fn test_get_blocks_uses_standard_mode_without_monitored_addresses() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let slot = 123456789u64;

	// Without monitored_addresses, should use standard getBlock approach
	mock_solana
		.expect_send_raw_request()
		.withf(|method: &str, _params: &Option<Vec<Value>>| method == "getBlock")
		.times(1)
		.returning(move |_: &str, _: Option<Vec<Value>>| {
			Ok(json!({
				"jsonrpc": "2.0",
				"result": create_mock_block(slot),
				"id": 1
			}))
		});

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	// No monitored addresses set

	let result = client.get_blocks(slot, None).await;

	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_blocks_for_addresses_empty_addresses() {
	let mock_solana = MockSolanaTransportClient::new();

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);

	// Empty addresses should return empty result without making any RPC calls
	let result = SolanaClientTrait::get_blocks_for_addresses(&client, &[], 100, Some(200)).await;

	assert!(result.is_ok());
	assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn test_get_blocks_for_addresses_no_signatures_found() {
	let mut mock_solana = MockSolanaTransportClient::new();

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| {
			Ok(json!({
				"jsonrpc": "2.0",
				"result": [],
				"id": 1
			}))
		});

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);

	let result = SolanaClientTrait::get_blocks_for_addresses(
		&client,
		&["TestAddress".to_string()],
		100,
		Some(200),
	)
	.await;

	assert!(result.is_ok());
	assert!(result.unwrap().is_empty());
}

// ============================================================================
// Additional RPC error code tests
// ============================================================================

#[tokio::test]
async fn test_slot_skipped_error_code_32009() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Test with error code -32009 (slot skipped - alternative code)
	let mock_response = json!({
		"jsonrpc": "2.0",
		"error": {
			"code": -32009,
			"message": "Slot 123456789 was skipped"
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	assert!(result.is_err());
}

#[tokio::test]
async fn test_generic_rpc_error() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Test with a generic RPC error code
	let mock_response = json!({
		"jsonrpc": "2.0",
		"error": {
			"code": -32600,
			"message": "Invalid Request"
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	let err_str = err.to_string();
	assert!(err_str.contains("RPC") || err_str.contains("error"));
}

#[tokio::test]
async fn test_get_blocks_invalid_range() {
	let mock_solana = MockSolanaTransportClient::new();

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);

	// start_block > end_block should fail
	let result = client.get_blocks(200, Some(100)).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	let err_str = format!("{:?}", err); // Use debug format for full chain
									 // Error should mention the validation failure
	assert!(err_str.contains("cannot be greater") || err_str.contains("Invalid input"));
}

// ============================================================================
// get_program_accounts tests
// ============================================================================

#[tokio::test]
async fn test_get_program_accounts_success() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": [
			{
				"pubkey": "Account1",
				"account": create_mock_account_info()
			},
			{
				"pubkey": "Account2",
				"account": create_mock_account_info()
			}
		],
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.with(
			predicate::eq("getProgramAccounts"),
			predicate::function(move |params: &Option<Vec<Value>>| {
				if let Some(p) = params {
					!p.is_empty() && p[0].as_str() == Some(program_id)
				} else {
					false
				}
			}),
		)
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_program_accounts(program_id.to_string()).await;

	assert!(result.is_ok());
	let accounts = result.unwrap();
	assert_eq!(accounts.len(), 2);
}

#[tokio::test]
async fn test_get_program_accounts_empty() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": [],
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_program_accounts("TestProgram".to_string()).await;

	assert!(result.is_ok());
	assert!(result.unwrap().is_empty());
}

#[tokio::test]
async fn test_get_program_accounts_invalid_response() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": "not_an_array",
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_program_accounts("TestProgram".to_string()).await;

	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("Invalid"));
}

// ============================================================================
// Parsing edge case tests
// ============================================================================

#[tokio::test]
async fn test_parse_transaction_with_null_meta() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"slot": 123,
			"transaction": {
				"signatures": ["sig1"],
				"message": {
					"accountKeys": ["key1"],
					"recentBlockhash": "hash1",
					"instructions": []
				}
			},
			"meta": null
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction("sig1".to_string()).await;

	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_some());
	let tx = tx.unwrap();
	assert_eq!(tx.signature(), "sig1");
	// With null meta, is_success should be determined appropriately
}

#[tokio::test]
async fn test_parse_block_with_null_result() {
	let mut mock_solana = MockSolanaTransportClient::new();

	// Some slots are skipped and return null result
	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": null,
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transactions(123456789).await;

	// Should fail because block data is null
	assert!(result.is_err());
	let err = result.unwrap_err();
	let err_str = format!("{:?}", err); // Use debug format to get full error chain
									 // Error should indicate the slot and that parsing failed
	assert!(err_str.contains("parse") || err_str.contains("slot") || err_str.contains("block"));
}

#[tokio::test]
async fn test_parse_transaction_with_parsed_instructions() {
	let mut mock_solana = MockSolanaTransportClient::new();

	let mock_response = json!({
		"jsonrpc": "2.0",
		"result": {
			"slot": 123,
			"transaction": {
				"signatures": ["sig1"],
				"message": {
					"accountKeys": [
						{"pubkey": "key1", "signer": true, "writable": true}
					],
					"recentBlockhash": "hash1",
					"instructions": [
						{
							"programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
							"program": "spl-token",
							"parsed": {
								"type": "transfer",
								"info": {
									"source": "addr1",
									"destination": "addr2",
									"amount": "1000000"
								}
							}
						}
					]
				}
			},
			"meta": {
				"err": null,
				"fee": 5000,
				"preBalances": [1000000],
				"postBalances": [995000],
				"logMessages": ["Program log: Transfer"]
			}
		},
		"id": 1
	});

	mock_solana
		.expect_send_raw_request()
		.returning(move |_: &str, _: Option<Vec<Value>>| Ok(mock_response.clone()));

	let client = SolanaClient::<MockSolanaTransportClient>::new_with_transport(mock_solana);
	let result = client.get_transaction("sig1".to_string()).await;

	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_some());
	let tx = tx.unwrap();
	// Verify transaction was parsed correctly
	assert_eq!(tx.signature(), "sig1");
}
