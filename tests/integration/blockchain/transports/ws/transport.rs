use crate::integration::mocks::{create_default_method_responses, start_test_websocket_server};
use openzeppelin_monitor::{
	models::{BlockChainType, Network},
	services::blockchain::{
		BlockchainTransport, RotatingTransport, TransientErrorRetryStrategy, WsConfig,
		WsTransportClient,
	},
	utils::tests::builders::network::NetworkBuilder,
};

use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::sleep;

// Helper function to create a test network with specific URLs
fn create_test_network_with_urls(urls: Vec<&str>) -> Network {
	let mut builder = NetworkBuilder::new()
		.name("Test Network")
		.slug("test_network")
		.network_type(BlockChainType::EVM)
		.chain_id(1)
		.block_time_ms(1000)
		.confirmation_blocks(1)
		.cron_schedule("0 */5 * * * *")
		.max_past_blocks(10)
		.store_blocks(true);

	for (i, url) in urls.iter().enumerate() {
		builder = builder.add_rpc_url(url, "ws_rpc", 100 - (i as u32 * 10));
	}

	builder.build()
}

#[tokio::test]
async fn test_ws_transport_connection() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);

	// Test client creation
	let client = WsTransportClient::new(&network, None).await;
	assert!(client.is_ok(), "Failed to create WebSocket client");
	let client = client.unwrap();

	// Test URL management
	let current_url = client.get_current_url().await;
	assert!(!current_url.is_empty(), "Current URL should not be empty");
	assert!(
		current_url.starts_with("ws://"),
		"URL should be a WebSocket URL"
	);

	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_fallback() {
	// Start two test servers
	let (url1, shutdown_tx1) = start_test_websocket_server(create_default_method_responses()).await;
	let (url2, shutdown_tx2) = start_test_websocket_server(create_default_method_responses()).await;

	let network = create_test_network_with_urls(vec![&url1, &url2]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test fallback functionality by simulating a connection error
	let current_url = client.get_current_url().await;
	assert_eq!(current_url, url1, "Initial URL should be the first URL");

	// Shutdown the first server to force a connection error
	let _ = shutdown_tx1.send(());

	// This should trigger rotation to the second URL
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should handle rotation gracefully");

	// Verify URL was updated
	let new_url = client.get_current_url().await;
	assert!(
		new_url.starts_with("ws://"),
		"URL should be a WebSocket URL"
	);
	assert!(new_url != url1, "URL should have changed after rotation");

	let _ = shutdown_tx2.send(());
}

#[tokio::test]
async fn test_ws_transport_invalid_urls() {
	let network = create_test_network_with_urls(vec!["ws://invalid.example.com"]);
	let client = WsTransportClient::new(&network, None).await;

	assert!(
		client.is_err(),
		"Should fail to create client with invalid URLs"
	);
	assert!(
		client
			.unwrap_err()
			.to_string()
			.contains("Failed to connect"),
		"Should indicate connection failure"
	);
}

#[tokio::test]
async fn test_ws_transport_no_ws_urls() {
	let network = create_test_network_with_urls(vec![]);
	let client = WsTransportClient::new(&network, None).await;
	assert!(
		client.is_err(),
		"Should fail to create client with no WebSocket URLs"
	);
	assert!(
		client
			.unwrap_err()
			.to_string()
			.contains("No WebSocket URLs available"),
		"Should indicate no WebSocket URLs available"
	);
}

#[tokio::test]
async fn test_ws_transport_multiple_fallbacks() {
	// Start three test servers
	let (url1, shutdown_tx1) = start_test_websocket_server(create_default_method_responses()).await;
	let (url2, shutdown_tx2) = start_test_websocket_server(create_default_method_responses()).await;
	let (url3, shutdown_tx3) = start_test_websocket_server(create_default_method_responses()).await;

	let network = create_test_network_with_urls(vec![&url1, &url2, &url3]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test multiple fallback attempts by simulating connection errors
	// Shutdown first server to force rotation
	let _ = shutdown_tx1.send(());
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should handle first rotation gracefully");

	// Shutdown second server to force another rotation
	let _ = shutdown_tx2.send(());
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should handle second rotation gracefully");

	// Verify we've rotated to the third URL
	let final_url = client.get_current_url().await;
	assert_eq!(final_url, url3, "Should have rotated to the third URL");

	// Cleanup
	let _ = shutdown_tx3.send(());
}

#[tokio::test]
async fn test_ws_transport_unimplemented_methods() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);
	let mut client = WsTransportClient::new(&network, None).await.unwrap();

	// Test set_retry_policy
	let policy = ExponentialBackoff::builder().build_with_max_retries(3);
	let result = client.set_retry_policy(policy, Some(TransientErrorRetryStrategy));
	assert!(result.is_err(), "set_retry_policy should return error");
	assert!(result.unwrap_err().to_string().contains("not implemented"));

	// Test update_endpoint_manager_client
	let client_builder = ClientBuilder::new(reqwest::Client::new())
		.with(RetryTransientMiddleware::new_with_policy(
			ExponentialBackoff::builder().build_with_max_retries(3),
		))
		.build();
	let result = client.update_endpoint_manager_client(client_builder);
	assert!(
		result.is_err(),
		"update_endpoint_manager_client should return error"
	);
	assert!(result.unwrap_err().to_string().contains("not implemented"));

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_request_response() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test simple request
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should successfully send request");
	let response = result.unwrap();
	assert!(response.is_object(), "Response should be a JSON object");
	assert_eq!(
		response["result"].as_str().unwrap(),
		"Development",
		"Should get expected response"
	);

	// Test request with parameters
	let params = json!(["latest"]);
	let result = client
		.send_raw_request("chain_getBlockHash", Some(params))
		.await;
	assert!(
		result.is_ok(),
		"Should successfully send request with parameters"
	);
	let response = result.unwrap();
	assert!(response.is_object(), "Response should be a JSON object");
	assert!(
		response["result"].as_str().unwrap().starts_with("0x"),
		"Should get block hash response"
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_timeout() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test request timeout by sending a request that will cause the server to hang
	let result = client.send_raw_request("timeout_test", None::<Value>).await;
	assert!(result.is_err(), "Should timeout on hanging request");
	assert!(
		result.unwrap_err().to_string().contains("Connection error"),
		"Should indicate connection error"
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_connection_state() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test initial connection state
	let connection_result = client.try_connect(&url).await;
	assert!(connection_result.is_ok(), "Should be connected initially");

	// Test connection health after activity
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should maintain connection after activity");

	// Test reconnection after disconnection
	// Simulate disconnection by dropping the server
	let _ = shutdown_tx.send(());
	sleep(Duration::from_millis(100)).await;

	// Start a new server
	let (new_url, new_shutdown_tx) =
		start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&new_url]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Verify reconnection
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should reconnect successfully");

	// Cleanup
	let _ = new_shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_concurrent_requests() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);
	let client = WsTransportClient::new(&network, None).await.unwrap();

	// Test multiple concurrent requests
	let mut handles = vec![];
	for _i in 0..5 {
		let client = client.clone();
		handles.push(tokio::spawn(async move {
			client.send_raw_request("system_chain", None::<Value>).await
		}));
	}

	// Wait for all requests to complete
	let results = futures::future::join_all(handles).await;
	for result in results {
		assert!(result.unwrap().is_ok(), "Concurrent request should succeed");
	}

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_websocket_connection_health() {
	let (url, shutdown_tx) = start_test_websocket_server(create_default_method_responses()).await;
	let network = create_test_network_with_urls(vec![&url]);

	// Create client with custom config
	let config = WsConfig::single_attempt();
	let client = WsTransportClient::new(&network, Some(config))
		.await
		.unwrap();

	// Test initial connection health
	let connection = client.connection.lock().await;
	assert!(connection.is_connected());
	assert!(connection.is_healthy);

	// Test connection health after activity
	drop(connection);

	// Send a request to update activity
	let result = client.send_raw_request("system_chain", None::<Value>).await;
	assert!(result.is_ok(), "Should maintain connection after activity");

	// Verify connection is still healthy
	let connection = client.connection.lock().await;
	assert!(connection.is_connected());
	assert!(connection.is_healthy);
	drop(connection);

	// Test connection timeout
	let result = client.send_raw_request("timeout_test", None::<Value>).await;
	assert!(result.is_err(), "Should timeout on hanging request");
	assert!(
		result.unwrap_err().to_string().contains("Connection error"),
		"Should indicate connection error"
	);

	let connection = client.connection.lock().await;
	assert!(!connection.is_connected());
	assert!(!connection.is_healthy);
	drop(connection);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_websocket_connection_timeout() {
	// Create config with very short timeouts
	let config = WsConfig::new()
		.with_max_reconnect_attempts(0)
		.with_connection_timeout(Duration::from_millis(1))
		.with_message_timeout(Duration::from_millis(1))
		.build();

	// Test connection timeout with invalid URL
	let invalid_url = "ws://invalid-domain-that-does-not-exist:12345";

	let network = create_test_network_with_urls(vec![invalid_url]);
	let result = WsTransportClient::new(&network, Some(config)).await;
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("timeout"));
}
