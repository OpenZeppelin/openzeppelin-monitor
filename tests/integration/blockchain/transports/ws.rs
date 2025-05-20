use crate::integration::mocks::start_test_websocket_server;
use openzeppelin_monitor::{
	models::{BlockChainType, Network},
	services::blockchain::{BlockchainTransport, TransientErrorRetryStrategy, WsTransportClient},
	utils::tests::builders::network::NetworkBuilder,
};

use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::Value;

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
	// Start a test WebSocket server
	let (url, shutdown_tx) = start_test_websocket_server().await;
	let network = create_test_network_with_urls(vec![&url]);

	// Test client creation
	let client = WsTransportClient::new(&network).await;
	assert!(client.is_ok(), "Failed to create WebSocket client");

	let client = client.unwrap();

	// Test connection check
	let connection_result = client.check_connection().await;
	assert!(
		connection_result.is_ok(),
		"Failed to establish WebSocket connection"
	);

	// Test URL management
	let current_url = client.get_current_url().await;
	assert!(!current_url.is_empty(), "Current URL should not be empty");
	assert!(
		current_url.starts_with("ws://"),
		"URL should be a WebSocket URL"
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_fallback() {
	// Start two test servers
	let (url1, shutdown_tx1) = start_test_websocket_server().await;
	let (url2, shutdown_tx2) = start_test_websocket_server().await;

	let network = create_test_network_with_urls(vec![&url1, &url2]);
	let client = WsTransportClient::new(&network).await.unwrap();

	// Test fallback functionality
	let fallback_result = client.try_fallback().await;
	assert!(fallback_result.is_ok(), "Failed to switch to fallback URL");

	// Verify URL was updated
	let current_url = client.get_current_url().await;
	assert!(
		current_url.starts_with("ws://"),
		"URL should be a WebSocket URL"
	);
	assert!(
		current_url != url1,
		"URL should have changed after fallback"
	);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
}

#[tokio::test]
async fn test_ws_transport_invalid_urls() {
	let network = create_test_network_with_urls(vec!["ws://invalid.example.com"]);

	// Test client creation with invalid URLs
	let client = WsTransportClient::new(&network).await;
	assert!(
		client.is_err(),
		"Should fail to create client with invalid URLs"
	);
	assert!(
		client
			.unwrap_err()
			.to_string()
			.contains("No working WebSocket URLs found"),
		"Should indicate no working URLs were found"
	);
}

#[tokio::test]
async fn test_ws_transport_no_ws_urls() {
	let network = create_test_network_with_urls(vec![]);

	// Test client creation with no WebSocket URLs
	let client = WsTransportClient::new(&network).await;
	assert!(
		client.is_err(),
		"Should fail to create client with no WebSocket URLs"
	);
	assert!(
		client
			.unwrap_err()
			.to_string()
			.contains("No valid WebSocket RPC URLs found"),
		"Should indicate no valid WebSocket URLs were found"
	);
}

#[tokio::test]
async fn test_ws_transport_multiple_fallbacks() {
	// Start three test servers
	let (url1, shutdown_tx1) = start_test_websocket_server().await;
	let (url2, shutdown_tx2) = start_test_websocket_server().await;
	let (url3, shutdown_tx3) = start_test_websocket_server().await;

	let network = create_test_network_with_urls(vec![&url1, &url2, &url3]);
	let client = WsTransportClient::new(&network).await.unwrap();

	// Test multiple fallback attempts
	for _ in 0..2 {
		let fallback_result = client.try_fallback().await;
		assert!(
			fallback_result.is_ok(),
			"Should be able to switch to fallback URLs"
		);
	}

	// Verify we've exhausted all fallbacks
	let final_fallback = client.try_fallback().await;

	assert!(
		final_fallback.is_err(),
		"Should fail when no more fallbacks available"
	);
	assert!(
		final_fallback
			.unwrap_err()
			.to_string()
			.contains("No fallback URLs available"),
		"Should indicate no fallback URLs are available"
	);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
	let _ = shutdown_tx3.send(());
}

#[tokio::test]
async fn test_ws_transport_unimplemented_methods() {
	let (url, shutdown_tx) = start_test_websocket_server().await;
	let network = create_test_network_with_urls(vec![&url]);
	let mut client = WsTransportClient::new(&network).await.unwrap();

	// Test send_raw_request
	let result = client.send_raw_request::<Value>("testMethod", None).await;
	assert!(result.is_err(), "send_raw_request should return error");
	assert!(result.unwrap_err().to_string().contains("not implemented"));

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
