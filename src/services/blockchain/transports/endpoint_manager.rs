//! Manages the rotation of blockchain RPC endpoints
//!
//! Provides methods for rotating between multiple URLs and sending requests to the active endpoint
//! with automatic fallback to other URLs on failure.
use log::debug;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::services::blockchain::{
	transports::{RotatingTransport, ROTATE_ON_ERROR_CODES},
	BlockChainError,
};

/// Manages the rotation of blockchain RPC endpoints
///
/// Provides methods for rotating between multiple URLs and sending requests to the active endpoint
/// with automatic fallback to other URLs on failure.
///
/// # Fields
/// * `active_url` - The current active URL
/// * `fallback_urls` - A list of fallback URLs to rotate to
/// * `rotation_lock` - A lock for managing the rotation process
#[derive(Clone)]
pub struct EndpointManager {
	pub active_url: Arc<RwLock<String>>,
	pub fallback_urls: Arc<RwLock<Vec<String>>>,
	rotation_lock: Arc<tokio::sync::Mutex<()>>,
}

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

		let current_active = self.active_url.read().await.clone();

		// Get a different URL from fallbacks
		let new_url = {
			let mut fallback_urls = self.fallback_urls.write().await;
			if fallback_urls.is_empty() {
				return Err(BlockChainError::connection_error(
					"No fallback URLs available".to_string(),
				));
			}

			// Find first URL that's different from current
			let idx = fallback_urls.iter().position(|url| url != &current_active);

			match idx {
				Some(pos) => fallback_urls.remove(pos),
				None => {
					return Err(BlockChainError::connection_error(
						"No fallback URLs available".to_string(),
					));
				}
			}
		};

		match transport.try_connect(&new_url).await {
			Ok(_) => {
				transport.update_client(&new_url).await?;

				// Update URLs
				{
					let mut active_url = self.active_url.write().await;
					let mut fallback_urls = self.fallback_urls.write().await;
					debug!(
						"Successful rotation - from: {}, to: {}",
						current_active, new_url
					);
					fallback_urls.push(current_active);
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
	pub async fn send_raw_request<
		T: RotatingTransport,
		P: Into<Value> + Send + Clone + Serialize,
	>(
		&self,
		transport: &T,
		method: &str,
		params: Option<P>,
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
