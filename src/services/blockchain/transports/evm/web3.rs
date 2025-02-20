//! Web3 transport implementation for EVM blockchain interactions.
//!
//! This module provides a client implementation for interacting with EVM-compatible nodes
//! via Web3, supporting connection management and raw JSON-RPC request functionality.

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use web3::{transports::Http, Web3};

use crate::{
	models::Network,
	services::blockchain::{
		transports::{EndpointManager, RotatingTransport},
		BlockChainError,
	},
};

/// A client for interacting with EVM-compatible blockchain nodes via Web3
#[derive(Clone)]
pub struct Web3TransportClient {
	/// The underlying Web3 client for RPC requests
	pub client: Arc<RwLock<Web3<Http>>>,
	/// Manages RPC endpoint rotation and request handling
	endpoint_manager: EndpointManager,
}

impl Web3TransportClient {
	/// Creates a new Web3 transport client by attempting to connect to available endpoints
	///
	/// Tries each RPC URL in order of descending weight until a successful connection is
	/// established.
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		// Filter web3 URLs with weight > 0 and sort by weight descending
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		for rpc_url in rpc_urls.iter() {
			match Http::new(rpc_url.url.as_str()) {
				Ok(transport) => {
					let client = Web3::new(transport);
					if client.net().version().await.is_ok() {
						let fallback_urls: Vec<String> = rpc_urls
							.iter()
							.filter(|url| url.url != rpc_url.url)
							.map(|url| url.url.clone())
							.collect();

						return Ok(Self {
							client: Arc::new(RwLock::new(client)),
							endpoint_manager: EndpointManager::new(
								rpc_url.url.clone(),
								fallback_urls,
							),
						});
					}
				}
				Err(_) => continue,
			}
		}

		Err(BlockChainError::connection_error(
			"All RPC URLs failed to connect".to_string(),
		))
	}

	/// Sends a raw JSON-RPC request to the EVM node
	///
	/// This method sends a JSON-RPC request to the current active URL and handles
	/// connection errors by rotating to a fallback URL.
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Vector of parameters to pass to the method
	///
	/// # Returns
	/// * `Result<Value, BlockChainError>` - JSON response or error
	pub async fn send_raw_request(
		&self,
		method: &str,
		params: Vec<Value>,
	) -> Result<Value, BlockChainError> {
		self.endpoint_manager
			.send_raw_request(self, method, Some(json!(params)))
			.await
	}
}

#[async_trait::async_trait]
impl RotatingTransport for Web3TransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError> {
		match Http::new(url) {
			Ok(transport) => {
				let client = Web3::new(transport);
				if client.net().version().await.is_ok() {
					Ok(())
				} else {
					Err(BlockChainError::connection_error(
						"Failed to connect".to_string(),
					))
				}
			}
			Err(_) => Err(BlockChainError::connection_error("Invalid URL".to_string())),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), BlockChainError> {
		if let Ok(transport) = Http::new(url) {
			let new_client = Web3::new(transport);
			let mut client = self.client.write().await;
			*client = new_client;
			Ok(())
		} else {
			Err(BlockChainError::connection_error(
				"Failed to create client".to_string(),
			))
		}
	}
}
