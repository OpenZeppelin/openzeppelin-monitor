use serde::{
	de::{self, Visitor},
	Deserialize, Deserializer, Serialize, Serializer,
};

use crate::models::{BlockChainType, SecretValue};

/// Maximum number of past blocks to process for a network.
///
/// Serialized as a JSON number for `Limited(n)` or the string `"unlimited"`
/// for `Unlimited`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaxPastBlocks {
	/// Process at most this many past blocks behind the latest confirmed block
	Limited(u64),
	/// No limit: never skip blocks, always resume from the last processed block
	Unlimited,
}

impl std::fmt::Display for MaxPastBlocks {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			MaxPastBlocks::Limited(blocks) => write!(f, "{}", blocks),
			MaxPastBlocks::Unlimited => write!(f, "unlimited"),
		}
	}
}

impl Serialize for MaxPastBlocks {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match self {
			MaxPastBlocks::Limited(blocks) => serializer.serialize_u64(*blocks),
			MaxPastBlocks::Unlimited => serializer.serialize_str("unlimited"),
		}
	}
}

impl<'de> Deserialize<'de> for MaxPastBlocks {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct MaxPastBlocksVisitor;

		impl Visitor<'_> for MaxPastBlocksVisitor {
			type Value = MaxPastBlocks;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("a non-negative integer or the string \"unlimited\"")
			}

			fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				Ok(MaxPastBlocks::Limited(value))
			}

			fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				u64::try_from(value)
					.map(MaxPastBlocks::Limited)
					.map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))
			}

			fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
			where
				E: de::Error,
			{
				if value == "unlimited" {
					Ok(MaxPastBlocks::Unlimited)
				} else {
					Err(E::invalid_value(de::Unexpected::Str(value), &self))
				}
			}
		}

		deserializer.deserialize_any(MaxPastBlocksVisitor)
	}
}

/// Configuration for missed block recovery job.
///
/// Defines parameters for the background job that retries fetching and processing
/// blocks that were missed during normal monitoring cycles.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BlockRecoveryConfig {
	/// Whether the recovery job is enabled
	pub enabled: bool,

	/// Cron schedule for the recovery job (e.g., "0 */5 * * * *" for every 5 minutes)
	pub cron_schedule: String,

	/// Maximum number of blocks to attempt recovery per execution
	pub max_blocks_per_run: u64,

	/// Maximum age of missed blocks to consider (in blocks from current)
	/// Blocks older than this are pruned and not recovered
	pub max_block_age: u64,

	/// Maximum number of retry attempts per block before marking as failed
	pub max_retries: u32,

	/// Delay in milliseconds between retry attempts for failed blocks
	pub retry_delay_ms: u64,
}

/// Configuration for connecting to and interacting with a blockchain network.
///
/// Defines connection details and operational parameters for a specific blockchain network.
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

	/// Maximum number of past blocks to process (a number or "unlimited")
	pub max_past_blocks: Option<MaxPastBlocks>,

	/// Whether to store processed blocks
	pub store_blocks: Option<bool>,

	/// Configuration for missed block recovery job
	pub recovery_config: Option<BlockRecoveryConfig>,
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
