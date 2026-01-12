use mockito::Server;
use openzeppelin_monitor::{
	services::blockchain::{
		BlockchainTransport, RotatingTransport, SolanaCommitment, SolanaGetBlockConfig,
		SolanaGetTransactionConfig, SolanaTransportClient,
	},
	utils::RetryConfig,
};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::{json, Value};

use crate::integration::mocks::{
	create_solana_test_network_with_urls, create_solana_valid_server_mock_network_response,
};

#[tokio::test]
async fn test_client_creation() {
	let mut server = Server::new_async().await;
	let mock = create_solana_valid_server_mock_network_response(&mut server);
	let network = create_solana_test_network_with_urls(vec![&server.url()]);

	match SolanaTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server.url());
			mock.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}

	let network = create_solana_test_network_with_urls(vec!["invalid-url"]);

	match SolanaTransportClient::new(&network).await {
		Err(error) => {
			assert!(error.to_string().contains("All RPC URLs failed to connect"));
		}
		_ => panic!("Transport creation should fail"),
	}

	mock.assert();
}

#[tokio::test]
async fn test_client_creation_with_fallback() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	// Use the default retry config to determine expected attempts
	let expected_attempts = 1 + RetryConfig::default().max_retries;

	let mock = server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"getHealth"}"#)
		.with_header("content-type", "application/json")
		.with_status(500) // Simulate a failed request
		.expect(expected_attempts as usize)
		.create();

	let mock2 = create_solana_valid_server_mock_network_response(&mut server2);

	let network = create_solana_test_network_with_urls(vec![&server.url(), &server2.url()]);

	match SolanaTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server2.url());
			mock.assert();
			mock2.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}
}

#[tokio::test]
async fn test_client_update_client() {
	let mut server = Server::new_async().await;
	let server2 = Server::new_async().await;

	let mock1 = create_solana_valid_server_mock_network_response(&mut server);

	let network = create_solana_test_network_with_urls(vec![&server.url()]);

	let client = SolanaTransportClient::new(&network).await.unwrap();

	// Test successful update
	let result = client.update_client(&server2.url()).await;
	assert!(result.is_ok(), "Update to valid URL should succeed");

	assert_eq!(client.get_current_url().await, server2.url());

	// Test invalid URL update
	let result = client.update_client("invalid-url").await;
	assert!(result.is_err(), "Update with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("invalid-url"));

	// Verify mock was called the expected number of times
	mock1.assert();
}

#[tokio::test]
async fn test_client_try_connect() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;
	let server3 = Server::new_async().await;

	let mock = create_solana_valid_server_mock_network_response(&mut server);
	let mock2 = create_solana_valid_server_mock_network_response(&mut server2);

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.try_connect(&server2.url()).await;
	assert!(result.is_ok(), "Try connect should succeed");

	let result = client.try_connect("invalid-url").await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("Invalid URL"));

	let result = client.try_connect(&server3.url()).await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("Failed to connect"));

	mock.assert();
	mock2.assert();
}

#[tokio::test]
async fn test_send_raw_request() {
	let mut server = Server::new_async().await;

	// First, set up the network verification mock that's called during client creation
	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	// Then set up the test request mock with the correct field order
	let test_mock = server
		.mock("POST", "/")
		.match_body(
			r#"{"id":1,"jsonrpc":"2.0","method":"getSlot","params":{"commitment":"finalized"}}"#,
		)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":123456789,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	// Test with params
	let params = json!({"commitment": "finalized"});
	let result = client.send_raw_request("getSlot", Some(params)).await;

	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"], 123456789);

	// Verify both mocks were called
	network_mock.assert();
	test_mock.assert();

	// Test without params
	let no_params_mock = server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"getHealth","params":null}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":"ok","id":1}"#)
		.create();

	let result = client.send_raw_request::<Value>("getHealth", None).await;

	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"], "ok");
	no_params_mock.assert();
}

#[tokio::test]
async fn test_update_endpoint_manager_client() {
	let mut server = Server::new_async().await;

	// Set up initial response
	let initial_mock = create_solana_valid_server_mock_network_response(&mut server);
	let initial_request_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": 100, "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let mut client = SolanaTransportClient::new(&network).await.unwrap();

	// Test initial client
	let result = client
		.send_raw_request("getSlot", Some(json!({"commitment": "finalized"})))
		.await
		.unwrap();
	assert_eq!(result["result"], 100);

	// Set up mock for updated client
	let updated_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": 200, "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	// Create and update to new client
	let new_client = ClientBuilder::new(reqwest::Client::new())
		.with(RetryTransientMiddleware::new_with_policy(
			ExponentialBackoff::builder().build_with_max_retries(3),
		))
		.build();

	let result = client.update_endpoint_manager_client(new_client);
	assert!(result.is_ok());

	// Test updated client
	let result = client
		.send_raw_request("getSlot", Some(json!({"commitment": "finalized"})))
		.await
		.unwrap();
	assert_eq!(result["result"], 200);

	// Verify all mocks were called
	initial_mock.assert();
	initial_request_mock.assert();
	updated_mock.assert();
}

// ========== Convenience Method Tests ==========

#[tokio::test]
async fn test_get_slot_without_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let slot_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getSlot"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":123456789,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_slot(None).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 123456789);

	network_mock.assert();
	slot_mock.assert();
}

#[tokio::test]
async fn test_get_slot_with_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let slot_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getSlot",
			"params": [{ "commitment": "confirmed" }]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":123456790,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_slot(Some(SolanaCommitment::Confirmed)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 123456790);

	network_mock.assert();
	slot_mock.assert();
}

#[tokio::test]
async fn test_get_block_with_full_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let block_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlock",
			"params": [100, {
				"encoding": "json",
				"transactionDetails": "full",
				"rewards": true,
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"blockhash":"abc123","transactions":[]},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_block(100, Some(SolanaGetBlockConfig::full()))
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["blockhash"], "abc123");

	network_mock.assert();
	block_mock.assert();
}

#[tokio::test]
async fn test_get_block_with_signatures_only_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let block_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlock",
			"params": [200, {
				"encoding": "json",
				"transactionDetails": "signatures",
				"rewards": false,
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","result":{"blockhash":"def456","signatures":["sig1","sig2"]},"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_block(200, Some(SolanaGetBlockConfig::signatures_only()))
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["blockhash"], "def456");

	network_mock.assert();
	block_mock.assert();
}

#[tokio::test]
async fn test_get_block_with_none_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let block_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlock",
			"params": [300, {
				"encoding": "json",
				"transactionDetails": "none",
				"rewards": false,
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","result":{"blockhash":"ghi789","blockTime":1234567890},"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_block(300, Some(SolanaGetBlockConfig::none()))
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["blockhash"], "ghi789");

	network_mock.assert();
	block_mock.assert();
}

#[tokio::test]
async fn test_get_block_default_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	// When None is passed, it should default to full config
	let block_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlock",
			"params": [400, {
				"encoding": "json",
				"transactionDetails": "full",
				"rewards": true,
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"blockhash":"jkl012"},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_block(400, None).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["blockhash"], "jkl012");

	network_mock.assert();
	block_mock.assert();
}

#[tokio::test]
async fn test_get_blocks_start_only() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let blocks_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlocks",
			"params": [100]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":[100,101,102,103],"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_blocks(100, None, None).await;
	assert!(result.is_ok());
	let response = result.unwrap();
	let slots = response["result"].as_array().unwrap();
	assert_eq!(slots.len(), 4);

	network_mock.assert();
	blocks_mock.assert();
}

#[tokio::test]
async fn test_get_blocks_with_end_slot() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let blocks_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlocks",
			"params": [100, 110]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":[100,105,110],"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_blocks(100, Some(110), None).await;
	assert!(result.is_ok());
	let response = result.unwrap();
	let slots = response["result"].as_array().unwrap();
	assert_eq!(slots.len(), 3);

	network_mock.assert();
	blocks_mock.assert();
}

#[tokio::test]
async fn test_get_blocks_with_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let blocks_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlocks",
			"params": [100, 200, { "commitment": "confirmed" }]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":[100,150,200],"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_blocks(100, Some(200), Some(SolanaCommitment::Confirmed))
		.await;
	assert!(result.is_ok());

	network_mock.assert();
	blocks_mock.assert();
}

#[tokio::test]
async fn test_get_transaction_json_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let tx_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getTransaction",
			"params": ["5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW", {
				"encoding": "json",
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"slot":123,"transaction":{"signatures":["sig1"]}},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_transaction(
			"5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW",
			Some(SolanaGetTransactionConfig::json()),
		)
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["slot"], 123);

	network_mock.assert();
	tx_mock.assert();
}

#[tokio::test]
async fn test_get_transaction_json_parsed_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let tx_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getTransaction",
			"params": ["sig123", {
				"encoding": "jsonParsed",
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"slot":456,"meta":{"fee":5000}},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_transaction("sig123", Some(SolanaGetTransactionConfig::json_parsed()))
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["meta"]["fee"], 5000);

	network_mock.assert();
	tx_mock.assert();
}

#[tokio::test]
async fn test_get_transaction_default_config() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	// When None is passed, it should default to json config
	let tx_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getTransaction",
			"params": ["sig456", {
				"encoding": "json",
				"commitment": "finalized",
				"maxSupportedTransactionVersion": 0
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"slot":789},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_transaction("sig456", None).await;
	assert!(result.is_ok());

	network_mock.assert();
	tx_mock.assert();
}

#[tokio::test]
async fn test_get_account_info_without_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let account_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getAccountInfo",
			"params": ["11111111111111111111111111111111", {
				"encoding": "jsonParsed",
				"commitment": "finalized"
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"context":{"slot":123},"value":{"lamports":1000000,"owner":"11111111111111111111111111111111"}},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_account_info("11111111111111111111111111111111", None)
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["value"]["lamports"], 1000000);

	network_mock.assert();
	account_mock.assert();
}

#[tokio::test]
async fn test_get_account_info_with_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let account_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getAccountInfo",
			"params": ["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", {
				"encoding": "jsonParsed",
				"commitment": "processed"
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","result":{"context":{"slot":456},"value":{"lamports":2000000}},"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_account_info(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
			Some(SolanaCommitment::Processed),
		)
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["value"]["lamports"], 2000000);

	network_mock.assert();
	account_mock.assert();
}

#[tokio::test]
async fn test_get_program_accounts_without_filters() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let program_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getProgramAccounts",
			"params": ["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", {
				"encoding": "jsonParsed",
				"commitment": "finalized"
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","result":[{"pubkey":"acc1","account":{"lamports":100}}],"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_program_accounts("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", None, None)
		.await;
	assert!(result.is_ok());
	let response = result.unwrap();
	let accounts = response["result"].as_array().unwrap();
	assert_eq!(accounts.len(), 1);

	network_mock.assert();
	program_mock.assert();
}

#[tokio::test]
async fn test_get_program_accounts_with_filters() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let program_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getProgramAccounts",
			"params": ["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA", {
				"encoding": "jsonParsed",
				"commitment": "confirmed",
				"filters": [
					{ "dataSize": 165 },
					{ "memcmp": { "offset": 0, "bytes": "base58encodeddata" } }
				]
			}]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":[{"pubkey":"acc2","account":{"lamports":200}},{"pubkey":"acc3","account":{"lamports":300}}],"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let filters = vec![
		json!({ "dataSize": 165 }),
		json!({ "memcmp": { "offset": 0, "bytes": "base58encodeddata" } }),
	];

	let result = client
		.get_program_accounts(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
			Some(filters),
			Some(SolanaCommitment::Confirmed),
		)
		.await;
	assert!(result.is_ok());
	let response = result.unwrap();
	let accounts = response["result"].as_array().unwrap();
	assert_eq!(accounts.len(), 2);

	network_mock.assert();
	program_mock.assert();
}

#[tokio::test]
async fn test_get_block_height_without_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let height_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlockHeight"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":1233456,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_block_height(None).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 1233456);

	network_mock.assert();
	height_mock.assert();
}

#[tokio::test]
async fn test_get_block_height_with_commitment() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let height_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlockHeight",
			"params": [{ "commitment": "processed" }]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":1233457,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client
		.get_block_height(Some(SolanaCommitment::Processed))
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 1233457);

	network_mock.assert();
	height_mock.assert();
}

#[tokio::test]
async fn test_get_block_time() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let time_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlockTime",
			"params": [123456789]
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":1704067200,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_block_time(123456789).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 1704067200);

	network_mock.assert();
	time_mock.assert();
}

#[tokio::test]
async fn test_get_first_available_block() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let first_block_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getFirstAvailableBlock"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":1000,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_first_available_block().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 1000);

	network_mock.assert();
	first_block_mock.assert();
}

#[tokio::test]
async fn test_minimum_ledger_slot() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let min_slot_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "minimumLedgerSlot"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":500,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.minimum_ledger_slot().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], 500);

	network_mock.assert();
	min_slot_mock.assert();
}

#[tokio::test]
async fn test_get_version() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let version_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getVersion"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","result":{"solana-core":"1.18.0","feature-set":123456},"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_version().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"]["solana-core"], "1.18.0");

	network_mock.assert();
	version_mock.assert();
}

#[tokio::test]
async fn test_get_health() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let health_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getHealth"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":"ok","id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_health().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap()["result"], "ok");

	network_mock.assert();
	health_mock.assert();
}

#[tokio::test]
async fn test_with_test_payload() {
	let mut server = Server::new_async().await;

	// Use a custom test payload (getVersion instead of getHealth)
	let custom_mock = server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"getVersion"}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"solana-core":"1.18.0"},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let custom_payload = Some(r#"{"id":1,"jsonrpc":"2.0","method":"getVersion"}"#.to_string());

	let result = SolanaTransportClient::with_test_payload(&network, custom_payload).await;
	assert!(result.is_ok());

	custom_mock.assert();
}

#[tokio::test]
async fn test_http_client_accessor() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	// Test that http_client() returns a reference to the underlying client
	let http_client = client.http_client();

	// Verify the http_client has the same URL
	let url = http_client.get_current_url().await;
	assert_eq!(url, server.url());

	network_mock.assert();
}

// ========== Error Handling Tests ==========

#[tokio::test]
async fn test_get_slot_rpc_error() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let error_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getSlot"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			r#"{"jsonrpc":"2.0","error":{"code":-32007,"message":"Slot 123 was skipped"},"id":1}"#,
		)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_slot(None).await;
	// The response is still valid JSON, just contains an error field
	assert!(result.is_ok());
	let response = result.unwrap();
	assert!(response.get("error").is_some());

	network_mock.assert();
	error_mock.assert();
}

#[tokio::test]
async fn test_get_block_unavailable_error() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let error_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getBlock"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","error":{"code":-32004,"message":"Block not available for slot 100"},"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_block(100, None).await;
	assert!(result.is_ok());
	let response = result.unwrap();
	assert!(response.get("error").is_some());
	assert_eq!(response["error"]["code"], -32004);

	network_mock.assert();
	error_mock.assert();
}

#[tokio::test]
async fn test_get_transaction_not_found() {
	let mut server = Server::new_async().await;

	let network_mock = create_solana_valid_server_mock_network_response(&mut server);

	let not_found_mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::PartialJson(json!({
			"method": "getTransaction"
		})))
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":null,"id":1}"#)
		.create();

	let network = create_solana_test_network_with_urls(vec![&server.url()]);
	let client = SolanaTransportClient::new(&network).await.unwrap();

	let result = client.get_transaction("nonexistent_sig", None).await;
	assert!(result.is_ok());
	assert!(result.unwrap()["result"].is_null());

	network_mock.assert();
	not_found_mock.assert();
}

// ========== Commitment Enum Tests ==========

#[test]
fn test_commitment_as_str() {
	assert_eq!(SolanaCommitment::Finalized.as_str(), "finalized");
	assert_eq!(SolanaCommitment::Confirmed.as_str(), "confirmed");
	assert_eq!(SolanaCommitment::Processed.as_str(), "processed");
}

#[test]
fn test_commitment_default() {
	let default: SolanaCommitment = Default::default();
	assert_eq!(default.as_str(), "finalized");
}

// ========== Config Object Tests ==========

#[test]
fn test_get_block_config_full() {
	let config = SolanaGetBlockConfig::full();
	assert_eq!(config.encoding, Some("json".to_string()));
	assert_eq!(config.transaction_details, Some("full".to_string()));
	assert_eq!(config.rewards, Some(true));
	assert_eq!(config.commitment, Some("finalized".to_string()));
	assert_eq!(config.max_supported_transaction_version, Some(0));
}

#[test]
fn test_get_block_config_signatures_only() {
	let config = SolanaGetBlockConfig::signatures_only();
	assert_eq!(config.encoding, Some("json".to_string()));
	assert_eq!(config.transaction_details, Some("signatures".to_string()));
	assert_eq!(config.rewards, Some(false));
	assert_eq!(config.commitment, Some("finalized".to_string()));
	assert_eq!(config.max_supported_transaction_version, Some(0));
}

#[test]
fn test_get_block_config_none() {
	let config = SolanaGetBlockConfig::none();
	assert_eq!(config.encoding, Some("json".to_string()));
	assert_eq!(config.transaction_details, Some("none".to_string()));
	assert_eq!(config.rewards, Some(false));
	assert_eq!(config.commitment, Some("finalized".to_string()));
	assert_eq!(config.max_supported_transaction_version, Some(0));
}

#[test]
fn test_get_block_config_default() {
	let config = SolanaGetBlockConfig::default();
	assert!(config.encoding.is_none());
	assert!(config.transaction_details.is_none());
	assert!(config.rewards.is_none());
	assert!(config.commitment.is_none());
	assert!(config.max_supported_transaction_version.is_none());
}

#[test]
fn test_get_transaction_config_json() {
	let config = SolanaGetTransactionConfig::json();
	assert_eq!(config.encoding, Some("json".to_string()));
	assert_eq!(config.commitment, Some("finalized".to_string()));
	assert_eq!(config.max_supported_transaction_version, Some(0));
}

#[test]
fn test_get_transaction_config_json_parsed() {
	let config = SolanaGetTransactionConfig::json_parsed();
	assert_eq!(config.encoding, Some("jsonParsed".to_string()));
	assert_eq!(config.commitment, Some("finalized".to_string()));
	assert_eq!(config.max_supported_transaction_version, Some(0));
}

#[test]
fn test_get_transaction_config_default() {
	let config = SolanaGetTransactionConfig::default();
	assert!(config.encoding.is_none());
	assert!(config.commitment.is_none());
	assert!(config.max_supported_transaction_version.is_none());
}
