//! Websocket transport implementation for blockchain interactions.
//!
//! This module provides a WebSocket client implementation for interacting with blockchain nodes
//! via WebSocket protocol, supporting connection checks and failover.

use async_trait::async_trait;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::connect_async;

use crate::{models::Network, services::blockchain::transports::BlockchainTransport};

use super::TransientErrorRetryStrategy;

/// Basic WebSocket transport client for blockchain interactions
#[derive(Clone, Debug)]
pub struct WsTransportClient {
	/// Current active URL
	pub active_url: Arc<Mutex<String>>,
	/// List of fallback URLs
	pub fallback_urls: Arc<Mutex<Vec<String>>>,
}

impl WsTransportClient {
	/// Creates a new WebSocket transport client
	///
	/// This method:
	/// 1. Filters and sorts WebSocket RPC URLs by weight
	/// 2. Tests each URL's connectivity using check_connection
	/// 3. Uses the first working URL as active
	/// 4. Adds any additional working URLs as fallbacks
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or error if no working URLs found
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "ws_rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		if rpc_urls.is_empty() {
			return Err(anyhow::anyhow!("No valid WebSocket RPC URLs found"));
		}

		// Try each URL in order of weight until we find one that works
		let mut active_url = None;
		let mut fallback_urls = Vec::new();

		for rpc_url in rpc_urls {
			let url = rpc_url.url.as_ref().to_string();
			let temp_client = Self {
				active_url: Arc::new(Mutex::new(url.clone())),
				fallback_urls: Arc::new(Mutex::new(Vec::new())),
			};

			if temp_client.check_connection().await.is_ok() {
				if active_url.is_none() {
					active_url = Some(url);
				} else {
					fallback_urls.push(url);
				}
			}
		}

		let active_url =
			active_url.ok_or_else(|| anyhow::anyhow!("No working WebSocket URLs found"))?;

		Ok(Self {
			active_url: Arc::new(Mutex::new(active_url)),
			fallback_urls: Arc::new(Mutex::new(fallback_urls)),
		})
	}

	/// Checks if the WebSocket connection is alive by attempting to establish a connection
	pub async fn check_connection(&self) -> Result<(), anyhow::Error> {
		let url = self.active_url.lock().await.clone();

		// Attempt to establish a WebSocket connection
		match connect_async(url).await {
			Ok((_, _)) => Ok(()),
			Err(e) => Err(anyhow::anyhow!("Failed to connect: {}", e)),
		}
	}

	/// Tries to connect to a fallback URL if the current one fails
	pub async fn try_fallback(&self) -> Result<(), anyhow::Error> {
		let mut fallback_urls = self.fallback_urls.lock().await;
		if fallback_urls.is_empty() {
			return Err(anyhow::anyhow!("No fallback URLs available"));
		}

		// Get the first fallback URL
		let url = fallback_urls[0].clone();

		if connect_async(&url).await.is_ok() {
			// Update active URL if connection successful
			let mut active = self.active_url.lock().await;
			*active = url.clone();

			// Remove the used URL from fallback_urls
			fallback_urls.remove(0);

			Ok(())
		} else {
			// Remove the failed URL from fallback_urls
			fallback_urls.remove(0);
			Err(anyhow::anyhow!("Failed to connect to fallback URL"))
		}
	}
}

#[async_trait]
impl BlockchainTransport for WsTransportClient {
	/// Retrieves the currently active WebSocket endpoint URL
	async fn get_current_url(&self) -> String {
		self.active_url.lock().await.clone()
	}

	/// Sends a JSON-RPC request to the blockchain node via WebSocket
	///
	/// Note: This is a placeholder implementation as WebSocket communication
	/// is handled by the substrate client.
	async fn send_raw_request<P>(
		&self,
		_method: &str,
		_params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		Err(anyhow::anyhow!("`send_raw_request` not implemented"))
	}

	/// Updates the retry policy configuration
	///
	/// Note: Not applicable for WebSocket transport
	fn set_retry_policy(
		&mut self,
		_retry_policy: ExponentialBackoff,
		_retry_strategy: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!("`set_retry_policy` not implemented"))
	}

	/// Update endpoint manager with a new client
	///
	/// Note: Not applicable for WebSocket transport
	fn update_endpoint_manager_client(
		&mut self,
		_client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!(
			"`update_endpoint_manager_client` not implemented"
		))
	}
}
