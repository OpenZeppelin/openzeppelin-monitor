//! Horizon API transport implementation for Stellar blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar's Horizon API,
//! supporting connection management and raw JSON-RPC requests.

use crate::{
	models::Network,
	services::blockchain::{
		transports::{BlockchainTransport, EndpointManager, RotatingTransport},
		BlockChainError,
	},
};

use async_trait::async_trait;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
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
	/// The retry policy for the transport
	retry_policy: ExponentialBackoff,
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

		// Default retry policy for Horizon transport
		let retry_policy = ExponentialBackoff::builder().build_with_max_retries(2);

		for rpc_url in horizon_urls.iter() {
			match HorizonHttpClient::new_from_str(&rpc_url.url) {
				Ok(client) => {
					let request = root::root();
					match client.request(request).await {
						Ok(_) => {
							let fallback_urls: Vec<String> = horizon_urls
								.iter()
								.filter(|url| url.url != rpc_url.url)
								.map(|url| url.url.clone())
								.collect();

							return Ok(Self {
								client: Arc::new(RwLock::new(client)),
								endpoint_manager: EndpointManager::new(
									rpc_url.url.as_ref(),
									fallback_urls,
								),
								retry_policy,
							});
						}
						Err(_) => {
							continue;
						}
					}
				}
				Err(_) => {
					continue;
				}
			}
		}

		Err(BlockChainError::connection_error(
			"All Horizon RPC URLs failed to connect".to_string(),
		))
	}
}

#[async_trait]
impl BlockchainTransport for HorizonTransportClient {
	/// Gets the current active URL
	///
	/// # Returns
	/// * `String` - The current active URL
	async fn get_current_url(&self) -> String {
		self.endpoint_manager.active_url.read().await.clone()
	}

	/// Sends a raw JSON-RPC request to the Horizon API endpoint
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

	/// Gets the retry policy for the transport
	///
	/// # Returns
	/// * `Result<ExponentialBackoff, BlockChainError>` - The retry policy
	fn get_retry_policy(&self) -> Result<ExponentialBackoff, BlockChainError> {
		Ok(self.retry_policy)
	}

	/// Sets the retry policy for the transport
	///
	/// # Arguments
	/// * `retry_policy` - The retry policy to set
	///
	/// # Returns
	/// * `Result<(), BlockChainError>` - The result of setting the retry policy
	fn set_retry_policy(
		&mut self,
		retry_policy: ExponentialBackoff,
	) -> Result<(), BlockChainError> {
		self.retry_policy = retry_policy;
		Ok(())
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
