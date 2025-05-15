//! Midnight block data structures.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use serde::{Deserialize, Serialize};
use std::ops::Deref;

use crate::models::MidnightRpcTransactionEnum;

/// Represents a Midnight block
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L214-L218>
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcBlock {
	#[serde(rename = "header")]
	pub header: BlockHeader,
	#[serde(rename = "body")]
	pub body: Vec<MidnightRpcTransactionEnum>,
	#[serde(rename = "transactionsIndex")]
	pub transactions_index: Vec<(String, String)>,
}
/// Represents a Midnight block header
///
/// Based on the response from the Midnight RPC endpoint
/// <https://docs.midnight.network/files/Insomnia_2024-11-21.json>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
	#[serde(rename = "parentHash")]
	/// Hash of the parent block
	pub parent_hash: String, // Hash
	#[serde(rename = "number")]
	/// Block number
	pub number: String, // Hex string
	#[serde(rename = "stateRoot")]
	/// State root hash
	pub state_root: String, // Hash
	#[serde(rename = "extrinsicsRoot")]
	/// Extrinsics root hash
	pub extrinsics_root: String, // Hash
	/// Block digest information
	pub digest: BlockDigest,
}

/// Block digest containing logs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDigest {
	#[serde(rename = "logs")]
	/// Vector of log entries
	pub logs: Vec<String>, // Hex strings
}

/// Wrapper around RpcBlock that implements additional functionality
///
/// This type provides a convenient interface for working with Midnight block data
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block(pub RpcBlock);

impl Block {
	/// Get the block number
	pub fn number(&self) -> Option<u64> {
		Some(u64::from_str_radix(self.0.header.number.trim_start_matches("0x"), 16).unwrap_or(0))
	}
}

impl From<RpcBlock> for Block {
	fn from(header: RpcBlock) -> Self {
		Self(header)
	}
}

impl Deref for Block {
	type Target = RpcBlock;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_block_creation_and_number() {
		let rpc_block = RpcBlock {
			header: BlockHeader {
				parent_hash: "0xabc123".to_string(),
				number: "0x12345".to_string(),
				state_root: "0x1234567890abcdef".to_string(),
				extrinsics_root: "0xabcdef1234567890".to_string(),
				digest: BlockDigest { logs: vec![] },
			},
			body: vec![],
			transactions_index: vec![],
		};

		let block = Block::from(rpc_block.clone());

		// Test number() method
		assert_eq!(block.number(), Some(74565u64)); // decimal representation of 0x12345

		// Test Deref implementation
		assert_eq!(block.header.parent_hash, "0xabc123");
		assert_eq!(block.header.number, "0x12345");
		assert_eq!(block.header.state_root, "0x1234567890abcdef");
		assert_eq!(block.header.extrinsics_root, "0xabcdef1234567890");
		assert_eq!(block.header.digest.logs, Vec::<String>::new());
	}

	#[test]
	fn test_serde_serialization() {
		let rpc_block = RpcBlock {
			header: BlockHeader {
				parent_hash: "0xabc123".to_string(),
				number: "0x12345".to_string(),
				state_root: "0x1234567890abcdef".to_string(),
				extrinsics_root: "0xabcdef1234567890".to_string(),
				digest: BlockDigest { logs: vec![] },
			},
			body: vec![],
			transactions_index: vec![],
		};

		let block = Block(rpc_block);

		// Test serialization
		let serialized = serde_json::to_string(&block).unwrap();

		// Test deserialization
		let deserialized: Block = serde_json::from_str(&serialized).unwrap();

		assert_eq!(deserialized.header.parent_hash, "0xabc123");
		assert_eq!(deserialized.number(), Some(74565u64)); // decimal representation of 0x12345
		assert_eq!(deserialized.header.number, "0x12345");
		assert_eq!(deserialized.header.state_root, "0x1234567890abcdef");
		assert_eq!(deserialized.header.extrinsics_root, "0xabcdef1234567890");
		assert_eq!(deserialized.body, vec![]);
		assert_eq!(deserialized.transactions_index, vec![]);
	}
}
