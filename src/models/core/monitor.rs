use serde::{Deserialize, Serialize};

/// Configuration for monitoring specific blockchain activity.
///
/// A Monitor defines what blockchain activity to watch for through a combination of:
/// - Network targets (which chains to monitor)
/// - Contract addresses to watch
/// - Conditions to match (functions, events, transactions)
/// - Triggers to execute when conditions are met
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Monitor {
    /// Unique name identifying this monitor
    pub name: String,

    /// List of network slugs this monitor should watch
    pub networks: Vec<String>,

    /// Whether this monitor is currently paused
    pub paused: bool,

    /// Contract addresses to monitor, optionally with their ABIs
    pub addresses: Vec<AddressWithABI>,

    /// Conditions that should trigger this monitor
    pub match_conditions: MatchConditions,

    /// IDs of triggers to execute when conditions match
    pub triggers: Vec<String>,
}

/// Contract address with optional ABI for decoding transactions and events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddressWithABI {
    /// Contract address in the network's native format
    pub address: String,

    /// Optional ABI for decoding contract interactions
    pub abi: Option<serde_json::Value>,
}

/// Collection of conditions that can trigger a monitor
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchConditions {
    /// Function calls to match
    pub functions: Vec<FunctionCondition>,

    /// Events to match
    pub events: Vec<EventCondition>,

    /// Transaction states to match
    pub transactions: Vec<TransactionCondition>,
}

/// Condition for matching contract function calls
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionCondition {
    /// Function signature (e.g., "transfer(address,uint256)")
    pub signature: String,

    /// Optional expression to filter function parameters
    pub expression: Option<String>,
}

/// Condition for matching contract events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventCondition {
    /// Event signature (e.g., "Transfer(address,address,uint256)")
    pub signature: String,

    /// Optional expression to filter event parameters
    pub expression: Option<String>,
}

/// Condition for matching transaction states
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionCondition {
    /// Required transaction status
    pub status: TransactionStatus,

    /// Optional expression to filter transaction properties
    pub expression: Option<String>,
}

/// Possible transaction execution states
#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq)]
pub enum TransactionStatus {
    /// Match any transaction status
    Any,
    /// Match only successful transactions
    Success,
    /// Match only failed transactions
    Failure,
}
