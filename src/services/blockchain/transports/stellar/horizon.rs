//! Horizon API transport implementation for Stellar blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar's Horizon API,
//! supporting connection management and raw JSON-RPC requests.

use crate::{
	models::Network,
	services::blockchain::{
		transports::{EndpointManager, RotatingTransport},
		BlockChainError,
	},
};

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use stellar_horizon::{
	api::root,
	client::{HorizonClient as HorizonClientTrait, HorizonHttpClient},
};
use tokio::sync::RwLock;

/// A client for interacting with Stellar's Horizon API endpoints
#[derive(Clone)]
pub struct HorizonTransportClient {
	/// The underlying HTTP client for Horizon API requests
	pub client: Arc<RwLock<HorizonHttpClient>>,
	/// Manages RPC endpoint rotation and request handling
	endpoint_manager: EndpointManager,
}

impl HorizonTransportClient {
	/// Creates a new Horizon transport client by attempting to connect to available endpoints
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		let mut horizon_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "horizon" && rpc_url.weight > 0)
			.collect();

		horizon_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		for rpc_url in horizon_urls.iter() {
			match HorizonHttpClient::new_from_str(&rpc_url.url) {
				Ok(client) => {
					let request = root::root();
					if client.request(request).await.is_ok() {
						let fallback_urls: Vec<String> = horizon_urls
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
			"All Horizon RPC URLs failed to connect".to_string(),
		))
	}

	/// Sends a raw JSON-RPC request to the Horizon API endpoint
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
		params: Option<Value>,
	) -> Result<Value, BlockChainError> {
		self.endpoint_manager
			.send_raw_request(self, method, params)
			.await
	}
}

#[async_trait]
impl RotatingTransport for HorizonTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError> {
		match HorizonHttpClient::new_from_str(url) {
			Ok(client) => {
				let request = root::root();
				if client.request(request).await.is_ok() {
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
		if let Ok(new_client) = HorizonHttpClient::new_from_str(url) {
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
