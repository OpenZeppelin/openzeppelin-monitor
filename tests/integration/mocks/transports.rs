use mockall::mock;
use reqwest_retry::policies::ExponentialBackoff;
use serde_json::Value;

use openzeppelin_monitor::services::blockchain::{BlockchainTransport, RotatingTransport};

// Mock implementation of a Alloy transport client.
// Used for testing Ethereum/Alloy-compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub AlloyTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, anyhow::Error>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for AlloyTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockAlloyTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone,
	{
		let params_value = params.map(|p| p.into());
		self.send_raw_request(method, params_value.and_then(|v| v.as_array().cloned()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockAlloyTransportClient {
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
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, anyhow::Error>;
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
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), anyhow::Error> {
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

// Mock implementation of a Horizon transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub HorizonTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, anyhow::Error>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for HorizonTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockHorizonTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockHorizonTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
}
