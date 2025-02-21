use mockito::Server;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use openzeppelin_monitor::services::blockchain::{
	BlockChainError, BlockchainTransport, EndpointManager, RotatingTransport,
};

// Mock transport implementation for testing
#[derive(Clone)]
struct MockTransport {
	client: reqwest::Client,
	current_url: Arc<RwLock<String>>,
}

impl MockTransport {
	fn new() -> Self {
		Self {
			client: reqwest::Client::new(),
			current_url: Arc::new(RwLock::new(String::new())),
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
	) -> Result<serde_json::Value, BlockChainError> {
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
}

#[async_trait::async_trait]
impl RotatingTransport for MockTransport {
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError> {
		// Simulate connection attempt
		match self.client.get(url).send().await {
			Ok(_) => Ok(()),
			Err(e) => Err(BlockChainError::connection_error(e.to_string())),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), BlockChainError> {
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

	let manager = EndpointManager::new(server1.url(), vec![server2.url(), server3.url()]);
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

	let manager = EndpointManager::new(server.url(), vec![]);
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

	let manager = EndpointManager::new(primary_server.url(), vec![fallback_server.url()]);
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
		.create_async()
		.await;

	let manager = EndpointManager::new(server.url(), vec![]);
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
