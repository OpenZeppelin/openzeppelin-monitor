//! Network transport implementations for blockchain clients.
//!
//! Provides concrete implementations for different blockchain network protocols:
//! - Web3 transport for EVM chains
//! - Horizon and Stellar RPC transport for Stellar

mod evm {
	pub mod web3;
}
mod stellar {
	pub mod horizon;
	pub mod soroban;
}

use crate::services::blockchain::BlockChainError;

pub use evm::web3::Web3TransportClient;
use log::debug;
use serde_json::{json, Value};
use std::sync::Arc;
pub use stellar::{horizon::HorizonTransportClient, soroban::StellarTransportClient};
use tokio::sync::RwLock;

/// HTTP status codes that trigger RPC endpoint rotation
/// - 429: Too Many Requests - indicates rate limiting from the current endpoint
const ROTATE_ON_ERROR_CODES: [u16; 1] = [429];

/// Trait for transport clients that support URL rotation and requests
#[async_trait::async_trait]
pub trait RotatingTransport: Send + Sync {
	/// Attempts to establish a connection with a new URL
	async fn try_connect(&self, url: &str) -> Result<(), BlockChainError>;
	/// Updates the client with a new URL
	async fn update_client(&self, url: &str) -> Result<(), BlockChainError>;
	/// Customizes the request for specific blockchain requirements
	async fn customize_request(&self, method: &str, params: Option<Value>) -> Value {
		// Default implementation for JSON-RPC
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": method,
			"params": params
		})
	}
}

#[derive(Clone)]
pub struct EndpointManager {
	active_url: Arc<RwLock<String>>,
	fallback_urls: Arc<RwLock<Vec<String>>>,
	rotation_lock: Arc<tokio::sync::Mutex<()>>,
}

/// Manages the rotation of blockchain RPC endpoints
///
/// Provides methods for rotating between multiple URLs and sending requests to the active endpoint
/// with automatic fallback to other URLs on failure.
impl EndpointManager {
	/// Creates a new rotating URL client
	///
	/// # Arguments
	/// * `active_url` - The initial active URL
	/// * `fallback_urls` - A list of fallback URLs to rotate to
	///
	/// # Returns
	pub fn new(active_url: String, fallback_urls: Vec<String>) -> Self {
		Self {
			active_url: Arc::new(RwLock::new(active_url)),
			fallback_urls: Arc::new(RwLock::new(fallback_urls)),
			rotation_lock: Arc::new(tokio::sync::Mutex::new(())),
		}
	}

	/// Rotates to the next available URL
	///
	/// # Arguments
	/// * `transport` - The transport client implementing the RotatingTransport trait
	///
	/// # Returns
	/// * `Result<(), BlockChainError>` - The result of the rotation operation
	pub async fn rotate_url<T: RotatingTransport>(
		&self,
		transport: &T,
	) -> Result<(), BlockChainError> {
		// Acquire rotation lock first
		let _guard = self.rotation_lock.lock().await;

		// Scope the write lock to release it as soon as possible
		let new_url = {
			let mut fallback_urls = self.fallback_urls.write().await;
			if fallback_urls.is_empty() {
				return Err(BlockChainError::connection_error(
					"No fallback URLs available".to_string(),
				));
			}
			fallback_urls.remove(0)
		}; // Lock is released here

		match transport.try_connect(&new_url).await {
			Ok(_) => {
				transport.update_client(&new_url).await?;

				// Acquire locks only when needed and in a smaller scope
				{
					let mut active_url = self.active_url.write().await;
					let mut fallback_urls = self.fallback_urls.write().await;

					fallback_urls.push(active_url.clone());

					debug!(
						"Rotating RPC endpoint from {} to {}",
						active_url.as_str(),
						new_url
					);

					*active_url = new_url;
				}
				Ok(())
			}
			Err(_) => {
				// Re-acquire lock to push back the failed URL
				let mut fallback_urls = self.fallback_urls.write().await;
				fallback_urls.push(new_url);
				Err(BlockChainError::connection_error(
					"Failed to connect to fallback URL".to_string(),
				))
			}
		}
	}

	/// Sends a raw request to the blockchain RPC endpoint with automatic URL rotation on failure
	///
	/// # Arguments
	/// * `transport` - The transport client implementing the RotatingTransport trait
	/// * `method` - The RPC method name to call
	/// * `params` - The parameters for the RPC method call as a JSON Value
	///
	/// # Returns
	/// * `Result<Value, BlockChainError>` - The JSON response from the RPC endpoint or an error
	///
	/// # Behavior
	/// - Automatically rotates to fallback URLs if the request fails with specific status codes
	///   (e.g., 429)
	/// - Retries the request with the new URL after rotation
	/// - Returns the first successful response or an error if all attempts fail
	pub async fn send_raw_request<T: RotatingTransport>(
		&self,
		transport: &T,
		method: &str,
		params: Option<Value>,
	) -> Result<Value, BlockChainError> {
		let client = reqwest::Client::new();

		loop {
			let current_url = self.active_url.read().await.clone();
			let request_body = transport.customize_request(method, params.clone()).await;

			let response = client
				.post(current_url.as_str())
				.header("Content-Type", "application/json")
				.json(&request_body)
				.send()
				.await
				.map_err(|e| BlockChainError::request_error(e.to_string()))?;

			let status = response.status();
			if !status.is_success() {
				let error_body = response.text().await.unwrap_or_default();

				// Check fallback URLs availability without holding the lock
				let should_rotate = {
					let fallback_urls = self.fallback_urls.read().await;
					!fallback_urls.is_empty() && ROTATE_ON_ERROR_CODES.contains(&status.as_u16())
				};

				if should_rotate && self.rotate_url(transport).await.is_ok() {
					continue;
				}

				return Err(BlockChainError::request_error(format!(
					"HTTP error {}: {}",
					status, error_body
				)));
			}

			let json: Value = response
				.json()
				.await
				.map_err(|e| BlockChainError::request_error(e.to_string()))?;

			return Ok(json);
		}
	}
}
