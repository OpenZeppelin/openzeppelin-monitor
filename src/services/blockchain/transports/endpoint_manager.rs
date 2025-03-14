//! Manages the rotation of blockchain RPC endpoints
//!
//! Provides methods for rotating between multiple URLs and sending requests to the active endpoint
//! with automatic fallback to other URLs on failure.
use anyhow::Context;
use reqwest_middleware::ClientBuilder;
use reqwest_retry::RetryTransientMiddleware;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::Level;

use crate::services::blockchain::transports::{RotatingTransport, ROTATE_ON_ERROR_CODES};

/// Manages the rotation of blockchain RPC endpoints
///
/// Provides methods for rotating between multiple URLs and sending requests to the active endpoint
/// with automatic fallback to other URLs on failure.
///
/// # Fields
/// * `active_url` - The current active URL
/// * `fallback_urls` - A list of fallback URLs to rotate to
/// * `rotation_lock` - A lock for managing the rotation process
#[derive(Clone, Debug)]
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
	pub fn new(active_url: &str, fallback_urls: Vec<String>) -> Self {
		Self {
			active_url: Arc::new(RwLock::new(active_url.to_string())),
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
	/// * `Result<(), anyhow::Error>` - The result of the rotation operation
	pub async fn rotate_url<T: RotatingTransport>(
		&self,
		transport: &T,
	) -> Result<(), anyhow::Error> {
		// Acquire rotation lock first
		let _guard = self.rotation_lock.lock().await;

		let current_active = self.active_url.read().await.clone();

		// Get a different URL from fallbacks
		let new_url = {
			let mut fallback_urls = self.fallback_urls.write().await;
			if fallback_urls.is_empty() {
				return Err(anyhow::anyhow!("No fallback URLs available"));
			}

			// Find first URL that's different from current
			let idx = fallback_urls.iter().position(|url| url != &current_active);

			match idx {
				Some(pos) => fallback_urls.remove(pos),
				None => {
					return Err(anyhow::anyhow!("No fallback URLs available"));
				}
			}
		};

		if transport.try_connect(&new_url).await.is_ok() {
			transport.update_client(&new_url).await?;

			// Update URLs
			{
				let mut active_url = self.active_url.write().await;
				let mut fallback_urls = self.fallback_urls.write().await;
				tracing::debug!(
					"Successful rotation - from: {}, to: {}",
					current_active,
					new_url
				);
				fallback_urls.push(current_active);
				*active_url = new_url;
			}
			Ok(())
		} else {
			// Re-acquire lock to push back the failed URL
			let mut fallback_urls = self.fallback_urls.write().await;
			fallback_urls.push(new_url);
			Err(anyhow::anyhow!("Failed to connect to fallback URL"))
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
	/// * `Result<Value, anyhow::Error>` - The JSON response from the RPC endpoint or an error
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
	) -> Result<Value, anyhow::Error> {
		// TODO: initialise this outside of the function
		let retry_policy = transport.get_retry_policy()?;
		let client = ClientBuilder::new(reqwest::Client::new())
			.with(
				RetryTransientMiddleware::new_with_policy(retry_policy)
					.with_retry_log_level(Level::WARN),
			)
			.build();

		loop {
			let current_url = self.active_url.read().await.clone();
			let request_body = transport.customize_request(method, params.clone()).await;

			let response = client
				.post(current_url.as_str())
				.header("Content-Type", "application/json")
				.body(
					serde_json::to_string(&request_body)
						.map_err(|e| anyhow::anyhow!("Failed to parse request: {}", e))?,
				)
				.send()
				.await
				.map_err(|e| anyhow::anyhow!("Failed to send request: {}", e))?;

			let status = response.status();
			if !status.is_success() {
				let error_body = response.text().await.unwrap_or_default();

				// Check fallback URLs availability without holding the lock
				let should_rotate = {
					let fallback_urls = self.fallback_urls.read().await;
					!fallback_urls.is_empty() && ROTATE_ON_ERROR_CODES.contains(&status.as_u16())
				};

				if should_rotate {
					let rotate_result = self
						.rotate_url(transport)
						.await
						.with_context(|| "Failed to rotate URL");

					if rotate_result.is_ok() {
						continue;
					}
				}

				return Err(anyhow::anyhow!("HTTP error {}: {}", status, error_body));
			}

			let json: Value = response
				.json()
				.await
				.map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

			return Ok(json);
		}
	}
}
