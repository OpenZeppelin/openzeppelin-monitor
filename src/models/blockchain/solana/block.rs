//! Solana block data structures.
//!
//! Note: These structures are based on the Solana RPC implementation:
//! <https://solana.com/docs/rpc/http/getblock>

use serde::{Deserialize, Serialize};
use std::ops::Deref;

use crate::models::SolanaTransaction;

/// A confirmed block from the Solana blockchain.
///
/// This structure represents the response from the Solana RPC `getBlock` endpoint
/// and matches the format defined in the Solana JSON-RPC specification.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedBlock {
	/// The slot number this block was produced in
	pub slot: u64,

	/// Hash of the block
	pub blockhash: String,

	/// Hash of the previous block
	pub previous_blockhash: String,

	/// The slot of the parent block
	pub parent_slot: u64,

	/// Timestamp when the block was produced (Unix timestamp in seconds)
	pub block_time: Option<i64>,

	/// Block height (may be None for older blocks)
	pub block_height: Option<u64>,

	/// Transactions in this block
	#[serde(default)]
	pub transactions: Vec<SolanaTransaction>,
}

/// Wrapper around ConfirmedBlock that implements additional functionality
///
/// This type provides a convenient interface for working with Solana block data
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block(pub ConfirmedBlock);

impl Block {
	/// Get the block number (slot)
	pub fn number(&self) -> Option<u64> {
		Some(self.0.slot)
	}

	/// Get the blockhash
	pub fn blockhash(&self) -> &str {
		&self.0.blockhash
	}

	/// Get the block timestamp
	pub fn block_time(&self) -> Option<i64> {
		self.0.block_time
	}
}

impl From<ConfirmedBlock> for Block {
	fn from(confirmed_block: ConfirmedBlock) -> Self {
		Self(confirmed_block)
	}
}

impl Deref for Block {
	type Target = ConfirmedBlock;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_block_creation_and_number() {
		let confirmed_block = ConfirmedBlock {
			slot: 123456789,
			blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			previous_blockhash: "3sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			parent_slot: 123456788,
			block_time: Some(1234567890),
			block_height: Some(100000000),
			transactions: vec![],
		};

		let block = Block::from(confirmed_block.clone());

		// Test number() method
		assert_eq!(block.number(), Some(123456789));

		// Test Deref implementation
		assert_eq!(
			block.blockhash,
			"4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn"
		);
		assert_eq!(block.slot, 123456789);
		assert_eq!(block.block_time, Some(1234567890));
	}

	#[test]
	fn test_default_implementation() {
		let block = Block::default();

		assert_eq!(block.slot, 0);
		assert_eq!(block.blockhash, "");
		assert_eq!(block.previous_blockhash, "");
		assert_eq!(block.parent_slot, 0);
		assert!(block.block_time.is_none());
		assert!(block.block_height.is_none());
		assert!(block.transactions.is_empty());
	}

	#[test]
	fn test_serde_serialization() {
		let confirmed_block = ConfirmedBlock {
			slot: 123456789,
			blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			previous_blockhash: "3sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			parent_slot: 123456788,
			block_time: Some(1234567890),
			block_height: Some(100000000),
			transactions: vec![],
		};

		let block = Block(confirmed_block);

		// Test serialization
		let serialized = serde_json::to_string(&block).unwrap();

		// Test deserialization
		let deserialized: Block = serde_json::from_str(&serialized).unwrap();

		assert_eq!(
			deserialized.blockhash,
			"4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn"
		);
		assert_eq!(deserialized.slot, 123456789);
		assert_eq!(deserialized.number(), Some(123456789));
	}

	#[test]
	fn test_block_helpers() {
		let confirmed_block = ConfirmedBlock {
			slot: 123456789,
			blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			previous_blockhash: "3sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
			parent_slot: 123456788,
			block_time: Some(1234567890),
			block_height: Some(100000000),
			transactions: vec![],
		};

		let block = Block::from(confirmed_block);

		assert_eq!(
			block.blockhash(),
			"4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn"
		);
		assert_eq!(block.block_time(), Some(1234567890));
	}
}
