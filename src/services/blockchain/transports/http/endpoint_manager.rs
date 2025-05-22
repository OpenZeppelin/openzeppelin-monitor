//! Manages the rotation of blockchain HTTP RPC endpoints
//!
//! Provides methods for rotating between multiple URLs and sending requests to the active endpoint
//! with automatic fallback to other URLs on failure.
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware, RetryableStrategy};
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::services::blockchain::transports::{RotatingTransport, ROTATE_ON_ERROR_CODES};

/// Manages the rotation of blockchain RPC endpoints
///
/// Provides methods for rotating between multiple URLs and sending requests to the active endpoint
/// with automatic fallback to other URLs on failure.
///
/// # Fields
/// * `active_url` - The current active URL
/// * `fallback_urls` - A list of fallback URLs to rotate to
/// * `client` - The client to use for the endpoint manager
/// * `rotation_lock` - A lock for managing the rotation process
#[derive(Clone, Debug)]
pub struct EndpointManager {
	pub active_url: Arc<RwLock<String>>,
	pub fallback_urls: Arc<RwLock<Vec<String>>>,
	client: ClientWithMiddleware,
	rotation_lock: Arc<tokio::sync::Mutex<()>>,
}

impl EndpointManager {
	/// Creates a new rotating URL client
	///
	/// # Arguments
	/// * `client` - The client to use for the endpoint manager
	/// * `active_url` - The initial active URL
	/// * `fallback_urls` - A list of fallback URLs to rotate to
	///
	/// # Returns
	pub fn new(client: ClientWithMiddleware, active_url: &str, fallback_urls: Vec<String>) -> Self {
		Self {
			active_url: Arc::new(RwLock::new(active_url.to_string())),
			fallback_urls: Arc::new(RwLock::new(fallback_urls)),
			rotation_lock: Arc::new(tokio::sync::Mutex::new(())),
			client,
		}
	}

	/// Updates the client with a new client
	///
	/// Useful for updating the client with a new retry policy or strategy
	///
	/// # Arguments
	/// * `client` - The new client to use for the endpoint manager
	pub fn update_client(&mut self, client: ClientWithMiddleware) {
		self.client = client;
	}

	/// Updates the retry policy for the client
	///
	/// Constructs a new client with the given retry policy and strategy
	/// and updates the endpoint manager with the new client
	///
	/// # Arguments
	/// * `retry_policy` - The new retry policy to use for the client
	/// * `retry_strategy` - The new retry strategy to use for the client
	pub fn set_retry_policy<R: RetryableStrategy + Send + Sync + 'static>(
		&mut self,
		retry_policy: ExponentialBackoff,
		retry_strategy: R,
	) {
		let updated_client = ClientBuilder::from_client(self.client.clone())
			.with(RetryTransientMiddleware::new_with_policy_and_strategy(
				retry_policy,
				retry_strategy,
			))
			.build();
		self.update_client(updated_client);
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

	/// Determines if rotation should be attempted and executes it if needed
	///
	/// This method encapsulates the logic for:
	/// 1. Checking if rotation is possible (fallback URLs exist)
	/// 2. Determining if rotation should occur based on error conditions
	/// 3. Executing the rotation if conditions are met
	///
	/// # Arguments
	/// * `transport` - The transport client implementing the RotatingTransport trait
	/// * `should_check_status` - If true, checks HTTP status against ROTATE_ON_ERROR_CODES
	/// * `status` - The HTTP status code to check (only used if should_check_status is true)
	///
	/// # Returns
	/// * `Ok(true)` - Rotation was needed and succeeded, caller should retry the request
	/// * `Ok(false)` - No rotation was needed or possible
	/// * `Err` - Rotation was attempted but failed
	async fn should_attempt_rotation<T: RotatingTransport>(
		&self,
		transport: &T,
		should_check_status: bool,
		status: Option<u16>,
	) -> Result<bool, anyhow::Error> {
		// Check fallback URLs availability without holding the lock
		let should_rotate = {
			let fallback_urls = self.fallback_urls.read().await;
			!fallback_urls.is_empty()
				&& (!should_check_status
					|| status.is_some_and(|s| ROTATE_ON_ERROR_CODES.contains(&s)))
		};

		if should_rotate {
			match self.rotate_url(transport).await {
				Ok(_) => Ok(true), // Rotation successful, continue loop
				Err(e) => Err(e.context("Failed to rotate URL")),
			}
		} else {
			Ok(false) // No rotation needed
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
		loop {
			let current_url = self.active_url.read().await.clone();
			let request_body = transport.customize_request(method, params.clone()).await;

			let response = match self
				.client
				.post(current_url.as_str())
				.header("Content-Type", "application/json")
				.body(
					serde_json::to_string(&request_body)
						.map_err(|e| anyhow::anyhow!("Failed to parse request: {}", e))?,
				)
				.send()
				.await
			{
				Ok(resp) => resp,
				Err(e) => {
					tracing::warn!("Network error while sending request: {}", e);
					// Try rotation for network errors without status check
					if self.should_attempt_rotation(transport, false, None).await? {
						continue;
					}
					return Err(anyhow::anyhow!("Failed to send request: {}", e));
				}
			};

			let status = response.status();
			if !status.is_success() {
				let error_body = response.text().await.unwrap_or_default();
				tracing::warn!("Request failed with status {}: {}", status, error_body);

				// Try rotation with status code check
				if self
					.should_attempt_rotation(transport, true, Some(status.as_u16()))
					.await?
				{
					continue;
				}
				return Err(anyhow::anyhow!("HTTP error {}: {}", status, error_body));
			}

			return response
				.json()
				.await
				.map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e));
		}
	}
}
