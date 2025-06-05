//! Midnight transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Midnight-compatible nodes
//! by wrapping the WsTransportClient. This allows for consistent behavior with other
//! transport implementations while providing specific Midnight-focused functionality.

use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
use serde_json::Value;

use crate::{
	models::Network,
	services::blockchain::{
		transports::{
			BlockchainTransport, RotatingTransport, TransientErrorRetryStrategy, WsTransportClient,
		},
		TransportError, WsConfig,
	},
};

/// A client for interacting with Midnight-compatible blockchain nodes via WebSocket
///
/// This implementation wraps the WsTransportClient to provide consistent
/// behavior with other transport implementations while offering Midnight-specific
/// functionality. It handles WebSocket connection management, message handling,
/// and endpoint rotation for Midnight-based networks.
#[derive(Clone, Debug)]
pub struct MidnightTransportClient {
	/// The underlying WebSocket transport client that handles actual RPC communications
	ws_client: WsTransportClient,
}

impl MidnightTransportClient {
	/// Creates a new Midnight transport client by initializing a WebSocket transport client
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs and other network details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn new(network: &Network, config: Option<WsConfig>) -> Result<Self, anyhow::Error> {
		let ws_client = WsTransportClient::new(network, config).await?;
		Ok(Self { ws_client })
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MidnightTransportClient {
	/// Gets the current active RPC URL
	///
	/// # Returns
	/// * `String` - The currently active RPC endpoint URL
	async fn get_current_url(&self) -> String {
		self.ws_client.get_current_url().await
	}

	/// Sends a raw JSON-RPC request to the Midnight node via WebSocket
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Optional parameters to pass with the request
	///
	/// # Returns
	/// * `Result<Value, anyhow::Error>` - The JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		self.ws_client.send_raw_request(method, params).await
	}

	/// Sets a new retry policy for the transport
	///
	/// Note: Not applicable for WebSocket transport
	fn set_retry_policy(
		&mut self,
		_retry_policy: ExponentialBackoff,
		_retry_strategy: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!(
			"`set_retry_policy` not implemented for WebSocket transport"
		))
	}

	/// Update endpoint manager with a new client
	///
	/// Note: Not applicable for WebSocket transport
	fn update_endpoint_manager_client(
		&mut self,
		_client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!(
			"`update_endpoint_manager_client` not implemented for WebSocket transport"
		))
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MidnightTransportClient {
	/// Tests connection to a specific WebSocket URL
	///
	/// # Arguments
	/// * `url` - The WebSocket URL to test connection with
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		self.ws_client.try_connect(url).await
	}

	/// Updates the client to use a new WebSocket URL
	///
	/// # Arguments
	/// * `url` - The new WebSocket URL to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		self.ws_client.update_client(url).await
	}
}
