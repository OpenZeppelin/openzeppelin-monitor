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
use reqwest::{Client, ClientBuilder};
use reqwest_retry::{policies::ExponentialBackoff, Jitter};
use serde::Serialize;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use url::Url;

use crate::{
	models::Network,
	services::blockchain::transports::{BlockchainTransport, EndpointManager, RotatingTransport},
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
	pub client: Arc<RwLock<Client>>,
	/// Manages RPC endpoint rotation and request handling for high availability
	endpoint_manager: EndpointManager,
	/// The retry policy for failed requests
	retry_policy: ExponentialBackoff,
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
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let mut rpc_urls: Vec<_> = network
			.rpc_urls
			.iter()
			.filter(|rpc_url| rpc_url.type_ == "rpc" && rpc_url.weight > 0)
			.collect();

		rpc_urls.sort_by(|a, b| b.weight.cmp(&a.weight));

		// Default retry policy
		let retry_policy = ExponentialBackoff::builder()
			.base(2)
			.retry_bounds(Duration::from_millis(100), Duration::from_secs(4))
			.jitter(Jitter::None)
			.build_with_max_retries(2);

		let client = ClientBuilder::new()
			.timeout(Duration::from_secs(30))
			.build()
			.context("Failed to create HTTP client")?;

		for rpc_url in rpc_urls.iter() {
			let url = match Url::parse(&rpc_url.url) {
				Ok(url) => url,
				Err(_) => continue,
			};

			// Test connection with a basic request
			let test_request = json!({
				"jsonrpc": "2.0",
				"id": 1,
				"method": "net_version",
				"params": []
			});

			let request = client.post(url.clone()).json(&test_request);
			// Attempt to connect to the endpoint
			match request.send().await {
				Ok(response) => {
					// Check if the response indicates an error status (4xx or 5xx)
					if response.error_for_status().is_err() {
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
						client: Arc::new(RwLock::new(client)),
						endpoint_manager: EndpointManager::new(rpc_url.url.as_ref(), fallback_urls),
						retry_policy,
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

	/// Retrieves the current retry policy configuration
	///
	/// # Returns
	/// * `Result<ExponentialBackoff, anyhow::Error>` - Current retry policy
	fn get_retry_policy(&self) -> Result<ExponentialBackoff, anyhow::Error> {
		Ok(self.retry_policy)
	}

	/// Updates the retry policy configuration
	///
	/// # Arguments
	/// * `retry_policy` - New retry policy to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	fn set_retry_policy(&mut self, retry_policy: ExponentialBackoff) -> Result<(), anyhow::Error> {
		self.retry_policy = retry_policy;
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

		let test_request = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "net_version",
			"params": []
		});

		let client = self.client.read().await;
		let request = client.post(url.clone()).json(&test_request);

		match request.send().await {
			Ok(_) => Ok(()),
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
