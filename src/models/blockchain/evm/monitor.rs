use serde::{Deserialize, Serialize};

use crate::models::{EVMTransaction, MatchConditions, Monitor};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EVMMonitorMatch {
    pub monitor: Monitor,
    pub transaction: EVMTransaction,
    pub receipt: web3::types::TransactionReceipt,
    pub matched_on: MatchConditions,
    pub matched_on_args: Option<MatchArguments>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamsMap {
    pub signature: String,
    pub args: Option<Vec<MatchParamEntry>>,
    pub hex_signature: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
    pub name: String,
    pub value: String,
    pub indexed: bool,
    pub kind: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
    pub functions: Option<Vec<MatchParamsMap>>,
    pub events: Option<Vec<MatchParamsMap>>,
}
