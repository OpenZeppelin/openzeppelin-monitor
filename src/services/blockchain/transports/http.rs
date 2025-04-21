//! HTTP transport implementation for blockchain interactions.
//!
//! This module provides a generic HTTP client implementation for interacting with blockchain nodes
//! via JSON-RPC, supporting:
//! - Multiple RPC endpoints with automatic failover
//! - Configurable retry policies
//! - Authentication via bearer tokens
//! - Connection health checks
//! - Endpoint rotation for high availability

use anyhow::Context;
use async_trait::async_trait;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, Jitter, RetryTransientMiddleware};
use serde::Serialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use url::Url;

use crate::{
	models::Network,
	services::blockchain::transports::{
		BlockchainTransport, EndpointManager, RotatingTransport, TransientErrorRetryStrategy,
	},
};

/// Basic HTTP transport client for blockchain interactions
///
/// This client provides a foundation for making JSON-RPC requests to blockchain nodes
/// with built-in support for:
/// - Connection pooling and reuse
/// - Automatic endpoint rotation on failure
/// - Configurable retry policies
///
/// The client is thread-safe and can be shared across multiple tasks.
#[derive(Clone, Debug)]
pub struct HttpTransportClient {
	/// HTTP client for making requests, wrapped in Arc for thread-safety
	pub client: Arc<Client>,
	/// Manages RPC endpoint rotation and request handling for high availability
	endpoint_manager: EndpointManager,
	/// The stringified JSON RPC payload to use for testing the connection
	test_connection_payload: Option<String>,
}

impl HttpTransportClient {
	/// Creates a new HTTP transport client with automatic endpoint management
	///
	/// This constructor attempts to connect to available endpoints in order of their
	/// weight until a successful connection is established. It configures default
	/// timeout and retry policies suitable for blockchain interactions.
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs, weights, and other details
	/// * `test_connection_payload` - Optional JSON RPC payload to test the connection (default is net_version)
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(
		network: &Network,
		test_connection_payload: Option<String>,
	) -> Result<Self, anyhow::Error> {
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		// Default retry policy
		let retry_policy = ExponentialBackoff::builder()
			.base(2)
			.retry_bounds(Duration::from_millis(250), Duration::from_secs(10))
			.jitter(Jitter::Full)
			.build_with_max_retries(3);

		let http_client = reqwest::ClientBuilder::new()
			.pool_idle_timeout(Duration::from_secs(90))
			.pool_max_idle_per_host(32)
			.timeout(Duration::from_secs(30))
			.connect_timeout(Duration::from_secs(20))
			.build()
			.context("Failed to create HTTP client")?;

		// Clone it before using it to create the middleware client
		let cloned_http_client = http_client.clone();

		let client = ClientBuilder::new(cloned_http_client)
			.with(RetryTransientMiddleware::new_with_policy_and_strategy(
				retry_policy,
				TransientErrorRetryStrategy,
			))
			.build();

		for rpc_url in rpc_urls.iter() {
			let url = match Url::parse(&rpc_url.url) {
				Ok(url) => url,
				Err(_) => continue,
			};

			let test_request = if let Some(test_payload) = &test_connection_payload {
				serde_json::from_str(test_payload)
					.context("Failed to parse test payload as JSON")?
			} else {
				json!({
					"jsonrpc": "2.0",
					"id": 1,
					"method": "net_version",
					"params": []
				})
			};

			let request = http_client.post(url.clone()).json(&test_request);
			// Attempt to connect to the endpoint
			match request.send().await {
				Ok(response) => {
					// Check if the response indicates an error status (4xx or 5xx)
					if !response.status().is_success() {
						// Skip this URL if we got an error status
						continue;
					}

					// Create list of fallback URLs (all URLs except the current one)
					let fallback_urls: Vec<String> = rpc_urls
						.iter()
						.filter(|url| url.url != rpc_url.url)
						.map(|url| url.url.clone())
						.collect();

					// Successfully connected - create and return the client
					return Ok(Self {
						client: Arc::new(http_client),
						endpoint_manager: EndpointManager::new(
							client,
							rpc_url.url.as_ref(),
							fallback_urls,
						),
						test_connection_payload,
					});
				}
				Err(_) => {
					// Connection failed - try next URL
					continue;
				}
			}
		}

		Err(anyhow::anyhow!("All RPC URLs failed to connect"))
	}
}

#[async_trait]
impl BlockchainTransport for HttpTransportClient {
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

	/// Sends a JSON-RPC request to the blockchain node
	///
	/// This method handles the formatting of the JSON-RPC request, including:
	/// - Adding required JSON-RPC 2.0 fields
	/// - Generating unique request IDs
	/// - Converting parameters to the correct format
	/// - Handling authentication
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
		let response = self
			.endpoint_manager
			.send_raw_request(self, method, params)
			.await?;

		Ok(response)
	}

	/// Updates the retry policy configuration
	///
	/// # Arguments
	/// * `retry_policy` - New retry policy to use for subsequent requests
	/// * `retry_strategy` - Optional retry strategy to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	fn set_retry_policy(
		&mut self,
		retry_policy: ExponentialBackoff,
		retry_strategy: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		self.endpoint_manager.set_retry_policy(
			retry_policy,
			retry_strategy.unwrap_or(TransientErrorRetryStrategy),
		);
		Ok(())
	}

	/// Update endpoint manager with a new client
	///
	/// # Arguments
	/// * `client` - The new client to use for the endpoint manager
	fn update_endpoint_manager_client(
		&mut self,
		client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		self.endpoint_manager.update_client(client);
		Ok(())
	}
}

#[async_trait]
impl RotatingTransport for HttpTransportClient {
	/// Tests connectivity to a specific RPC endpoint
	///
	/// Performs a basic JSON-RPC request to verify the endpoint is responsive
	/// and correctly handling requests.
	///
	/// # Arguments
	/// * `url` - The URL to test
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or detailed error message
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		let url = Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid URL: {}", url))?;

		let test_request = if let Some(test_payload) = &self.test_connection_payload {
			serde_json::from_str(test_payload).context("Failed to parse test payload as JSON")?
		} else {
			json!({
				"jsonrpc": "2.0",
				"id": 1,
				"method": "net_version",
				"params": []
			})
		};

		let request = self.client.post(url.clone()).json(&test_request);

		match request.send().await {
			Ok(response) => {
				let status = response.status();
				if !status.is_success() {
					Err(anyhow::anyhow!(
						"Failed to connect to {}: {}",
						url,
						status.as_u16()
					))
				} else {
					Ok(())
				}
			}
			Err(e) => Err(anyhow::anyhow!("Failed to connect to {}: {}", url, e)),
		}
	}

	/// Updates the active endpoint URL
	///
	/// This method is called when rotating to a new endpoint, typically
	/// after a failure of the current endpoint.
	///
	/// # Arguments
	/// * `url` - The new URL to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		let parsed_url = Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid URL: {}", url))?;
		// Normalize the URL by trimming trailing slash if present
		let normalized_url = parsed_url.as_str().trim_end_matches('/');

		// For HTTP client, we don't need to update the client itself
		// We just need to update the endpoint manager's active URL
		let mut active_url = self.endpoint_manager.active_url.write().await;
		*active_url = normalized_url.to_string();
		Ok(())
	}
}
