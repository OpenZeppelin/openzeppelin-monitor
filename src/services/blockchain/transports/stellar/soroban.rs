//! Stellar RPC transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Stellar Core nodes
//! via JSON-RPC, supporting connection management and raw request functionality.

use crate::{
	models::Network,
	services::blockchain::transports::{BlockchainTransport, EndpointManager, RotatingTransport},
};

use async_trait::async_trait;
use reqwest_retry::{policies::ExponentialBackoff, Jitter};
use serde::Serialize;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use stellar_rpc_client::Client as StellarHttpClient;
use tokio::sync::RwLock;

/// A client for interacting with Stellar Core RPC endpoints
#[derive(Clone, Debug)]
pub struct StellarTransportClient {
	/// The underlying HTTP client for Stellar RPC requests
	pub client: Arc<RwLock<StellarHttpClient>>,
	/// Manages RPC endpoint rotation and request handling
	endpoint_manager: EndpointManager,
	/// The retry policy for the transport
	retry_policy: ExponentialBackoff,
}

impl StellarTransportClient {
	/// Creates a new Stellar transport client by attempting to connect to available endpoints
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let mut stellar_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		stellar_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		// Default retry policy for Stellar transport
		let retry_policy = ExponentialBackoff::builder()
			.base(2)
			.retry_bounds(Duration::from_millis(100), Duration::from_secs(4))
			.jitter(Jitter::None)
			.build_with_max_retries(2);

		for rpc_url in stellar_urls.iter() {
			match StellarHttpClient::new(rpc_url.url.as_str()) {
				Ok(client) => match client.get_network().await {
					Ok(_) => {
						let fallback_urls: Vec<String> = stellar_urls
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
					Err(_) => continue,
				},
				Err(_) => continue,
			}
		}

		Err(anyhow::anyhow!("All RPC URLs failed to connect"))
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

#[async_trait]
impl RotatingTransport for StellarTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		match StellarHttpClient::new(url) {
			Ok(client) => {
				if client.get_network().await.is_ok() {
					Ok(())
				} else {
					Err(anyhow::anyhow!("Failed to connect: {}", url))
				}
			}
			Err(e) => Err(anyhow::anyhow!("Invalid URL: {}", e)),
		}
	}

	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		if let Ok(new_client) = StellarHttpClient::new(url) {
			let mut client = self.client.write().await;
			*client = new_client;

			// Update the endpoint manager's active URL as well
			let mut active_url = self.endpoint_manager.active_url.write().await;
			*active_url = url.to_string();

			Ok(())
		} else {
			Err(anyhow::anyhow!("Failed to create client: {}", url))
		}
	}
}
