//! Monitor implementation for Stellar blockchain.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{MatchConditions, Monitor, StellarBlock, StellarTransaction};

/// Result of a successful monitor match on a Stellar chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: StellarTransaction,

	/// Ledger containing the matched transaction
	pub ledger: StellarBlock,

	/// Network slug that the transaction was sent from
	pub network_slug: String,

	/// Conditions that were matched
	pub matched_on: MatchConditions,

	/// Decoded arguments from the matched conditions
	pub matched_on_args: Option<MatchArguments>,
}

/// Collection of decoded parameters from matched conditions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamsMap {
	/// Function or event signature
	pub signature: String,

	/// Decoded argument values
	pub args: Option<Vec<MatchParamEntry>>,
}

/// Single decoded parameter from a function or event
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
	/// Parameter name
	pub name: String,

	/// Parameter value
	pub value: String,

	/// Parameter type
	pub kind: String,

	/// Whether this is an indexed parameter
	pub indexed: bool,
}

/// Arguments matched from functions and events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched function arguments
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched event arguments
	pub events: Option<Vec<MatchParamsMap>>,
}

/// Parsed result of a Stellar contract operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedOperationResult {
	/// Address of the contract that was called
	pub contract_address: String,

	/// Name of the function that was called
	pub function_name: String,

	/// Full function signature
	pub function_signature: String,

	/// Decoded function arguments
	pub arguments: Vec<Value>,
}

/// Decoded parameter from a Stellar contract function or event
///
/// This structure represents a single decoded parameter from a contract interaction,
/// providing the parameter's value, type information, and indexing status.
/// Similar to EVM event/function parameters but adapted for Stellar's type system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParamEntry {
	/// String representation of the parameter value
	pub value: String,

	/// Parameter type (e.g., "address", "i128", "bytes")
	pub kind: String,

	/// Whether this parameter is indexed (for event topics)
	pub indexed: bool,
}
