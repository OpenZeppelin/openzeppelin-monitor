//! Stellar RPC transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar Core nodes
//! via JSON-RPC, supporting connection management and raw request functionality.

use crate::{
	models::Network,
	services::blockchain::{
		transports::{BlockchainTransport, EndpointManager, RotatingTransport},
		BlockChainError,
	},
};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use stellar_rpc_client::Client as StellarHttpClient;
use tokio::sync::RwLock;

/// A client for interacting with Stellar Core RPC endpoints
#[derive(Clone)]
pub struct StellarTransportClient {
	/// The underlying HTTP client for Stellar RPC requests
	pub client: Arc<RwLock<StellarHttpClient>>,
	/// Manages RPC endpoint rotation and request handling
	endpoint_manager: EndpointManager,
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
		let mut stellar_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		stellar_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		for rpc_url in stellar_urls.iter() {
			match StellarHttpClient::new(rpc_url.url.as_str()) {
				Ok(client) => {
					if client.get_network().await.is_ok() {
						let fallback_urls: Vec<String> = stellar_urls
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
			"All Stellar RPC URLs failed to connect".to_string(),
		))
	}
}

#[async_trait]
impl BlockchainTransport for StellarTransportClient {
	/// Gets the current active URL
	///
	/// # Returns
	/// * `String` - The current active URL
	async fn get_current_url(&self) -> String {
		self.endpoint_manager.active_url.read().await.clone()
	}

	/// Sends a raw JSON-RPC request to the Stellar Core endpoint
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Parameters to pass to the method
	///
	/// # Returns
	/// * `Result<Value, BlockChainError>` - JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, BlockChainError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		self.endpoint_manager
			.send_raw_request(self, method, params)
			.await
	}
}

#[async_trait]
impl RotatingTransport for StellarTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError> {
		match StellarHttpClient::new(url) {
			Ok(client) => {
				if client.get_network().await.is_ok() {
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
		if let Ok(new_client) = StellarHttpClient::new(url) {
			let mut client = self.client.write().await;
			*client = new_client;

			// Update the endpoint manager's active URL as well
			let mut active_url = self.endpoint_manager.active_url.write().await;
			*active_url = url.to_string();

			Ok(())
		} else {
			Err(BlockChainError::connection_error(
				"Failed to create client".to_string(),
			))
		}
	}
}
