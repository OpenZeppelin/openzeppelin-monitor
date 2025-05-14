use serde::{Deserialize, Serialize};

use crate::models::blockchain::ContractSpec;

/// Configuration for monitoring specific blockchain activity.
///
/// A Monitor defines what blockchain activity to watch for through a combination of:
/// - Network targets (which chains to monitor)
/// - Contract addresses to watch
/// - Conditions to match (functions, events, transactions)
/// - Triggers conditions refers to a custom filter script that being executed apply extra filters
///   to the matched transactions before triggering the notifications
/// - Triggers to execute when conditions are met
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
	/// Unique name identifying this monitor
	pub name: String,

	/// List of network slugs this monitor should watch
	pub networks: Vec<String>,

	/// Whether this monitor is currently paused
	pub paused: bool,

	/// Contract addresses to monitor, optionally with their contract specs
	pub addresses: Vec<AddressWithSpec>,

	/// Conditions that should trigger this monitor
	pub match_conditions: MatchConditions,

	/// Conditions that should be met prior to triggering notifications
	pub trigger_conditions: Vec<TriggerConditions>,

	/// IDs of triggers to execute when conditions match
	pub triggers: Vec<String>,
}

/// Contract address with optional ABI for decoding transactions and events
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddressWithSpec {
	/// Contract address in the network's native format
	pub address: String,

	/// Optional contract spec for decoding contract interactions
	pub contract_spec: Option<ContractSpec>,
}

/// Collection of conditions that can trigger a monitor
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct MatchConditions {
	/// Function calls to match
	pub functions: Vec<FunctionCondition>,

	/// Events to match
	pub events: Vec<EventCondition>,

	/// Transaction states to match
	pub transactions: Vec<TransactionCondition>,
}

/// Condition for matching contract function calls
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FunctionCondition {
	/// Function signature (e.g., "transfer(address,uint256)")
	pub signature: String,

	/// Optional expression to filter function parameters
	pub expression: Option<String>,
}

/// Condition for matching contract events
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventCondition {
	/// Event signature (e.g., "Transfer(address,address,uint256)")
	pub signature: String,

	/// Optional expression to filter event parameters
	pub expression: Option<String>,
}

/// Condition for matching transaction states
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TransactionCondition {
	/// Required transaction status
	pub status: TransactionStatus,

	/// Optional expression to filter transaction properties
	pub expression: Option<String>,
}

/// Possible transaction execution states
#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub enum TransactionStatus {
	/// Match any transaction status
	Any,
	/// Match only successful transactions
	Success,
	/// Match only failed transactions
	Failure,
}

/// Conditions that should be met prior to triggering notifications
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TriggerConditions {
	/// The path to the script
	pub script_path: String,

	/// The arguments of the script
	#[serde(default)]
	pub arguments: Option<Vec<String>>,

	/// The language of the script
	pub language: ScriptLanguage,

	/// The timeout of the script
	pub timeout_ms: u32,
}
/// The possible languages of the script
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub enum ScriptLanguage {
	JavaScript,
	Python,
	Bash,
}
