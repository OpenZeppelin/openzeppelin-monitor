use mockall::mock;
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, RotatingTransport, TransportError,
};

// Mock implementation of a EVM transport client.
// Used for testing Ethereum compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub EVMTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, TransportError>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for EVMTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockEVMTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone,
	{
		let params_value = params.map(|p| p.into());
		self.send_raw_request(method, params_value.and_then(|v| v.as_array().cloned()))
			.await
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockEVMTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

// Mock implementation of a Stellar transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub StellarTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, TransportError>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for StellarTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockStellarTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockStellarTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

// Mock transport that always fails to update the client
// Used for testing URL update failure scenarios in rotating transports.
#[derive(Clone)]
pub struct AlwaysFailsToUpdateClientTransport {
	pub current_url: Arc<RwLock<String>>,
}

#[async_trait::async_trait]
impl BlockchainTransport for AlwaysFailsToUpdateClientTransport {
	async fn get_current_url(&self) -> String {
		self.current_url.read().await.clone()
	}
	async fn send_raw_request<P: Into<Value> + Send + Clone + Serialize>(
		&self,
		_method: &str,
		_params: Option<P>,
	) -> Result<serde_json::Value, TransportError> {
		Ok(json!({"jsonrpc": "2.0", "result": "mocked_response", "id": 1}))
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

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for AlwaysFailsToUpdateClientTransport {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!("Simulated client update failure"))
	}
}

// Mock transport implementation for testing
// Used to simulate blockchain transport behavior without actual network calls in endpoint manager tests.
#[derive(Clone)]
pub struct MockTransport {
	client: reqwest::Client,
	current_url: Arc<RwLock<String>>,
}

impl MockTransport {
	pub fn new() -> Self {
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
