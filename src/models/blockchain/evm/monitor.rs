use serde::{Deserialize, Serialize};

use crate::models::{EVMTransaction, EVMTransactionReceipt, MatchConditions, Monitor};

/// Result of a successful monitor match on an EVM chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EVMMonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: EVMTransaction,

	/// Transaction receipt with execution results
	pub receipt: EVMTransactionReceipt,

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

	/// Raw function/event signature as bytes
	pub hex_signature: Option<String>,
}

/// Single decoded parameter from a function or event
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
	/// Parameter name
	pub name: String,

	/// Parameter value
	pub value: String,

	/// Whether this is an indexed parameter (for events)
	pub indexed: bool,

	/// Parameter type (uint256, address, etc)
	pub kind: String,
}

/// Arguments matched from functions and events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched function arguments
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched event arguments
	pub events: Option<Vec<MatchParamsMap>>,
}
