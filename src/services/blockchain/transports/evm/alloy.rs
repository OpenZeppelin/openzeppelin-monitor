//! Alloy transport implementation for EVM blockchain interactions.
//!
//! This module provides a client implementation for interacting with EVM-compatible nodes
//! via alloy, supporting connection management and raw JSON-RPC request functionality.

use alloy::rpc::client::{ClientBuilder, RpcClient};
use reqwest_retry::{policies::ExponentialBackoff, Jitter};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use url::Url;

use crate::{
	models::Network,
	services::blockchain::transports::{BlockchainTransport, EndpointManager, RotatingTransport},
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
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		// Default retry policy for Alloy transport
		let retry_policy = ExponentialBackoff::builder()
			.base(2)
			.retry_bounds(Duration::from_millis(100), Duration::from_secs(4))
			.jitter(Jitter::None)
			.build_with_max_retries(2);

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

		Err(anyhow::anyhow!("All RPC URLs failed to connect"))
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
	/// * `Result<Value, anyhow::Error>` - JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		let response = self
			.endpoint_manager
			.send_raw_request(self, method, params)
			.await?;

		Ok(response)
	}

	/// Gets the retry policy for the transport
	///
	/// # Returns
	/// * `Result<ExponentialBackoff, anyhow::Error>` - The retry policy
	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(self.retry_policy)
	}

	/// Sets the retry policy for the transport
	///
	/// # Arguments
	/// * `retry_policy` - The retry policy to set
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - The result of setting the retry policy
	fn set_retry_policy(&mut self, retry_policy: ExponentialBackoff) -> Result<(), anyhow::Error> {
		self.retry_policy = retry_policy;
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for AlloyTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		let url = match Url::parse(url) {
			Ok(url) => url,
			Err(_) => return Err(anyhow::anyhow!("Invalid URL: {}", url.to_string())),
		};

		let url_clone = url.clone();
		let client = ClientBuilder::default().http(url);

		match client.request_noparams::<String>("net_version").await {
			Ok(_) => Ok(()),
			Err(_) => Err(anyhow::anyhow!(
				"Failed to connect: {}",
				url_clone.to_string()
			)),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		let parsed_url = match Url::parse(url) {
			Ok(url) => url,
			Err(_) => return Err(anyhow::anyhow!("Invalid URL: {}", url)),
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
