//! Manages the rotation of blockchain HTTP RPC endpoints
//!
//! Provides methods for rotating between multiple URLs and sending requests to the active endpoint
//! with automatic fallback to other URLs on failure.
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use url::Url;

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
/// * `network_slug` - The network identifier for metrics labeling
/// * `non_rotating_jsonrpc_codes` - JSON-RPC error codes that should not trigger endpoint
///   rotation (e.g. Solana skipped-slot codes that represent legitimate chain state).
#[derive(Clone, Debug)]
pub struct EndpointManager {
	pub active_url: Arc<RwLock<String>>,
	pub fallback_urls: Arc<RwLock<Vec<String>>>,
	client: ClientWithMiddleware,
	rotation_lock: Arc<tokio::sync::Mutex<()>>,
	network_slug: String,
	non_rotating_jsonrpc_codes: &'static [i64],
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
	/// * `network_slug` - The network identifier for metrics labeling
	/// * `non_rotating_jsonrpc_codes` - JSON-RPC error codes for which the manager should
	///   pass the response through unchanged instead of rotating to a fallback. Use `&[]`
	///   if every JSON-RPC error should rotate.
	///
	/// # Returns
	pub fn new(
		client: ClientWithMiddleware,
		active_url: &str,
		fallback_urls: Vec<String>,
		network_slug: String,
		non_rotating_jsonrpc_codes: &'static [i64],
	) -> Self {
		Self {
			active_url: Arc::new(RwLock::new(active_url.to_string())),
			fallback_urls: Arc::new(RwLock::new(fallback_urls)),
			rotation_lock: Arc::new(tokio::sync::Mutex::new(())),
			client,
			network_slug,
			non_rotating_jsonrpc_codes,
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
	/// * `Result<String, TransportError>` - The result of the rotation attempt, containing the new active URL or an error
	pub async fn try_rotate_url<T: RotatingTransport>(
		&self,
		transport: &T,
	) -> Result<String, TransportError> {
		// Acquire the rotation lock to prevent concurrent rotations
		let _guard = self.rotation_lock.lock().await;
		let initial_active_url = self.active_url.read().await.clone();
		let current_fallbacks_snapshot = self.fallback_urls.read().await.clone();

		tracing::debug!(
			"Trying to rotate URL: Current Active: '{}', Fallbacks: {:?}",
			initial_active_url,
			current_fallbacks_snapshot,
		);

		// --- Select a new URL ---
		let new_url = match current_fallbacks_snapshot
			.iter()
			.find(|&url| *url != initial_active_url)
		{
			Some(url) => url.clone(),
			None => {
				let msg = format!(
					"No fallback URLs available. Current active: '{}', Fallbacks checked: {:?}",
					initial_active_url, current_fallbacks_snapshot
				);
				return Err(TransportError::url_rotation(msg, None, None));
			}
		};

		// --- Attempt to connect and update the transport client ---
		tracing::debug!(
			"Attempting try_connect to new_url during rotation: '{}'",
			new_url
		);

		transport
			.try_connect(&new_url)
			.await
			.map_err(|connect_err| {
				TransportError::url_rotation(
					format!("Failed to connect to new URL '{}'", new_url),
					Some(connect_err.into()),
					None,
				)
			})?;

		tracing::debug!(
			"Attempting update_client with new_url during rotation: '{}'",
			new_url
		);

		transport
			.update_client(&new_url)
			.await
			.map_err(|update_err| {
				TransportError::url_rotation(
					format!(
						"Failed to update transport client with new URL '{}'",
						new_url
					),
					Some(update_err.into()),
					None,
				)
			})?;

		// --- All checks passed, update shared state ---
		{
			let mut active_url_guard = self.active_url.write().await;
			let mut fallback_urls_guard = self.fallback_urls.write().await;

			// Construct the new fallbacks list:
			// old fallbacks, MINUS the new_url_candidate, PLUS the initial_active_url.
			let mut next_fallback_urls: Vec<String> = Vec::with_capacity(fallback_urls_guard.len());
			for url in fallback_urls_guard.iter() {
				if *url != new_url {
					next_fallback_urls.push(url.clone());
				}
			}
			next_fallback_urls.push(initial_active_url.clone()); // Add the previously active URL

			tracing::debug!(
				"Successful URL rotation - from: '{}', to: '{}'. New Fallbacks: {:?}",
				initial_active_url,
				new_url,
				next_fallback_urls
			);

			*fallback_urls_guard = next_fallback_urls;
			*active_url_guard = new_url.clone();
		}
		Ok(new_url)
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
	async fn try_request_on_url<P>(
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
		// Cap rotations per request: count distinct configured endpoints (active + unique
		// fallbacks) so we can stop once each has been tried once.
		let total_unique_endpoints = {
			let mut endpoints: HashSet<String> = HashSet::new();
			endpoints.insert(self.active_url.read().await.clone());
			for url in self.fallback_urls.read().await.iter() {
				endpoints.insert(url.clone());
			}
			endpoints.len()
		};
		let mut tried_urls: HashSet<String> = HashSet::new();

		loop {
			let attempt_start = Instant::now();

			// Record each attempt so error rates stay consistent
			crate::utils::metrics::record_rpc_request(&self.network_slug, method);

			let current_url_snapshot = self.active_url.read().await.clone();
			let current_host_snapshot = Url::parse(&current_url_snapshot)
				.ok()
				.and_then(|u| u.host_str().map(|s| s.to_string()))
				.unwrap_or_else(|| "unknown-host".into());
			tried_urls.insert(current_url_snapshot.clone());

			tracing::debug!(
				"Attempting request on active URL: '{}'",
				current_url_snapshot
			);

			// Attempt to send the request to the current active URL
			let attempt_result = self
				.try_request_on_url(&current_url_snapshot, transport, method, params.clone())
				.await;

			match attempt_result {
				// Handle successful response
				SingleRequestAttemptOutcome::Success(response) => {
					let status = response.status();
					if status.is_success() {
						// Record successful request duration
						let duration = attempt_start.elapsed().as_secs_f64();
						crate::utils::metrics::observe_rpc_duration(&self.network_slug, duration);

						// Parse JSON body
						let value: Value = match response.json().await {
							Ok(v) => v,
							Err(e) => {
								return Err(TransportError::response_parse(
									"Failed to parse JSON response".to_string(),
									Some(Box::new(e)),
									None,
								));
							}
						};

						// Inspect the JSON-RPC envelope: a well-formed response has either a
						// `result` field (success, possibly null) or an `error` field.
						match (value.get("result"), value.get("error")) {
							(Some(result), _) => {
								if result.is_null() {
									crate::utils::metrics::record_null_result(
										&self.network_slug,
										method,
									);
								}
								return Ok(value);
							}
							(None, Some(error_obj)) => {
								let code =
									error_obj.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
								let message = error_obj
									.get("message")
									.and_then(|m| m.as_str())
									.unwrap_or("")
									.to_string();

								if self.non_rotating_jsonrpc_codes.contains(&code) {
									// Legitimate chain-state response — record for visibility
									// but pass it through so the per-client error handler runs.
									// Tracked under a dedicated counter (not the errors counter)
									// so it doesn't inflate error-rate alerts.
									crate::utils::metrics::record_jsonrpc_passthrough(
										&self.network_slug,
										&code.to_string(),
									);
									return Ok(value);
								}

								crate::utils::metrics::record_rpc_error(
									&self.network_slug,
									&code.to_string(),
									"jsonrpc",
								);

								tracing::warn!(
									"JSON-RPC error from {}: code {} - {}",
									current_host_snapshot,
									code,
									message,
								);

								// Stop once every distinct endpoint has been tried; otherwise
								// healthy-but-erroring endpoints would cycle forever.
								if tried_urls.len() >= total_unique_endpoints {
									return Err(TransportError::rpc_error(
										code,
										message,
										current_url_snapshot.clone(),
										None,
										None,
									));
								}

								crate::utils::metrics::record_endpoint_rotation(
									&self.network_slug,
									"jsonrpc_error",
								);

								match self.try_rotate_url(transport).await {
									Ok(_new_url) => continue, // Retry on the new active URL
									Err(rotation_error) => {
										return Err(TransportError::rpc_error(
											code,
											message,
											current_url_snapshot.clone(),
											Some(Box::new(rotation_error)),
											None,
										));
									}
								}
							}
							(None, None) => {
								// Malformed envelope — no `result` and no `error`. Treat as a
								// rotatable failure, since the upstream is misbehaving.
								// `status_code="0"` keeps the slot numeric for PromQL filters
								// (e.g. `status_code=~"5.."`); the malformed case is carried on
								// `error_type` instead.
								crate::utils::metrics::record_rpc_error(
									&self.network_slug,
									"0",
									"malformed_jsonrpc",
								);

								tracing::warn!(
									"Malformed JSON-RPC envelope from {} (neither 'result' nor 'error' field)",
									current_host_snapshot,
								);

								if tried_urls.len() >= total_unique_endpoints {
									return Err(TransportError::rpc_error(
										0,
										"Malformed JSON-RPC envelope",
										current_url_snapshot.clone(),
										None,
										None,
									));
								}

								crate::utils::metrics::record_endpoint_rotation(
									&self.network_slug,
									"jsonrpc_error",
								);

								match self.try_rotate_url(transport).await {
									Ok(_new_url) => continue,
									Err(rotation_error) => {
										return Err(TransportError::rpc_error(
											0,
											"Malformed JSON-RPC envelope",
											current_url_snapshot.clone(),
											Some(Box::new(rotation_error)),
											None,
										));
									}
								}
							}
						}
					} else {
						// HTTP error
						let error_body = response.text().await.unwrap_or_default();
						let status_code = status.as_u16();

						// Record HTTP error metric
						crate::utils::metrics::record_rpc_error(
							&self.network_slug,
							&status_code.to_string(),
							"http",
						);

						tracing::warn!(
							"Request to {} failed with status {}: {}",
							current_host_snapshot,
							status,
							error_body
						);

						// Check if we should rotate based on status code
						if ROTATE_ON_ERROR_CODES.contains(&status_code) {
							// Record rate limit metric only for 429
							if status_code == 429 {
								let endpoint_label = Url::parse(&current_url_snapshot)
									.ok()
									.and_then(|u| u.host_str().map(|h| h.to_string()))
									.unwrap_or_else(|| "unknown".to_string());

								crate::utils::metrics::record_rate_limit(
									&self.network_slug,
									&endpoint_label,
								);
							}

							tracing::debug!(
								"send_raw_request: HTTP status {} on '{}' triggers URL rotation attempt",
								status,
								current_url_snapshot
							);

							let rotation_reason = if status_code == 429 {
								"rate_limit"
							} else {
								"http_error"
							};

							crate::utils::metrics::record_endpoint_rotation(
								&self.network_slug,
								rotation_reason,
							);

							match self.try_rotate_url(transport).await {
								Ok(_new_url) => {
									continue; // Retry on the new active URL
								}
								Err(rotation_error) => {
									// Return the original HTTP error with rotation error context
									return Err(TransportError::http(
										status,
										current_url_snapshot.clone(),
										error_body,
										Some(Box::new(rotation_error)),
										None,
									));
								}
							}
						} else {
							// HTTP error that doesn't trigger rotation
							tracing::warn!(
								"HTTP error status {} on {} does not trigger rotation. Failing.",
								status,
								current_host_snapshot
							);
							return Err(TransportError::http(
								status,
								current_url_snapshot,
								error_body,
								None,
								None,
							));
						}
					}
				}
				// Handle network error, try rotation
				SingleRequestAttemptOutcome::NetworkError(network_error) => {
					// Record network error metric
					crate::utils::metrics::record_rpc_error(&self.network_slug, "0", "network");

					tracing::warn!(
						"Network error for {}: {}",
						current_host_snapshot,
						network_error,
					);

					// Record endpoint rotation due to network error
					crate::utils::metrics::record_endpoint_rotation(
						&self.network_slug,
						"network_error",
					);

					// Always attempt rotation on network errors
					match self.try_rotate_url(transport).await {
						Ok(new_url) => {
							tracing::debug!(
								"Rotation successful after network error, retrying request on new URL: '{}'",
								new_url
							);
							continue; // Retry on the new active URL
						}
						Err(rotation_error) => {
							// Return network error with rotation error context
							return Err(TransportError::network(
								network_error.to_string(),
								Some(Box::new(rotation_error)),
								None,
							));
						}
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
