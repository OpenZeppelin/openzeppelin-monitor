//! Stellar transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar-compatible nodes
//! by wrapping the HttpTransportClient. This allows for consistent behavior with other
//! transport implementations while providing specific Stellar-focused functionality.

use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::Value;

use crate::{
	models::Network,
	services::blockchain::transports::{
		BlockchainTransport, HttpTransportClient, RotatingTransport, TransportError,
	},
};

/// A client for interacting with Stellar-compatible blockchain nodes
///
/// This implementation wraps the HttpTransportClient to provide consistent
/// behavior with other transport implementations while offering Stellar-specific
/// functionality. It handles connection management, request retries, and
/// endpoint rotation for Stellar-based networks.
#[derive(Clone, Debug)]
pub struct StellarTransportClient {
	/// The underlying HTTP transport client that handles actual RPC communications
	http_client: HttpTransportClient,
}

impl StellarTransportClient {
	/// Creates a new Stellar transport client by initializing an HTTP transport client
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs and other network details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let test_connection_payload =
			Some(r#"{"id":1,"jsonrpc":"2.0","method":"getNetwork","params":[]}"#.to_string());
		let http_client = HttpTransportClient::new(network, test_connection_payload).await?;
		Ok(Self { http_client })
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for StellarTransportClient {
	/// Gets the current active RPC URL
	///
	/// # Returns
	/// * `String` - The currently active RPC endpoint URL
	async fn get_current_url(&self) -> String {
		self.http_client.get_current_url().await
	}

	/// Sends a raw JSON-RPC request to the Stellar node
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Optional parameters to pass with the request
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - The JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		self.http_client.send_raw_request(method, params).await
	}

	/// Update endpoint manager with a new client
	///
	/// # Arguments
	/// * `client` - The new client to use for the endpoint manager
	fn update_endpoint_manager_client(
		&mut self,
		client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		self.http_client.update_endpoint_manager_client(client)
	}
}

#[async_trait::async_trait]
impl RotatingTransport for StellarTransportClient {
	/// Tests connection to a specific URL
	///
	/// # Arguments
	/// * `url` - The URL to test connection with
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		self.http_client.try_connect(url).await
	}

	/// Updates the client to use a new URL
	///
	/// # Arguments
	/// * `url` - The new URL to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		self.http_client.update_client(url).await
	}
}
