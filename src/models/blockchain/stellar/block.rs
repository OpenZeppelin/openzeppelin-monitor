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
#[derive(Debug, Serialize, Deserialize, Clone)]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
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
