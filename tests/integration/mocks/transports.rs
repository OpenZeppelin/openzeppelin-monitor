use mockall::mock;
use reqwest_retry::policies::ExponentialBackoff;
use serde_json::Value;

use openzeppelin_monitor::services::blockchain::{
	BlockChainError, BlockchainTransport, RotatingTransport,
};

// Mock implementation of a Web3 transport client.
// Used for testing Ethereum/Web3-compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub Web3TransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, BlockChainError>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for Web3TransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockWeb3TransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, BlockChainError>
	where
		P: Into<Value> + Send + Clone,
	{
		let params_value = params.map(|p| p.into());
		self.send_raw_request(method, params_value.and_then(|v| v.as_array().cloned()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, BlockChainError> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), BlockChainError> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockWeb3TransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}
}

// Mock implementation of a Stellar transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub StellarTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, BlockChainError>;
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
	) -> Result<Value, BlockChainError>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, BlockChainError> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), BlockChainError> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockStellarTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}
}

// Mock implementation of a Horizon transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub HorizonTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, BlockChainError>;
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
	) -> Result<Value, BlockChainError>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn get_retry_policy(&self) -> Result<ExponentialBackoff, BlockChainError> {
		Ok(ExponentialBackoff::builder().build_with_max_retries(2))
	}

	fn set_retry_policy(&mut self, _: ExponentialBackoff) -> Result<(), BlockChainError> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockHorizonTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), BlockChainError> {
		Ok(())
	}
}
