use openzeppelin_monitor::{
	models::{BlockChainType, Network},
	services::blockchain::{BlockchainTransport, TransientErrorRetryStrategy, WsTransportClient},
	utils::tests::builders::network::NetworkBuilder,
};

use futures_util::{SinkExt, StreamExt};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

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

async fn start_test_server() -> (String, oneshot::Sender<()>) {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let addr = listener.local_addr().unwrap();
	let url = format!("ws://{}", addr);
	let (shutdown_tx, shutdown_rx) = oneshot::channel();

	tokio::spawn(async move {
		let mut shutdown_rx = shutdown_rx;
		loop {
			tokio::select! {
				accept_result = listener.accept() => {
					if let Ok((stream, _)) = accept_result {
						let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();
						let (mut write, _) = ws_stream.split();
						write.send(Message::Text("Hello".into())).await.unwrap();
					}
				}
				_ = &mut shutdown_rx => {
					break;
				}
			}
		}
	});

	(url, shutdown_tx)
}

#[tokio::test]
async fn test_ws_transport_connection() {
	// Start a test WebSocket server
	let (url, shutdown_tx) = start_test_server().await;
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
	let (url1, shutdown_tx1) = start_test_server().await;
	let (url2, shutdown_tx2) = start_test_server().await;

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
	let (url1, shutdown_tx1) = start_test_server().await;
	let (url2, shutdown_tx2) = start_test_server().await;
	let (url3, shutdown_tx3) = start_test_server().await;

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
	let (url, shutdown_tx) = start_test_server().await;
	let network = create_test_network_with_urls(vec![&url]);
	let mut client = WsTransportClient::new(&network).await.unwrap();

	// Test send_raw_request
	let result = client.send_raw_request::<Value>("testMethod", None).await;
	assert!(result.is_err(), "send_raw_request should return error");
	assert_eq!(
		result.unwrap_err().to_string(),
		"`send_raw_request` not implemented",
		"Should return exact error message"
	);

	// Test set_retry_policy
	let policy = ExponentialBackoff::builder().build_with_max_retries(3);
	let result = client.set_retry_policy(policy, Some(TransientErrorRetryStrategy));
	assert!(result.is_err(), "set_retry_policy should return error");
	assert_eq!(
		result.unwrap_err().to_string(),
		"`set_retry_policy` not implemented",
		"Should return exact error message"
	);

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
	assert_eq!(
		result.unwrap_err().to_string(),
		"`update_endpoint_manager_client` not implemented",
		"Should return exact error message"
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_connection_failure() {
	// Create a network with an invalid WebSocket URL
	let network = create_test_network_with_urls(vec!["ws://invalid.example.com:12345"]);

	// Test client creation with invalid URL
	let client = WsTransportClient::new(&network).await;
	assert!(
		client.is_err(),
		"Should fail to create client with invalid URL"
	);
	assert_eq!(
		client.unwrap_err().to_string(),
		"No working WebSocket URLs found",
		"Should indicate no working URLs were found"
	);
}

#[tokio::test]
async fn test_ws_transport_connection_check_failure() {
	// Start a test server
	let (url, shutdown_tx) = start_test_server().await;
	let network = create_test_network_with_urls(vec![&url]);

	// Create client with valid URL
	let client = WsTransportClient::new(&network).await.unwrap();

	// Manually set an invalid URL to test check_connection
	let mut active_url = client.active_url.lock().await;
	*active_url = "ws://invalid.example.com:12345".to_string();
	drop(active_url); // Release the lock

	// Test connection check with invalid URL
	let result = client.check_connection().await;
	assert!(result.is_err(), "Should fail to connect to invalid URL");
	assert!(
		result
			.unwrap_err()
			.to_string()
			.starts_with("Failed to connect:"),
		"Should return connection failure error"
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_ws_transport_fallback_failure() {
	// Start two test servers
	let (url1, shutdown_tx1) = start_test_server().await;
	let (url2, shutdown_tx2) = start_test_server().await;

	let network = create_test_network_with_urls(vec![&url1, &url2]);
	let client = WsTransportClient::new(&network).await.unwrap();

	// Manually set an invalid URL as the first fallback
	let mut fallback_urls = client.fallback_urls.lock().await;
	fallback_urls[0] = "ws://invalid.example.com:12345".to_string();
	drop(fallback_urls); // Release the lock

	// Test fallback to invalid URL
	let result = client.try_fallback().await;
	assert!(result.is_err(), "Should fail to connect to fallback URL");
	assert_eq!(
		result.unwrap_err().to_string(),
		"Failed to connect to fallback URL",
		"Should return fallback connection failure error"
	);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
}
