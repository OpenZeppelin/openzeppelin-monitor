//! Network transport implementations for blockchain clients.
//!
//! Provides concrete implementations for different blockchain network protocols:
//! - Web3 transport for EVM chains
//! - Horizon and Stellar RPC transport for Stellar

mod evm {
	pub mod web3;
}
mod stellar {
	pub mod horizon;
	pub mod soroban;
}
mod endpoint_manager;

use crate::services::blockchain::BlockChainError;
pub use endpoint_manager::EndpointManager;
pub use evm::web3::Web3TransportClient;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
pub use stellar::{horizon::HorizonTransportClient, soroban::StellarTransportClient};

use serde_json::{json, Value};

/// HTTP status codes that trigger RPC endpoint rotation
/// - 429: Too Many Requests - indicates rate limiting from the current endpoint
pub const ROTATE_ON_ERROR_CODES: [u16; 1] = [429];

/// Base trait for all blockchain transport clients
#[async_trait::async_trait]
pub trait BlockchainTransport: Send + Sync {
	/// Get the current URL being used by the transport
	async fn get_current_url(&self) -> String;

	/// Send a raw request to the blockchain
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, BlockChainError>
	where
		P: Into<Value> + Send + Clone + Serialize;

	/// Customizes the request for specific blockchain requirements
	async fn customize_request<P>(&self, method: &str, params: Option<P>) -> Value
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		// Default implementation for JSON-RPC
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": method,
			"params": params.map(|p| p.into())
		})
	}

	/// Sets the retry policy for the transport
	fn set_retry_policy(&mut self, retry_policy: ExponentialBackoff)
		-> Result<(), BlockChainError>;

	/// Gets the retry policy for the transport
	fn get_retry_policy(&self) -> Result<ExponentialBackoff, BlockChainError>;
}

/// Extension trait for transports that support URL rotation
#[async_trait::async_trait]
pub trait RotatingTransport: BlockchainTransport {
	/// Attempts to establish a connection with a new URL
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError>;

	/// Updates the client with a new URL
	async fn update_client(&self, url: &str) -> Result<(), BlockChainError>;
}
