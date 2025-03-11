//! Alloy transport implementation for EVM blockchain interactions.
//!
//! This module provides a client implementation for interacting with EVM-compatible nodes
//! via alloy, supporting connection management and raw JSON-RPC request functionality.

use alloy::rpc::client::{ClientBuilder, RpcClient};
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use url::Url;

use crate::{
	models::Network,
	services::blockchain::{
		transports::{BlockchainTransport, EndpointManager, RotatingTransport},
		BlockChainError,
	},
};

/// A client for interacting with EVM-compatible blockchain nodes via alloy
#[derive(Clone, Debug)]
pub struct AlloyTransportClient {
	/// The underlying alloy client for RPC requests
	pub client: Arc<RwLock<RpcClient>>,
	/// Manages RPC endpoint rotation and request handling
	endpoint_manager: EndpointManager,
	/// The retry policy for the transport
	retry_policy: ExponentialBackoff,
}

impl AlloyTransportClient {
	/// Creates a new alloy transport client by attempting to connect to available endpoints
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
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		// Default retry policy for alloy transport
		let retry_policy = ExponentialBackoff::builder().build_with_max_retries(2);

		for rpc_url in rpc_urls.iter() {
			let url = match Url::parse(&rpc_url.url) {
				Ok(url) => url,
				Err(_) => continue,
			};
			let client = ClientBuilder::default().http(url);
			match client.request_noparams::<String>("net_version").await {
				Ok(_) => {
					let fallback_urls: Vec<String> = rpc_urls
						.iter()
						.filter(|url| url.url != rpc_url.url)
						.map(|url| url.url.clone())
						.collect();

					return Ok(Self {
						client: Arc::new(RwLock::new(client)),
						endpoint_manager: EndpointManager::new(rpc_url.url.as_ref(), fallback_urls),
						retry_policy,
					});
				}
				Err(_) => {
					continue;
				}
			}
		}

		Err(BlockChainError::connection_error(
			"All RPC URLs failed to connect".to_string(),
		))
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for AlloyTransportClient {
	/// Gets the current active URL
	///
	/// # Returns
	/// * `String` - The current active URL
	async fn get_current_url(&self) -> String {
		self.endpoint_manager.active_url.read().await.clone()
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

#[async_trait::async_trait]
impl RotatingTransport for AlloyTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError> {
		let url = match Url::parse(url) {
			Ok(url) => url,
			Err(_) => return Err(BlockChainError::connection_error("Invalid URL".to_string())),
		};

		let client = ClientBuilder::default().http(url);

		match client.request_noparams::<String>("net_version").await {
			Ok(_) => Ok(()),
			Err(_) => Err(BlockChainError::connection_error(
				"Failed to connect".to_string(),
			)),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), BlockChainError> {
		let parsed_url = match Url::parse(url) {
			Ok(url) => url,
			Err(_) => return Err(BlockChainError::connection_error("Invalid URL".to_string())),
		};
		let new_client = ClientBuilder::default().http(parsed_url);

		let mut client = self.client.write().await;
		*client = new_client;

		// Update the endpoint manager's active URL as well
		let mut active_url = self.endpoint_manager.active_url.write().await;
		*active_url = url.to_string();

		Ok(())
	}
}
