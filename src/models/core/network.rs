use serde::{Deserialize, Serialize};

use crate::models::{BlockChainType, SecretValue};

/// Configuration for connecting to and interacting with a blockchain network.
///
/// Defines connection details and operational parameters for a specific blockchain network,
/// supporting both EVM and Stellar-based chains.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Network {
	/// Type of blockchain (EVM, Stellar, etc)
	pub network_type: BlockChainType,

	/// Unique identifier for this network
	pub slug: String,

	/// Human-readable name of the network
	pub name: String,

	/// List of RPC endpoints with their weights for load balancing
	pub rpc_urls: Vec<RpcUrl>,

	/// Chain ID for EVM networks
	pub chain_id: Option<u64>,

	/// Network passphrase for Stellar networks
	pub network_passphrase: Option<String>,

	/// Average block time in milliseconds
	pub block_time_ms: u64,

	/// Number of blocks needed for confirmation
	pub confirmation_blocks: u64,

	/// Cron expression for how often to check for new blocks
	pub cron_schedule: String,

	/// Maximum number of past blocks to process
	pub max_past_blocks: Option<u64>,

	/// Whether to store processed blocks
	pub store_blocks: Option<bool>,
}

/// RPC endpoint configuration with load balancing weight
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RpcUrl {
	/// Type of RPC endpoint (e.g. "rpc")
	pub type_: String,

	/// URL of the RPC endpoint (can be a secret value)
	pub url: SecretValue,

	/// Weight for load balancing (0-100)
	pub weight: u32,
}
