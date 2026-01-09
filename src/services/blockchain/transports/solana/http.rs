//! Solana transport implementation for blockchain interactions.
//!
//! This module provides a client implementation for interacting with Solana nodes
//! by wrapping the HttpTransportClient. This allows for consistent behavior with other
//! transport implementations while providing specific Solana-focused functionality.

use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
	models::Network,
	services::blockchain::transports::{
		BlockchainTransport, HttpTransportClient, RotatingTransport, TransportError,
	},
};

/// Solana RPC method constants
pub mod rpc_methods {
	/// Get the current slot height
	pub const GET_SLOT: &str = "getSlot";
	/// Get block data for a specific slot
	pub const GET_BLOCK: &str = "getBlock";
	/// Get multiple blocks
	pub const GET_BLOCKS: &str = "getBlocks";
	/// Get transaction details by signature
	pub const GET_TRANSACTION: &str = "getTransaction";
	/// Get the health status of the node
	pub const GET_HEALTH: &str = "getHealth";
	/// Get the version of the node
	pub const GET_VERSION: &str = "getVersion";
	/// Get account info
	pub const GET_ACCOUNT_INFO: &str = "getAccountInfo";
	/// Get multiple accounts
	#[allow(dead_code)]
	pub const GET_MULTIPLE_ACCOUNTS: &str = "getMultipleAccounts";
	/// Get program accounts
	pub const GET_PROGRAM_ACCOUNTS: &str = "getProgramAccounts";
	/// Get block height
	pub const GET_BLOCK_HEIGHT: &str = "getBlockHeight";
	/// Get block time
	pub const GET_BLOCK_TIME: &str = "getBlockTime";
	/// Get first available block
	pub const GET_FIRST_AVAILABLE_BLOCK: &str = "getFirstAvailableBlock";
	/// Get slot leader
	#[allow(dead_code)]
	pub const GET_SLOT_LEADER: &str = "getSlotLeader";
	/// Get minimum ledger slot
	pub const MINIMUM_LEDGER_SLOT: &str = "minimumLedgerSlot";
}

/// Configuration options for getBlock RPC calls
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockConfig {
	/// Encoding format for account data
	#[serde(skip_serializing_if = "Option::is_none")]
	pub encoding: Option<String>,
	/// Transaction details level
	#[serde(skip_serializing_if = "Option::is_none")]
	pub transaction_details: Option<String>,
	/// Whether to include rewards
	#[serde(skip_serializing_if = "Option::is_none")]
	pub rewards: Option<bool>,
	/// Commitment level
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commitment: Option<String>,
	/// Max supported transaction version
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_supported_transaction_version: Option<u8>,
}

impl GetBlockConfig {
	/// Creates a config for fetching full transaction data with JSON encoding
	pub fn full() -> Self {
		Self {
			encoding: Some("json".to_string()),
			transaction_details: Some("full".to_string()),
			rewards: Some(true),
			commitment: Some("finalized".to_string()),
			max_supported_transaction_version: Some(0),
		}
	}

	/// Creates a config for fetching block with signatures only (lighter)
	pub fn signatures_only() -> Self {
		Self {
			encoding: Some("json".to_string()),
			transaction_details: Some("signatures".to_string()),
			rewards: Some(false),
			commitment: Some("finalized".to_string()),
			max_supported_transaction_version: Some(0),
		}
	}

	/// Creates a config for fetching block metadata only (no transactions)
	pub fn none() -> Self {
		Self {
			encoding: Some("json".to_string()),
			transaction_details: Some("none".to_string()),
			rewards: Some(false),
			commitment: Some("finalized".to_string()),
			max_supported_transaction_version: Some(0),
		}
	}
}

/// Configuration options for getTransaction RPC calls
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetTransactionConfig {
	/// Encoding format
	#[serde(skip_serializing_if = "Option::is_none")]
	pub encoding: Option<String>,
	/// Commitment level
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commitment: Option<String>,
	/// Max supported transaction version
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_supported_transaction_version: Option<u8>,
}

impl GetTransactionConfig {
	/// Creates a default config for fetching transaction with JSON encoding
	pub fn json() -> Self {
		Self {
			encoding: Some("json".to_string()),
			commitment: Some("finalized".to_string()),
			max_supported_transaction_version: Some(0),
		}
	}

	/// Creates a config for fetching transaction with JSON parsed encoding
	pub fn json_parsed() -> Self {
		Self {
			encoding: Some("jsonParsed".to_string()),
			commitment: Some("finalized".to_string()),
			max_supported_transaction_version: Some(0),
		}
	}
}

/// Commitment levels for Solana RPC requests
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Commitment {
	/// The node will query the most recent block confirmed by the cluster
	#[default]
	Finalized,
	/// The node will query the most recent block voted on by supermajority
	Confirmed,
	/// The node will query its most recent block (may not be finalized)
	Processed,
}

impl Commitment {
	/// Returns the commitment as a string
	pub fn as_str(&self) -> &'static str {
		match self {
			Commitment::Finalized => "finalized",
			Commitment::Confirmed => "confirmed",
			Commitment::Processed => "processed",
		}
	}
}

/// A client for interacting with Solana blockchain nodes
///
/// This implementation wraps the HttpTransportClient to provide consistent
/// behavior with other transport implementations while offering Solana-specific
/// functionality. It handles connection management, request retries, and
/// endpoint rotation for Solana networks.
#[derive(Clone, Debug)]
pub struct SolanaTransportClient {
	/// The underlying HTTP transport client that handles actual RPC communications
	http_client: HttpTransportClient,
}

impl SolanaTransportClient {
	/// Creates a new Solana transport client by initializing an HTTP transport client
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs and other network details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		// Use getHealth as the test connection method - it's lightweight and indicates node health
		let test_connection_payload =
			Some(r#"{"id":1,"jsonrpc":"2.0","method":"getHealth"}"#.to_string());
		let http_client = HttpTransportClient::new(network, test_connection_payload).await?;
		Ok(Self { http_client })
	}

	/// Creates a new Solana transport client with a custom test payload
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC URLs and other network details
	/// * `test_payload` - Optional custom test connection payload
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - A new client instance or connection error
	pub async fn with_test_payload(
		network: &Network,
		test_payload: Option<String>,
	) -> Result<Self, anyhow::Error> {
		let http_client = HttpTransportClient::new(network, test_payload).await?;
		Ok(Self { http_client })
	}

	/// Gets the underlying HTTP client for direct access if needed
	pub fn http_client(&self) -> &HttpTransportClient {
		&self.http_client
	}

	// ========== Convenience RPC Methods ==========

	/// Gets the current slot number
	///
	/// # Arguments
	/// * `commitment` - Optional commitment level (defaults to finalized)
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - The current slot number or error
	pub async fn get_slot(&self, commitment: Option<Commitment>) -> Result<Value, TransportError> {
		let params = commitment.map(|c| json!([{ "commitment": c.as_str() }]));
		self.http_client
			.send_raw_request(rpc_methods::GET_SLOT, params)
			.await
	}

	/// Gets block data for a specific slot
	///
	/// # Arguments
	/// * `slot` - The slot number to fetch
	/// * `config` - Optional configuration for the request
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Block data or error
	pub async fn get_block(
		&self,
		slot: u64,
		config: Option<GetBlockConfig>,
	) -> Result<Value, TransportError> {
		let config = config.unwrap_or_else(GetBlockConfig::full);
		let params = json!([slot, config]);
		self.http_client
			.send_raw_request(rpc_methods::GET_BLOCK, Some(params))
			.await
	}

	/// Gets a range of available block slots
	///
	/// # Arguments
	/// * `start_slot` - Start slot (inclusive)
	/// * `end_slot` - Optional end slot (inclusive). If None, returns blocks up to current
	/// * `commitment` - Optional commitment level
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Array of slot numbers or error
	pub async fn get_blocks(
		&self,
		start_slot: u64,
		end_slot: Option<u64>,
		commitment: Option<Commitment>,
	) -> Result<Value, TransportError> {
		let params = match (end_slot, commitment) {
			(Some(end), Some(c)) => json!([start_slot, end, { "commitment": c.as_str() }]),
			(Some(end), None) => json!([start_slot, end]),
			(None, Some(c)) => json!([start_slot, null, { "commitment": c.as_str() }]),
			(None, None) => json!([start_slot]),
		};
		self.http_client
			.send_raw_request(rpc_methods::GET_BLOCKS, Some(params))
			.await
	}

	/// Gets transaction details by signature
	///
	/// # Arguments
	/// * `signature` - The transaction signature (base58 encoded)
	/// * `config` - Optional configuration for the request
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Transaction data or error
	pub async fn get_transaction(
		&self,
		signature: &str,
		config: Option<GetTransactionConfig>,
	) -> Result<Value, TransportError> {
		let config = config.unwrap_or_else(GetTransactionConfig::json);
		let params = json!([signature, config]);
		self.http_client
			.send_raw_request(rpc_methods::GET_TRANSACTION, Some(params))
			.await
	}

	/// Gets account info for a given public key
	///
	/// # Arguments
	/// * `pubkey` - The account's public key (base58 encoded)
	/// * `commitment` - Optional commitment level
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Account info or error
	pub async fn get_account_info(
		&self,
		pubkey: &str,
		commitment: Option<Commitment>,
	) -> Result<Value, TransportError> {
		let config = json!({
			"encoding": "jsonParsed",
			"commitment": commitment.unwrap_or_default().as_str()
		});
		let params = json!([pubkey, config]);
		self.http_client
			.send_raw_request(rpc_methods::GET_ACCOUNT_INFO, Some(params))
			.await
	}

	/// Gets program accounts for a given program ID
	///
	/// # Arguments
	/// * `program_id` - The program's public key (base58 encoded)
	/// * `filters` - Optional filters to apply
	/// * `commitment` - Optional commitment level
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Array of program accounts or error
	pub async fn get_program_accounts(
		&self,
		program_id: &str,
		filters: Option<Vec<Value>>,
		commitment: Option<Commitment>,
	) -> Result<Value, TransportError> {
		let mut config = json!({
			"encoding": "jsonParsed",
			"commitment": commitment.unwrap_or_default().as_str()
		});

		if let Some(f) = filters {
			config["filters"] = json!(f);
		}

		let params = json!([program_id, config]);
		self.http_client
			.send_raw_request(rpc_methods::GET_PROGRAM_ACCOUNTS, Some(params))
			.await
	}

	/// Gets the current block height
	///
	/// # Arguments
	/// * `commitment` - Optional commitment level
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Current block height or error
	pub async fn get_block_height(
		&self,
		commitment: Option<Commitment>,
	) -> Result<Value, TransportError> {
		let params = commitment.map(|c| json!([{ "commitment": c.as_str() }]));
		self.http_client
			.send_raw_request(rpc_methods::GET_BLOCK_HEIGHT, params)
			.await
	}

	/// Gets the estimated production time of a block
	///
	/// # Arguments
	/// * `slot` - The slot number
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Unix timestamp or error
	pub async fn get_block_time(&self, slot: u64) -> Result<Value, TransportError> {
		let params = json!([slot]);
		self.http_client
			.send_raw_request(rpc_methods::GET_BLOCK_TIME, Some(params))
			.await
	}

	/// Gets the slot of the lowest confirmed block
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - First available slot or error
	pub async fn get_first_available_block(&self) -> Result<Value, TransportError> {
		self.http_client
			.send_raw_request::<Value>(rpc_methods::GET_FIRST_AVAILABLE_BLOCK, None)
			.await
	}

	/// Gets the minimum slot that the node has information about
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Minimum ledger slot or error
	pub async fn minimum_ledger_slot(&self) -> Result<Value, TransportError> {
		self.http_client
			.send_raw_request::<Value>(rpc_methods::MINIMUM_LEDGER_SLOT, None)
			.await
	}

	/// Gets the node version
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Version info or error
	pub async fn get_version(&self) -> Result<Value, TransportError> {
		self.http_client
			.send_raw_request::<Value>(rpc_methods::GET_VERSION, None)
			.await
	}

	/// Gets the health status of the node
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - Health status or error
	pub async fn get_health(&self) -> Result<Value, TransportError> {
		self.http_client
			.send_raw_request::<Value>(rpc_methods::GET_HEALTH, None)
			.await
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for SolanaTransportClient {
	/// Gets the current active RPC URL
	///
	/// # Returns
	/// * `String` - The currently active RPC endpoint URL
	async fn get_current_url(&self) -> String {
		self.http_client.get_current_url().await
	}

	/// Sends a raw JSON-RPC request to the Solana node
	///
	/// # Arguments
	/// * `method` - The JSON-RPC method to call
	/// * `params` - Optional parameters to pass with the request
	///
	/// # Returns
	/// * `Result<Value, TransportError>` - The JSON response or error
	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone + Serialize,
	{
		self.http_client.send_raw_request(method, params).await
	}

	/// Update endpoint manager with a new client
	///
	/// # Arguments
	/// * `client` - The new client to use for the endpoint manager
	fn update_endpoint_manager_client(
		&mut self,
		client: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		self.http_client.update_endpoint_manager_client(client)
	}
}

#[async_trait::async_trait]
impl RotatingTransport for SolanaTransportClient {
	/// Tests connection to a specific URL
	///
	/// # Arguments
	/// * `url` - The URL to test connection with
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		self.http_client.try_connect(url).await
	}

	/// Updates the client to use a new URL
	///
	/// # Arguments
	/// * `url` - The new URL to use for subsequent requests
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error status
	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		self.http_client.update_client(url).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_solana_transport_implements_traits() {
		// This test ensures the types implement the required traits at compile time
		fn assert_blockchain_transport<T: BlockchainTransport>() {}
		fn assert_rotating_transport<T: RotatingTransport>() {}
		fn assert_send_sync<T: Send + Sync>() {}
		fn assert_clone<T: Clone>() {}

		assert_blockchain_transport::<SolanaTransportClient>();
		assert_rotating_transport::<SolanaTransportClient>();
		assert_send_sync::<SolanaTransportClient>();
		assert_clone::<SolanaTransportClient>();
	}
}
