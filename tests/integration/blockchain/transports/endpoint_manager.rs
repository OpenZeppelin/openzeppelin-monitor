use mockito::Server;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, EndpointManager, RotatingTransport, TransientErrorRetryStrategy,
	TransportError,
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
	) -> Result<serde_json::Value, TransportError> {
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

	fn set_retry_policy(
		&mut self,
		_retry_policy: ExponentialBackoff,
		_retry_strategy: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
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

fn get_mock_client_builder() -> ClientWithMiddleware {
	ClientBuilder::new(reqwest::Client::new()).build()
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

	let manager = EndpointManager::new(
		get_mock_client_builder(),
		server1.url().as_ref(),
		vec![server2.url(), server3.url()],
	);
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

	let manager = EndpointManager::new(get_mock_client_builder(), server.url().as_ref(), vec![]);
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
		.expect(1) // Expect 1 request due to 429 error which is not retried
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

	let manager = EndpointManager::new(
		get_mock_client_builder(),
		primary_server.url().as_ref(),
		vec![fallback_server.url()],
	);
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
		.expect(1) // Expect 1 request due to 429 error which is not retried
		.create_async()
		.await;

	let manager = EndpointManager::new(get_mock_client_builder(), server.url().as_ref(), vec![]);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	match err {
		TransportError::Http {
			status_code,
			url,
			body,
			..
		} => {
			assert_eq!(status_code, 429);
			assert_eq!(url, server.url());
			assert_eq!(body, "Rate limited");
		}
		_ => panic!("Expected Http error with status code 429"),
	}
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
	let manager = EndpointManager::new(get_mock_client_builder(), server.url().as_ref(), vec![]);
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
		get_mock_client_builder(),
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
	let manager = EndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![invalid_url.to_string()],
	);
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

#[tokio::test]
async fn test_update_client() {
	let mut server = Server::new_async().await;

	// Set up two different responses to differentiate between clients
	let initial_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "initial_client", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let mut manager =
		EndpointManager::new(get_mock_client_builder(), server.url().as_ref(), vec![]);

	// Test initial client
	let transport = MockTransport::new();
	let initial_result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();
	assert_eq!(initial_result["result"], "initial_client");
	initial_mock.assert();

	// Set up mock for new client
	let updated_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "updated_client", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	// Create and update to new client with different configuration
	let new_client = ClientBuilder::new(reqwest::Client::new())
		.with(RetryTransientMiddleware::new_with_policy(
			ExponentialBackoff::builder().build_with_max_retries(3),
		))
		.build();
	manager.update_client(new_client);

	// Test updated client
	let updated_result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();
	assert_eq!(updated_result["result"], "updated_client");
	updated_mock.assert();
}

#[tokio::test]
async fn test_set_retry_policy() {
	let mut server = Server::new_async().await;

	// Set up a sequence of responses to test retry behavior
	let retry_mock = server
		.mock("POST", "/")
		.with_status(429) // Too Many Requests
		.with_body("Rate limited")
		.expect(2) // Expect 2 retries
		.create_async()
		.await;

	let success_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success_after_retry", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let mut manager = EndpointManager::new(
		get_mock_client_builder(), // Initial client with no retry policy
		server.url().as_ref(),
		vec![],
	);

	// Set a custom retry policy with exactly 2 retries
	let retry_policy = ExponentialBackoff::builder().build_with_max_retries(2);

	manager.set_retry_policy(retry_policy, TransientErrorRetryStrategy);

	// Make request that should trigger retries
	let transport = MockTransport::new();
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	// Verify that we got the successful response after retries
	assert_eq!(result["result"], "success_after_retry");

	// Verify that both mocks were called the expected number of times
	retry_mock.assert();
	success_mock.assert();
}

#[tokio::test]
async fn test_send_raw_request_network_error() {
	// Set up with an invalid primary URL that will cause a network error
	let invalid_url = "http://invalid-domain-that-will-fail:12345";
	let mut valid_server = Server::new_async().await;

	// Set up mock for fallback server
	let success_mock = valid_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = EndpointManager::new(
		get_mock_client_builder(),
		invalid_url,
		vec![valid_server.url()], // Add valid fallback URL
	);
	let transport = MockTransport::new();

	// Send request - should fail first with network error, then rotate and succeed
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	// Verify success after rotation
	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"], "success");
	success_mock.assert();

	// Verify URL rotation occurred
	assert_eq!(&*manager.active_url.read().await, &valid_server.url());
}

#[tokio::test]
async fn test_send_raw_request_network_error_no_fallback() {
	// Set up with an invalid URL and no fallbacks
	let invalid_url = "http://invalid-domain-that-will-fail:12345";
	let manager = EndpointManager::new(
		get_mock_client_builder(),
		invalid_url,
		vec![], // No fallback URLs
	);
	let transport = MockTransport::new();

	// Send request - should fail with network error and no rotation possible
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	// Verify error
	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), TransportError::Network(_)));

	// Verify URL didn't change
	assert_eq!(&*manager.active_url.read().await, invalid_url);
}

#[tokio::test]
async fn test_send_raw_request_response_parse_error() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "invalid_json"#) // Missing closing brace
		.expect(1)
		.create_async()
		.await;

	let manager = EndpointManager::new(get_mock_client_builder(), server.url().as_ref(), vec![]);
	let transport = MockTransport::new();

	// Send request - should fail with parse error
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	assert!(matches!(
		result.unwrap_err(),
		TransportError::ResponseParse(_)
	));

	mock.assert();
}

#[tokio::test]
async fn test_send_raw_request_all_urls_fail_with_rotation_error() {
	let invalid_url1 = "http://invalid-domain-that-will-fail-1:12345";
	let invalid_url2 = "http://invalid-domain-that-will-fail-2:12345";
	let invalid_url3 = "http://invalid-domain-that-will-fail-3:12345";

	let manager = EndpointManager::new(
		get_mock_client_builder(),
		invalid_url1,
		vec![invalid_url2.to_string(), invalid_url3.to_string()],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	assert!(matches!(
		result.unwrap_err(),
		TransportError::UrlRotation(_)
	));
}
