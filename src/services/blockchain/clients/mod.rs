//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network
//!
//! Transport clients for each blockchain type
//! - Web3TransportClient for EVM
//! - StellarTransportClient for Stellar
mod evm {
	pub mod client;
}
mod stellar {
	pub mod client;
}

pub use evm::client::{EvmClient, EvmClientTrait};
pub use stellar::client::{StellarClient, StellarClientTrait};

use crate::services::blockchain::{BlockChainError, StellarTransportClient, Web3TransportClient};
use async_trait::async_trait;
use serde_json::Value;

/// Trait for sending raw requests to the Web3 transport client
#[async_trait]
pub trait Web3Transport {
	async fn send_raw_request(
		&self,
		method: &str,
		params: Vec<Value>,
	) -> Result<Value, BlockChainError>;
}

/// Implementation of the Web3 transport trait for the Web3 transport client
#[async_trait]
impl Web3Transport for Web3TransportClient {
	async fn send_raw_request(
		&self,
		method: &str,
		params: Vec<Value>,
	) -> Result<Value, BlockChainError> {
		self.send_raw_request(method, params).await
	}
}

/// Trait for sending raw requests to the Stellar transport client
#[async_trait]
pub trait StellarTransport {
	async fn send_raw_request(
		&self,
		method: &str,
		params: Option<Value>,
	) -> Result<Value, BlockChainError>;
}

/// Implementation of the Stellar transport trait for the Stellar transport client
#[async_trait]
impl StellarTransport for StellarTransportClient {
	async fn send_raw_request(
		&self,
		method: &str,
		params: Option<Value>,
	) -> Result<Value, BlockChainError> {
		self.send_raw_request(method, params).await
	}
}
