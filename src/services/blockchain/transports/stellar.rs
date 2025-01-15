//! Stellar RPC transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar Core nodes
//! via JSON-RPC, supporting connection management and raw request functionality.

use crate::{models::Network, services::blockchain::BlockChainError};

use serde_json::{json, Value};
use stellar_rpc_client::Client as StellarHttpClient;

/// A client for interacting with Stellar Core RPC endpoints
pub struct StellarTransportClient {
	/// The underlying HTTP client for Stellar RPC requests
	pub client: StellarHttpClient,
	/// The base URL of the Stellar RPC endpoint
	pub url: String,
}

impl StellarTransportClient {
	/// Creates a new Stellar transport client by attempting to connect to available endpoints
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		// Filter stellar URLs with weight > 0 and sort by weight descending
		let mut stellar_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		stellar_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		for rpc_url in stellar_urls {
			match StellarHttpClient::new(rpc_url.url.as_str()) {
				Ok(client) => {
					// Test connection by fetching network info
					match client.get_network().await {
						Ok(_) => {
							return Ok(Self {
								client,
								url: rpc_url.url.clone(),
							})
						}
						Err(_) => continue,
					}
				}
				Err(_) => continue,
			}
		}

		Err(BlockChainError::connection_error(
			"All Stellar RPC URLs failed to connect".to_string(),
		))
	}

	/// Sends a raw JSON-RPC request to the Stellar Core endpoint
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Parameters to pass to the method
	///
	/// # Returns
	/// * `Result<Value, BlockChainError>` - JSON response or error
	pub async fn send_raw_request(
		&self,
		method: &str,
		params: Value,
	) -> Result<Value, BlockChainError> {
		let client = reqwest::Client::new();
		let url = self.url.clone();

		// Construct the JSON-RPC request
		let request_body = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": method,
			"params": params
		});

		let response = client
			.post(url)
			.header("Content-Type", "application/json")
			.json(&request_body) // Use .json() instead of .body() for proper serialization
			.send()
			.await
			.map_err(|e| BlockChainError::connection_error(e.to_string()))?;

		let json: Value = response
			.json()
			.await
			.map_err(|e| BlockChainError::connection_error(e.to_string()))?;

		Ok(json)
	}
}
