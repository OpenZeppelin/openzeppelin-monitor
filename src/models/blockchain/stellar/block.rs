//! Stellar block (ledger) data structures.
//!
//! Note: These structures are based on the Stellar RPC implementation:
//! <https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/methods/get_ledgers.go>

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Information about a Stellar ledger (block)
///
/// This structure represents the response from the Stellar RPC endpoint
/// and matches the format defined in the stellar-rpc repository.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LedgerInfo {
	/// Hash of the ledger
	#[serde(rename = "hash")]
	pub hash: String,

	/// Sequence number of the ledger
	#[serde(rename = "sequence")]
	pub sequence: u32,

	/// Timestamp when the ledger was closed
	#[serde(rename = "ledgerCloseTime")]
	pub ledger_close_time: String,

	/// Base64-encoded XDR of the ledger header
	#[serde(rename = "headerXdr")]
	pub ledger_header: String,

	/// Decoded JSON representation of the ledger header
	#[serde(rename = "headerJson")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ledger_header_json: Option<Value>,

	/// Base64-encoded XDR of the ledger metadata
	#[serde(rename = "metadataXdr")]
	pub ledger_metadata: String,

	/// Decoded JSON representation of the ledger metadata
	#[serde(rename = "metadataJSON")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ledger_metadata_json: Option<Value>,
}

/// Wrapper around LedgerInfo that implements additional functionality
///
/// This type provides a convenient interface for working with Stellar ledger data
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block(pub LedgerInfo);

impl Block {
	/// Get the block number (sequence)
	pub fn number(&self) -> Option<u64> {
		Some(self.0.sequence as u64)
	}
}

impl From<LedgerInfo> for Block {
	fn from(header: LedgerInfo) -> Self {
		Self(header)
	}
}

impl Deref for Block {
	type Target = LedgerInfo;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_block_creation_and_number() {
		let ledger_info = LedgerInfo {
			hash: "abc123".to_string(),
			sequence: 12345,
			ledger_close_time: "2024-03-20T10:00:00Z".to_string(),
			ledger_header: "base64header".to_string(),
			ledger_header_json: Some(json!({"version": 1})),
			ledger_metadata: "base64metadata".to_string(),
			ledger_metadata_json: Some(json!({"operations": []})),
		};

		let block = Block::from(ledger_info.clone());

		// Test number() method
		assert_eq!(block.number(), Some(12345u64));

		// Test Deref implementation
		assert_eq!(block.hash, "abc123");
		assert_eq!(block.sequence, 12345);
		assert_eq!(block.ledger_close_time, "2024-03-20T10:00:00Z");
		assert_eq!(block.ledger_header, "base64header");
		assert_eq!(block.ledger_metadata, "base64metadata");
	}

	#[test]
	fn test_default_implementation() {
		let block = Block::default();

		assert_eq!(block.hash, "");
		assert_eq!(block.sequence, 0);
		assert_eq!(block.ledger_close_time, "");
		assert_eq!(block.ledger_header, "");
		assert_eq!(block.ledger_metadata, "");
		assert!(block.ledger_header_json.is_none());
		assert!(block.ledger_metadata_json.is_none());
	}

	#[test]
	fn test_serde_serialization() {
		let ledger_info = LedgerInfo {
			hash: "abc123".to_string(),
			sequence: 12345,
			ledger_close_time: "2024-03-20T10:00:00Z".to_string(),
			ledger_header: "base64header".to_string(),
			ledger_header_json: Some(json!({"version": 1})),
			ledger_metadata: "base64metadata".to_string(),
			ledger_metadata_json: Some(json!({"operations": []})),
		};

		let block = Block(ledger_info);

		// Test serialization
		let serialized = serde_json::to_string(&block).unwrap();

		// Test deserialization
		let deserialized: Block = serde_json::from_str(&serialized).unwrap();

		assert_eq!(deserialized.hash, "abc123");
		assert_eq!(deserialized.sequence, 12345);
		assert_eq!(deserialized.number(), Some(12345u64));
	}
}
