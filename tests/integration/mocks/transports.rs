use mockall::mock;
use serde_json::Value;

use openzeppelin_monitor::services::blockchain::{
	BlockChainError, RotatingTransport, StellarTransport, Web3Transport,
};

// Mock implementation of a Web3 client.
// Represents the internal web3 client used for Ethereum interactions.
mock! {
	pub Web3Client {}

	impl Clone for Web3Client {
		fn clone(&self) -> Self;
	}
}

// Mock implementation of a Web3 transport client.
// Used for testing Ethereum/Web3-compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub Web3TransportClient {}

	#[async_trait::async_trait]
	impl Web3Transport for Web3TransportClient {
		async fn send_raw_request(
			&self,
			method: &str,
			params: Vec<Value>,
		) -> Result<Value, BlockChainError>;
	}

	impl Clone for Web3TransportClient {
		fn clone(&self) -> Self;
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

// Mock implementation of a Stellar client.
// Represents the internal stellar client used for Stellar interactions.
mock! {
	pub StellarClient {}

	impl Clone for StellarClient {
		fn clone(&self) -> Self;
	}
}

// Mock implementation of a Horizon transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub HorizonTransportClient {}

	#[async_trait::async_trait]
	impl StellarTransport for HorizonTransportClient {
		async fn send_raw_request(
			&self,
			method: &str,
			params: Option<Value>,
		) -> Result<Value, BlockChainError>;
	}

	impl Clone for HorizonTransportClient {
		fn clone(&self) -> Self;
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

// Mock implementation of a Stellar transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub StellarTransportClient {}

	#[async_trait::async_trait]
	impl StellarTransport for StellarTransportClient {
		async fn send_raw_request(
			&self,
			method: &str,
			params: Option<Value>,
		) -> Result<Value, BlockChainError>;
	}

	impl Clone for StellarTransportClient {
		fn clone(&self) -> Self;
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
