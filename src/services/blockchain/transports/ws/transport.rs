//! Websocket transport implementation for blockchain interactions.
//!
//! This module provides a WebSocket client implementation for interacting with blockchain nodes
//! via WebSocket protocol, supporting connection checks and failover.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
	sync::atomic::{AtomicU64, Ordering},
	sync::Arc,
	time::Duration,
};
use tokio::{net::TcpStream, sync::Mutex, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::{
	models::Network,
	services::blockchain::{
		transports::{
			ws::{
				config::WsConfig, connection::WebSocketConnection,
				endpoint_manager::EndpointManager,
			},
			BlockchainTransport, RotatingTransport,
		},
		TransportError,
	},
};

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
	pub connection: Arc<Mutex<WebSocketConnection>>,
	/// Manages WebSocket endpoint rotation and request handling
	endpoint_manager: Arc<EndpointManager>,
	/// Configuration settings for WebSocket connections
	config: WsConfig,
	/// Counter for generating unique request IDs
	request_id_counter: Arc<AtomicU64>,
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
	pub async fn new(network: &Network, config: Option<WsConfig>) -> Result<Self, anyhow::Error> {
		let config = config.unwrap_or_else(|| WsConfig::from_network(network));

		// Filter and sort WebSocket URLs by weight
		let mut ws_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "ws_rpc" && rpc_url.weight > 0)
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
			if active_url.is_none() {
				match timeout(config.connection_timeout, connect_async(&url)).await {
					Ok(Ok(_)) => {
						active_url = Some(url.clone());
						// Picked as active; do not push to fallbacks
						continue;
					}
					Ok(Err(e)) => {
						tracing::warn!("WS connect failed for {}: {}", url, e);
						// try next URL
					}
					Err(e) => {
						tracing::warn!("WS connect timeout for {}: {}", url, e);
						// try next URL
					}
				}
			}
			// Either already have active, or this one failed and remains a fallback candidate
			fallback_urls.push(url);
		}

		let active_url =
			active_url.ok_or_else(|| anyhow::anyhow!("Failed to connect to any WebSocket URL"))?;
		let endpoint_manager = Arc::new(EndpointManager::new(&config, &active_url, fallback_urls));
		let connection = Arc::new(Mutex::new(WebSocketConnection::default()));

		let client = Self {
			connection,
			endpoint_manager,
			config,
			request_id_counter: Arc::new(AtomicU64::new(1)),
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

	/// Sends a JSON-RPC request via WebSocket
	///
	/// This method handles:
	/// - Connection state verification
	/// - Request formatting
	/// - Message sending with timeout
	/// - Response parsing
	/// - Automatic URL rotation on failure
	///
	/// # Arguments
	/// * `method` - The RPC method to call
	/// * `params` - Optional parameters for the method call
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		loop {
			let mut connection = self.connection.lock().await;
			if !connection.is_connected() {
				return Err(TransportError::network("Not connected", None, None));
			}
			connection.update_activity();

			// Helper function to handle connection errors
			let handle_connection_error = |connection: &mut WebSocketConnection| {
				connection.is_healthy = false;
				connection.stream = None;
			};

			let stream = match connection.stream.as_mut() {
				Some(stream) => stream,
				None => {
					handle_connection_error(&mut connection);
					drop(connection);
					if !self.endpoint_manager.should_rotate().await {
						return Err(TransportError::network("Not connected", None, None));
					}
					self.endpoint_manager.rotate_url(self).await.map_err(|e| {
						TransportError::url_rotation("Failed to rotate URL", Some(e.into()), None)
					})?;
					continue;
				}
			};

			// Generate unique request ID
			let request_id = self.request_id_counter.fetch_add(1, Ordering::SeqCst);
			let request_body = json!({
				"jsonrpc": "2.0",
				"id": request_id,
				"method": method,
				"params": params.clone().map(|p| p.into())
			});

			// Try to send the request
			if let Err(e) = stream
				.send(Message::Text(request_body.to_string().into()))
				.await
			{
				handle_connection_error(&mut connection);
				drop(connection);
				if !self.endpoint_manager.should_rotate().await {
					return Err(TransportError::network(
						format!("Failed to send request: {}", e),
						None,
						None,
					));
				}
				self.endpoint_manager.rotate_url(self).await.map_err(|e| {
					TransportError::url_rotation("Failed to rotate URL", Some(e.into()), None)
				})?;
				continue;
			}

			// Helper function to handle ping messages
			async fn handle_ping(
				stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
				data: Vec<u8>,
			) -> Result<(), anyhow::Error> {
				stream
					.send(Message::Pong(data.into()))
					.await
					.map_err(|e| anyhow::anyhow!("Failed to send pong: {}", e))
			}

			// Helper function to wait for response
			async fn wait_for_response(
				stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
				timeout: Duration,
			) -> Result<Message, anyhow::Error> {
				tokio::time::timeout(timeout, stream.next())
					.await
					.map_err(|_| anyhow::anyhow!("Response timeout"))?
					.ok_or_else(|| anyhow::anyhow!("Connection closed"))?
					.map_err(|e| anyhow::anyhow!("WebSocket error: {}", e))
			}

			// Wait for response with timeout, retrying until we get our specific response ID
			loop {
				match wait_for_response(stream, self.config.message_timeout).await {
					Ok(Message::Text(text)) => {
						// Parse the response
						let response: Value = serde_json::from_str(&text).map_err(|e| {
							TransportError::response_parse(
								"Failed to parse response",
								Some(e.into()),
								None,
							)
						})?;

						// Check if this response is for our request
						if let Some(response_id) = response.get("id").and_then(|v| v.as_u64()) {
							if response_id == request_id {
								// This is our response!
								return Ok(response);
							}
							// Not our response, continue waiting
							continue;
						}

						// No ID in response
						return Ok(response);
					}
					Ok(Message::Ping(data)) => {
						// Respond to ping and wait for actual response
						if let Err(e) = handle_ping(stream, data.to_vec()).await {
							handle_connection_error(&mut connection);
							drop(connection);
							if !self.endpoint_manager.should_rotate().await {
								return Err(TransportError::network(
									"Failed to send pong",
									Some(e.into()),
									None,
								));
							}
							self.endpoint_manager.rotate_url(self).await.map_err(|e| {
								TransportError::url_rotation(
									"Failed to rotate URL",
									Some(e.into()),
									None,
								)
							})?;
							break;
						}
					}
					Ok(_) => {
						handle_connection_error(&mut connection);
						drop(connection);
						if !self.endpoint_manager.should_rotate().await {
							return Err(TransportError::network(
								"Unexpected message type",
								None,
								None,
							));
						}
						self.endpoint_manager.rotate_url(self).await.map_err(|e| {
							TransportError::url_rotation(
								"Failed to rotate URL",
								Some(e.into()),
								None,
							)
						})?;
						break;
					}
					Err(e) => {
						handle_connection_error(&mut connection);
						drop(connection);
						if !self.endpoint_manager.should_rotate().await {
							return Err(TransportError::network(
								"Failed to handle response",
								Some(e.into()),
								None,
							));
						}
						self.endpoint_manager.rotate_url(self).await.map_err(|e| {
							TransportError::url_rotation(
								"Failed to rotate URL",
								Some(e.into()),
								None,
							)
						})?;
						break;
					}
				}
			}
		}
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
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		WsTransportClient::send_raw_request(self, method, params).await
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
