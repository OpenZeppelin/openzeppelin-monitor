//! Manages the rotation of blockchain RPC endpoints
//!
//! Provides methods for rotating between multiple URLs and sending requests to the active endpoint
//! with automatic fallback to other URLs on failure.
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::services::blockchain::transports::{
	RotatingTransport, TransportError, ROTATE_ON_ERROR_CODES,
};

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

/// Represents the outcome of a `EndpointManager::attempt_request_on_url` method call
/// Used within the `EndpointManager::send_raw_request` method to handle different paths of request execution
/// and response handling.
#[derive(Debug)]
enum SingleRequestAttemptOutcome {
	/// Successfully got a response (status might still be error)
	Success(reqwest::Response),
	/// Error during send (e.g., connection, timeout)
	NetworkError(reqwest_middleware::Error),
	/// Error serializing the request body
	SerializationError(TransportError),
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

	/// Rotates to the next available URL
	///
	/// # Arguments
	/// * `transport` - The transport client implementing the RotatingTransport trait
	///
	/// # Returns
	/// * `Result<(), TransportError>` - The result of the rotation operation
	pub async fn rotate_url<T: RotatingTransport>(
		&self,
		transport: &T,
	) -> Result<(), TransportError> {
		// Acquire rotation lock first
		let _guard = self.rotation_lock.lock().await;

		let current_active = self.active_url.read().await.clone();

		// Get a different URL from fallbacks
		let new_url = {
			let mut fallback_urls = self.fallback_urls.write().await;
			if fallback_urls.is_empty() {
				let message = format!(
					"No fallback URLs available for rotation. Current active URL: {}",
					current_active
				);
				return Err(TransportError::url_rotation(message, None, None));
			}

			// Find first URL that's different from current
			let idx = fallback_urls.iter().position(|url| url != &current_active);

			match idx {
				Some(pos) => fallback_urls.remove(pos),
				None => {
					let message = format!(
						"All fallback URLs are the same as the current active URL: {}",
						current_active
					);
					return Err(TransportError::url_rotation(message, None, None));
				}
			}
		};

		if transport.try_connect(&new_url).await.is_ok() {
			transport.update_client(&new_url).await.map_err(|e| {
				TransportError::url_rotation(
					"Failed to update transport client with new URL".to_string(),
					Some(e.into()),
					None,
				)
			})?;

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
			let message = format!(
				"Failed to connect to new URL: {}. Retaining it in fallback list.",
				&new_url
			);
			// Re-acquire lock to push back the failed URL
			let mut fallback_urls = self.fallback_urls.write().await;
			fallback_urls.push(new_url);
			Err(TransportError::url_rotation(message, None, None))
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
	/// * `TransportError` - Rotation was attempted but failed
	async fn should_attempt_rotation<T: RotatingTransport>(
		&self,
		transport: &T,
		should_check_status: bool,
		status: Option<u16>,
	) -> Result<bool, TransportError> {
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
				Err(e) => {
					let message =
						format!("Rotation failed for URL: {}", self.active_url.read().await);
					Err(TransportError::url_rotation(message, Some(e.into()), None))
				}
			}
		} else {
			Ok(false) // No rotation needed
		}
	}

	/// Attempts to send a request to the specified URL
	/// # Arguments
	/// * `url` - The URL to send the request to
	/// * `transport` - The transport client implementing the RotatingTransport trait
	/// * `method` - The HTTP method to use for the request (e.g., "POST")
	/// * `params` - Optional parameters for the request, serialized to JSON
	///
	/// # Returns
	/// * `SingleRequestAttemptOutcome` - The outcome of the request attempt
	async fn attempt_request_on_url<P>(
		&self,
		url: &str,
		transport: &impl RotatingTransport,
		method: &str,
		params: Option<P>,
	) -> SingleRequestAttemptOutcome
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		// Create the request body using the transport's customization method
		let request_body = transport.customize_request(method, params).await;

		// Serialize the request body to JSON
		let request_body_str = match serde_json::to_string(&request_body) {
			Ok(body) => body,
			Err(e) => {
				tracing::error!("Failed to serialize request body: {}", e);
				return SingleRequestAttemptOutcome::SerializationError(
					TransportError::request_serialization(
						"Failed to serialize request JSON",
						Some(Box::new(e)),
						None,
					),
				);
			}
		};

		// Send the request to the specified URL
		let response_result = self
			.client
			.post(url)
			.header("Content-Type", "application/json")
			.body(request_body_str)
			.send()
			.await;

		// Handle the response
		match response_result {
			Ok(response) => SingleRequestAttemptOutcome::Success(response),
			Err(network_error) => {
				tracing::warn!("Network error while sending request: {}", network_error);
				SingleRequestAttemptOutcome::NetworkError(network_error)
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
	/// * `Result<Value, TransportError>` - The JSON response from the RPC endpoint or an error
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
	) -> Result<Value, TransportError> {
		loop {
			let current_url = self.active_url.read().await.clone();

			// Attempt to send the request to the current active URL
			let attempt_result = self
				.attempt_request_on_url(&current_url, transport, method, params.clone())
				.await;

			match attempt_result {
				// Handle successful response
				SingleRequestAttemptOutcome::Success(response) => {
					let status = response.status();
					if status.is_success() {
						// Successful response, parse JSON
						return response.json().await.map_err(|e| {
							TransportError::response_parse(
								"Failed to parse JSON response".to_string(),
								Some(Box::new(e)),
								None,
							)
						});
					} else {
						// HTTP error
						let error_body = response.text().await.unwrap_or_default();
						tracing::warn!(
							"Request to {} failed with status {}: {}",
							current_url,
							status,
							error_body
						);

						// Check if we should rotate based on status code
						let should_rotate = self
							.should_attempt_rotation(transport, true, Some(status.as_u16()))
							.await?;

						if should_rotate {
							tracing::debug!(
								"Rotation successful after HTTP error status {}, retrying request.",
								status
							);
							continue;
						} else {
							tracing::warn!("No rotation possible or needed after HTTP error status {}. Failing.", status);
							return Err(TransportError::http(
								status,
								current_url.clone(),
								error_body,
								None,
								None,
							));
						}
					}
				}
				// Handle network error, try rotation
				SingleRequestAttemptOutcome::NetworkError(network_error) => {
					tracing::warn!(
						"Network error while sending request to {}: {}",
						current_url,
						network_error
					);

					// Check if we should rotate based on network error
					let should_rotate =
						self.should_attempt_rotation(transport, false, None).await?;

					if should_rotate {
						tracing::debug!(
							"Rotation successful after network error, retrying request."
						);
						continue;
					} else {
						tracing::warn!(
							"No rotation possible or needed after network error. Failing."
						);
						return Err(TransportError::network(
							format!(
								"Failed to send request to {} due to network error",
								current_url
							),
							Some(network_error.into()),
							None,
						));
					}
				}
				// Non-retryable serialization error
				SingleRequestAttemptOutcome::SerializationError(serialization_error) => {
					return Err(serialization_error);
				}
			}
		}
	}
}
