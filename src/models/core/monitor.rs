use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Monitor {
    pub name: String,
    pub networks: Vec<String>,
    pub paused: bool,
    pub addresses: Vec<AddressWithABI>,
    pub match_conditions: MatchConditions,
    pub triggers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddressWithABI {
    pub address: String,
    pub abi: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchConditions {
    pub functions: Vec<FunctionCondition>,
    pub events: Vec<EventCondition>,
    pub transactions: Vec<TransactionCondition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionCondition {
    pub signature: String,
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventCondition {
    pub signature: String,
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionCondition {
    pub status: TransactionStatus,
    pub expression: Option<String>,
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize, PartialEq)]
pub enum TransactionStatus {
    Any,
    Success,
    Failure,
}
