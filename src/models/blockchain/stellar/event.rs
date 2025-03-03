//! Stellar contract event data structures.
//!
//! Note: These structures are based on the Stellar RPC implementation:
//! <https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/methods/get_events.go>

use serde::{Deserialize, Serialize};

/// Represents a contract event emitted during transaction execution
///
/// This structure represents the response from the Stellar RPC endpoint
/// and matches the format defined in the stellar-rpc repository.
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Event {
	/// Type of the event
	#[serde(rename = "type")]
	pub event_type: String,

	/// Ledger sequence number containing this event
	pub ledger: u32,

	/// Timestamp when the ledger was closed
	#[serde(rename = "ledgerClosedAt")]
	pub ledger_closed_at: String,

	/// Contract address that emitted the event
	#[serde(rename = "contractId")]
	pub contract_id: String,

	/// Unique identifier for this event
	pub id: String,

	/// Deprecated: Use cursor at top level for pagination
	#[serde(rename = "pagingToken")]
	pub paging_token: String,

	/// Whether the event was emitted during a successful contract call
	#[serde(rename = "inSuccessfulContractCall")]
	pub in_successful_contract_call: bool,

	/// Transaction hash that generated this event
	#[serde(rename = "txHash")]
	pub transaction_hash: String,

	/// Base64-encoded list of ScVals representing the event topics
	#[serde(rename = "topic", skip_serializing_if = "Option::is_none")]
	pub topic_xdr: Option<Vec<String>>,

	/// Decoded JSON representation of the event topics
	#[serde(rename = "topicJson", skip_serializing_if = "Option::is_none")]
	pub topic_json: Option<Vec<serde_json::Value>>,

	/// Base64-encoded ScVal representing the event value
	#[serde(rename = "value", skip_serializing_if = "Option::is_none")]
	pub value_xdr: Option<String>,

	/// Decoded JSON representation of the event value
	#[serde(rename = "valueJson", skip_serializing_if = "Option::is_none")]
	pub value_json: Option<serde_json::Value>,
}
