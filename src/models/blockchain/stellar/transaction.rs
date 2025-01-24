//! Stellar transaction data structures.
//!
//! Note: These structures are based on the Stellar RPC implementation:
//! <https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/methods/get_transactions.go>

use std::ops::Deref;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json;
use stellar_xdr::curr::{Limits, ReadXdr, TransactionEnvelope, TransactionMeta, TransactionResult};

/// Information about a Stellar transaction
///
/// This structure represents the response from the Stellar RPC endpoint
/// and matches the format defined in the stellar-rpc repository.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TransactionInfo {
	// Status fields
	/// Current status of the transaction
	pub status: String,

	/// Hash of the transaction
	#[serde(rename = "txHash")]
	pub transaction_hash: String,

	/// Order of this transaction within its ledger
	#[serde(rename = "applicationOrder")]
	pub application_order: i32,

	/// Whether this is a fee bump transaction
	#[serde(rename = "feeBump")]
	pub fee_bump: bool,

	// XDR and JSON fields
	/// Base64-encoded XDR of the transaction envelope
	#[serde(rename = "envelopeXdr", skip_serializing_if = "Option::is_none")]
	pub envelope_xdr: Option<String>,

	/// Decoded JSON representation of the envelope
	#[serde(rename = "envelopeJson", skip_serializing_if = "Option::is_none")]
	pub envelope_json: Option<serde_json::Value>,

	/// Base64-encoded XDR of the transaction result
	#[serde(rename = "resultXdr", skip_serializing_if = "Option::is_none")]
	pub result_xdr: Option<String>,

	/// Decoded JSON representation of the result
	#[serde(rename = "resultJson", skip_serializing_if = "Option::is_none")]
	pub result_json: Option<serde_json::Value>,

	/// Base64-encoded XDR of the transaction metadata
	#[serde(rename = "resultMetaXdr", skip_serializing_if = "Option::is_none")]
	pub result_meta_xdr: Option<String>,

	/// Decoded JSON representation of the metadata
	#[serde(rename = "resultMetaJson", skip_serializing_if = "Option::is_none")]
	pub result_meta_json: Option<serde_json::Value>,

	// Diagnostic events
	/// Base64-encoded XDR of diagnostic events
	#[serde(
		rename = "diagnosticEventsXdr",
		skip_serializing_if = "Option::is_none"
	)]
	pub diagnostic_events_xdr: Option<Vec<String>>,

	/// Decoded JSON representation of diagnostic events
	#[serde(
		rename = "diagnosticEventsJson",
		skip_serializing_if = "Option::is_none"
	)]
	pub diagnostic_events_json: Option<Vec<serde_json::Value>>,

	// Ledger information
	/// Sequence number of the containing ledger
	pub ledger: u32,

	/// Timestamp when the ledger was closed
	#[serde(rename = "createdAt")]
	pub ledger_close_time: i64,

	// Custom fields
	/// Decoded transaction data
	pub decoded: Option<DecodedTransaction>,
}

/// Decoded transaction data including envelope, result, and metadata
///
/// This structure contains the parsed XDR data from a Stellar transaction.
/// It provides access to the detailed transaction data in a more usable format
/// than the raw base64-encoded XDR strings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DecodedTransaction {
	/// Decoded transaction envelope containing the original transaction data
	pub envelope: Option<TransactionEnvelope>,

	/// Decoded transaction result containing success/failure and return values
	pub result: Option<TransactionResult>,

	/// Decoded transaction metadata containing execution effects
	pub meta: Option<TransactionMeta>,
}

/// Wrapper around TransactionInfo that provides additional functionality
///
/// This type implements convenience methods for working with Stellar transactions
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction(pub TransactionInfo);

impl Transaction {
	/// Get the transaction hash
	pub fn hash(&self) -> &String {
		&self.0.transaction_hash
	}

	/// Get the decoded transaction data if available
	///
	/// Returns the parsed XDR data including envelope, result, and metadata
	/// if it was successfully decoded during transaction creation.
	pub fn decoded(&self) -> Option<&DecodedTransaction> {
		self.0.decoded.as_ref()
	}

	/// Decode base64-encoded XDR data into raw bytes
	///
	/// This is an internal helper function used during transaction creation
	/// to parse the XDR fields from the RPC response.
	fn decode_xdr(xdr: &str) -> Option<Vec<u8>> {
		base64::engine::general_purpose::STANDARD.decode(xdr).ok()
	}
}

impl From<TransactionInfo> for Transaction {
	fn from(tx: TransactionInfo) -> Self {
		let decoded = DecodedTransaction {
			envelope: tx
				.envelope_xdr
				.as_ref()
				.and_then(|xdr| Self::decode_xdr(xdr))
				.and_then(|bytes| TransactionEnvelope::from_xdr(bytes, Limits::none()).ok()),

			result: tx
				.result_xdr
				.as_ref()
				.and_then(|xdr| Self::decode_xdr(xdr))
				.and_then(|bytes| TransactionResult::from_xdr(bytes, Limits::none()).ok()),

			meta: tx
				.result_meta_xdr
				.as_ref()
				.and_then(|xdr| Self::decode_xdr(xdr))
				.and_then(|bytes| TransactionMeta::from_xdr(bytes, Limits::none()).ok()),
		};

		Self(TransactionInfo {
			decoded: Some(decoded),
			..tx
		})
	}
}

impl Deref for Transaction {
	type Target = TransactionInfo;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
