//! Midnight block data structures.
//!
//! This module provides data structures and implementations for working with Midnight blockchain blocks.
//! It includes types for representing block headers, digests, and block data from RPC responses.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use serde::{Deserialize, Serialize};
use std::ops::Deref;

use crate::models::MidnightRpcTransactionEnum;

/// Represents a Midnight block
///
/// This struct contains the block header, body (transactions), and transaction indices.
/// The transactions_index field is renamed from "transactionsIndex" in the RPC response
/// to follow Rust naming conventions.
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L214-L218>
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcBlock<Header = BlockHeader> {
	/// The block header containing metadata about the block
	pub header: Header,
	/// The list of transactions in the block
	pub body: Vec<MidnightRpcTransactionEnum>,
	// NOTE: This should be `transactionsIndex` in the RPC response but it's not
	// so we're using `transactions_index` here but expect this may change in the future
	#[serde(rename = "transactions_index")]
	pub transactions_index: Vec<(String, String)>,
}

/// Represents a Midnight block header
///
/// This struct contains the essential metadata for a Midnight block, including
/// parent hash, block number, state root, and digest information.
///
/// Based on the response from the Midnight RPC endpoint
/// <https://docs.midnight.network/files/Insomnia_2024-11-21.json>
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
	/// Hash of the parent block
	pub parent_hash: String, // Hash
	/// Block number in hexadecimal format
	pub number: String, // Hex string
	/// State root hash representing the final state after applying all transactions
	pub state_root: String, // Hash
	/// Extrinsics root hash representing the Merkle root of all transactions
	pub extrinsics_root: String, // Hash
	/// Block digest containing additional block information
	pub digest: BlockDigest,
}

/// Block digest containing logs
///
/// This struct represents the digest information for a block, which includes
/// various logs and metadata about the block's processing.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct BlockDigest {
	/// Vector of log entries in hexadecimal format
	pub logs: Vec<String>, // Hex strings
}

/// Wrapper around RpcBlock that implements additional functionality
///
/// This type provides a convenient interface for working with Midnight block data
/// while maintaining compatibility with the RPC response format. It implements
/// methods for accessing and processing block information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block(pub RpcBlock);

impl Block {
	/// Get the block number as a decimal value
	///
	/// Converts the hexadecimal block number to a decimal u64 value.
	/// Returns None if the conversion fails.
	///
	/// # Returns
	/// * `Option<u64>` - The block number as a decimal value, or None if conversion fails
	pub fn number(&self) -> Option<u64> {
		Some(u64::from_str_radix(self.0.header.number.trim_start_matches("0x"), 16).unwrap_or(0))
	}
}

impl From<RpcBlock> for Block {
	/// Creates a new Block from an RpcBlock
	///
	/// # Arguments
	/// * `header` - The RpcBlock to convert
	///
	/// # Returns
	/// A new Block instance
	fn from(header: RpcBlock) -> Self {
		Self(header)
	}
}

impl Deref for Block {
	type Target = RpcBlock;

	/// Dereferences the Block to access the underlying RpcBlock
	///
	/// # Returns
	/// A reference to the underlying RpcBlock
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests block creation and number conversion
	#[test]
	fn test_block_creation_and_number() {
		let rpc_block = RpcBlock::<BlockHeader> {
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

	/// Tests serialization and deserialization of Block
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
