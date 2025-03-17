use mockito::Server;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, EndpointManager, RotatingTransport,
};

// Mock transport implementation for testing
#[derive(Clone)]
struct MockTransport {
	client: reqwest::Client,
	current_url: Arc<RwLock<String>>,
	retry_policy: ExponentialBackoff,
}

impl MockTransport {
	fn new() -> Self {
		Self {
			client: reqwest::Client::new(),
			current_url: Arc::new(RwLock::new(String::new())),
			retry_policy: ExponentialBackoff::builder().build_with_max_retries(2),
		}
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockTransport {
	async fn get_current_url(&self) -> String {
		self.current_url.read().await.clone()
	}

	async fn send_raw_request<P: Into<Value> + Send + Clone + Serialize>(
		&self,
		_method: &str,
		_params: Option<P>,
	) -> Result<serde_json::Value, anyhow::Error> {
		Ok(json!({
			"jsonrpc": "2.0",
			"result": "mocked_response",
			"id": 1
		}))
	}

	async fn customize_request<P: Into<Value> + Send + Clone + Serialize>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Value {
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": method,
			"params": params
		})
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(self.retry_policy)
	}

	fn set_retry_policy(&mut self, retry_policy: ExponentialBackoff) -> Result<(), anyhow::Error> {
		self.retry_policy = retry_policy;
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockTransport {
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		// Simulate connection attempt
		match self.client.get(url).send().await {
			Ok(_) => Ok(()),
			Err(e) => Err(anyhow::anyhow!("Failed to connect: {}", e)),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		*self.current_url.write().await = url.to_string();
		Ok(())
	}
}

#[tokio::test]
async fn test_endpoint_rotation() {
	// Set up mock servers
	let server1 = Server::new_async().await;
	let mut server2 = Server::new_async().await;
	let server3 = Server::new_async().await;

	let mock2 = server2
		.mock("GET", "/")
		.with_status(200)
		.create_async()
		.await;

	let manager = EndpointManager::new(server1.url().as_ref(), vec![server2.url(), server3.url()]);
	let transport = MockTransport::new();

	// Test initial state
	assert_eq!(&*manager.active_url.read().await, &server1.url());
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![server2.url(), server3.url()]
	);

	// Test rotation
	manager.rotate_url(&transport).await.unwrap();
	assert_eq!(&*manager.active_url.read().await, &server2.url());

	mock2.assert();
}

#[tokio::test]
async fn test_send_raw_request() {
	let mut server = Server::new_async().await;

	// Mock successful response
	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.create_async()
		.await;

	let manager = EndpointManager::new(server.url().as_ref(), vec![]);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	assert_eq!(result["result"], "success");
	mock.assert();
}

#[tokio::test]
async fn test_rotation_on_error() {
	let mut primary_server = Server::new_async().await;
	let mut fallback_server = Server::new_async().await;

	// Primary server returns 429 (Too Many Requests)
	let primary_mock = primary_server
		.mock("POST", "/")
		.with_status(429)
		.with_body("Rate limited")
		.expect(3) // Expect 3 requests due to default retry policy
		.create_async()
		.await;

	// Fallback server returns success
	let fallback_mock = fallback_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.create_async()
		.await;

	let manager = EndpointManager::new(primary_server.url().as_ref(), vec![fallback_server.url()]);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	assert_eq!(result["result"], "success");
	primary_mock.assert();
	fallback_mock.assert();

	// Verify rotation occurred
	assert_eq!(&*manager.active_url.read().await, &fallback_server.url());
}

#[tokio::test]
async fn test_no_fallback_urls_available() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(429)
		.with_body("Rate limited")
		.expect(3) // Expect 3 requests due to default retry policy
		.create_async()
		.await;

	let manager = EndpointManager::new(server.url().as_ref(), vec![]);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	mock.assert();
}

#[tokio::test]
async fn test_customize_request() {
	let transport = MockTransport::new();

	// Test with parameters
	let result = transport
		.customize_request("test_method", Some(json!(["param1", "param2"])))
		.await;

	assert_eq!(
		result,
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "test_method",
			"params": ["param1", "param2"]
		})
	);

	// Test without parameters
	let result = transport
		.customize_request::<Value>("test_method", None)
		.await;

	assert_eq!(
		result,
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "test_method",
			"params": null
		})
	);
}

#[tokio::test]
async fn test_rotate_url_no_fallbacks() {
	let server = Server::new_async().await;

	// Create manager with no fallback URLs
	let manager = EndpointManager::new(server.url().as_ref(), vec![]);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();
	assert!(err.to_string().contains("No fallback URLs available"));

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &server.url());
}

#[tokio::test]
async fn test_rotate_url_all_urls_match_active() {
	let server = Server::new_async().await;

	// Create manager with fallback URLs that are identical to the active URL
	let active_url = server.url();
	let manager = EndpointManager::new(
		active_url.as_ref(),
		vec![active_url.clone(), active_url.clone()],
	);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();
	assert!(err.to_string().contains("No fallback URLs available"));

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &active_url);

	// Verify fallback URLs are unchanged
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![active_url.clone(), active_url.clone()]
	);
}

#[tokio::test]
async fn test_rotate_url_connection_failure() {
	let server = Server::new_async().await;

	// Create manager with an invalid fallback URL that will fail to connect
	let invalid_url = "http://invalid-domain-that-does-not-exist:12345";
	let manager = EndpointManager::new(server.url().as_ref(), vec![invalid_url.to_string()]);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();
	assert!(err
		.to_string()
		.contains("Failed to connect to fallback URL"));

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &server.url());

	// Verify the failed URL was pushed back to fallback_urls
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![invalid_url.to_string()]
	);
}
