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
use tokio::{sync::Mutex, time::timeout};
use tokio_tungstenite::connect_async;

use crate::{
	models::Network,
	services::blockchain::transports::{
		BlockchainTransport, RotatingTransport, TransientErrorRetryStrategy,
	},
};

use super::{config::WsConfig, connection::WebSocketConnection, endpoint_manager::EndpointManager};

/// Basic WebSocket transport client for blockchain interactions
///
/// This client provides a foundation for making WebSocket connections to blockchain nodes
/// with built-in support for:
/// - Connection pooling and reuse
/// - Automatic endpoint rotation on failure
/// - Configurable timeouts and reconnection policies
/// - Heartbeat monitoring
///
/// The client is thread-safe and can be shared across multiple tasks.
#[derive(Clone, Debug)]
pub struct WsTransportClient {
	/// WebSocket connection state and stream
	connection: Arc<Mutex<WebSocketConnection>>,
	/// Manages WebSocket endpoint rotation and request handling
	endpoint_manager: Arc<EndpointManager>,
	/// Configuration settings for WebSocket connections
	config: WsConfig,
}

impl WsTransportClient {
	/// Creates a new WebSocket transport client with automatic endpoint management
	///
	/// This constructor:
	/// 1. Filters and sorts WebSocket RPC URLs by weight
	/// 2. Tests each URL's connectivity with timeout
	/// 3. Uses the first working URL as active
	/// 4. Adds any additional URLs as fallbacks
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs, weights, and other details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let config = WsConfig::from_network(network);

		// Filter and sort WebSocket URLs by weight
		let mut ws_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "ws" && rpc_url.weight > 0)
			.collect();

		ws_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		if ws_urls.is_empty() {
			return Err(anyhow::anyhow!("No WebSocket URLs available"));
		}

		// Find first working URL and use rest as fallbacks
		let mut active_url = None;
		let mut fallback_urls = Vec::new();

		for rpc_url in ws_urls {
			let url = rpc_url.url.as_ref().to_string();
			if active_url.is_none()
				&& timeout(config.connection_timeout, connect_async(&url))
					.await
					.is_ok()
			{
				active_url = Some(url);
				continue;
			}
			fallback_urls.push(url);
		}

		let active_url =
			active_url.ok_or_else(|| anyhow::anyhow!("No working WebSocket URLs found"))?;
		let endpoint_manager = Arc::new(EndpointManager::new(&config, &active_url, fallback_urls));
		let connection = Arc::new(Mutex::new(WebSocketConnection::new()));

		let client = Self {
			connection,
			endpoint_manager,
			config,
		};

		// Initial connection
		client.connect().await?;

		Ok(client)
	}

	/// Establishes initial connection to the active endpoint
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or connection error
	async fn connect(&self) -> Result<(), anyhow::Error> {
		let url = self.endpoint_manager.get_active_url().await?;
		self.try_connect(&url).await
	}
}

#[async_trait]
impl BlockchainTransport for WsTransportClient {
	/// Retrieves the currently active RPC endpoint URL
	///
	/// This method is useful for monitoring which endpoint is currently in use,
	/// especially in scenarios with multiple failover URLs.
	///
	/// # Returns
	/// * `String` - The URL of the currently active endpoint
	async fn get_current_url(&self) -> String {
		self.endpoint_manager.active_url.read().await.clone()
	}

	/// Sends a JSON-RPC request to the blockchain node via WebSocket
	///
	/// This method handles the formatting of the JSON-RPC request, including:
	/// - Adding required JSON-RPC 2.0 fields
	/// - Converting parameters to the correct format
	/// - Connection health checks
	/// - Activity tracking
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method name to call
	/// * `params` - Optional parameters for the method call
	///
	/// # Returns
	/// * `Result<Value, anyhow::Error>` - JSON response or error with context
	///
	/// # Type Parameters
	/// * `P` - Parameter type that can be serialized to JSON
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		let mut connection = self.connection.lock().await;
		if !connection.is_connected() {
			return Err(anyhow::anyhow!("Not connected"));
		}
		connection.update_activity();

		self.endpoint_manager
			.send_raw_request(self, method, params)
			.await
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

#[async_trait]
impl RotatingTransport for WsTransportClient {
	/// Tests connectivity to a specific WebSocket endpoint
	///
	/// Attempts to establish a WebSocket connection with timeout and updates
	/// the connection state accordingly.
	///
	/// # Arguments
	/// * `url` - The WebSocket URL to test
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or detailed error message
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		let mut connection = self.connection.lock().await;

		match timeout(self.config.connection_timeout, connect_async(url)).await {
			Ok(Ok((ws_stream, _))) => {
				connection.stream = Some(ws_stream);
				connection.is_healthy = true;
				connection.update_activity();
				Ok(())
			}
			Ok(Err(e)) => {
				connection.is_healthy = false;
				Err(anyhow::anyhow!("Failed to connect: {}", e))
			}
			Err(_) => {
				connection.is_healthy = false;
				Err(anyhow::anyhow!("Connection timeout"))
			}
		}
	}

	/// Updates the active endpoint URL
	///
	/// This method is called when rotating to a new endpoint, typically
	/// after a failure of the current endpoint.
	///
	/// # Arguments
	/// * `url` - The new URL to use for subsequent connections
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		*self.endpoint_manager.active_url.write().await = url.to_string();
		Ok(())
	}
}
