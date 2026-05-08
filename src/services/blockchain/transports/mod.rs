//! Network transport implementations for blockchain clients.
//!
//! Provides concrete implementations for different blockchain network protocols:
//!
//! - Generic HTTP transport for all chains

mod evm {
	pub mod http;
}
mod stellar {
	pub mod http;
}
mod midnight {
	pub mod ws;
}
mod solana {
	pub mod http;
}

mod http {
	pub mod endpoint_manager;
	pub mod transport;
}

mod ws {
	pub mod config;
	pub mod connection;
	pub mod endpoint_manager;
	pub mod transport;
}

mod error;

pub use http::{
	endpoint_manager::EndpointManager as HttpEndpointManager, transport::HttpTransportClient,
};
pub use ws::{
	config::WsConfig, endpoint_manager::EndpointManager as WsEndpointManager,
	transport::WsTransportClient,
};

pub use error::TransportError;
pub use evm::http::EVMTransportClient;
pub use midnight::ws::MidnightTransportClient as MidnightWsTransportClient;
pub use solana::http::{
	Commitment as SolanaCommitment, GetBlockConfig as SolanaGetBlockConfig,
	GetTransactionConfig as SolanaGetTransactionConfig, SolanaTransportClient,
};
pub use stellar::http::StellarTransportClient;

use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::{
	default_on_request_failure, default_on_request_success, Retryable, RetryableStrategy,
};
use serde::Serialize;
use serde_json::{json, Value};

/// HTTP status codes that trigger RPC endpoint rotation
/// - 400: Bad Request - some providers return 400 for transient/operational issues
///   (e.g. malformed proxy responses) rather than 5xx, so rotation is worth attempting
/// - 408: Request Timeout - the endpoint timed out processing the request
/// - 410: Gone - the endpoint is no longer available
/// - 429: Too Many Requests - indicates rate limiting from the current endpoint
/// - 500: Internal Server Error - the endpoint encountered an unexpected error
/// - 502: Bad Gateway - the upstream RPC node behind a proxy is down
/// - 503: Service Unavailable - the endpoint is overloaded or in maintenance
/// - 504: Gateway Timeout - the upstream RPC node is unresponsive
pub const ROTATE_ON_ERROR_CODES: [u16; 8] = [400, 408, 410, 429, 500, 502, 503, 504];

/// Base trait for all blockchain transport clients
#[async_trait::async_trait]
pub trait BlockchainTransport: Send + Sync {
	/// Get the current URL being used by the transport
	async fn get_current_url(&self) -> String;

	/// Send a raw request to the blockchain
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize;

	/// Customizes the request for specific blockchain requirements
	async fn customize_request<P>(&self, method: &str, params: Option<P>) -> Value
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		// Default implementation for JSON-RPC
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": method,
			"params": params.map(|p| p.into())
		})
	}

	/// Update endpoint manager with a new client
	fn update_endpoint_manager_client(
		&mut self,
		client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error>;
}

/// Extension trait for transports that support URL rotation
#[async_trait::async_trait]
pub trait RotatingTransport: BlockchainTransport {
	/// Attempts to establish a connection with a new URL
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error>;

	/// Updates the client with a new URL
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error>;
}

/// A default retry strategy that retries on requests based on the status code
/// This can be used to customise the retry strategy
pub struct TransientErrorRetryStrategy;
impl RetryableStrategy for TransientErrorRetryStrategy {
	fn handle(
		&self,
		res: &Result<reqwest::Response, reqwest_middleware::Error>,
	) -> Option<Retryable> {
		match res {
			Ok(success) => default_on_request_success(success),
			Err(error) => default_on_request_failure(error),
		}
	}
}
