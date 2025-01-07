use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{MatchConditions, Monitor};

use super::{StellarBlock, StellarTransaction};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorMatch {
    pub monitor: Monitor,
    pub transaction: StellarTransaction,
    pub ledger: StellarBlock,
    pub matched_on: MatchConditions,
    pub matched_on_args: Option<MatchArguments>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamsMap {
    pub signature: String,
    pub args: Option<Vec<MatchParamEntry>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
    pub name: String,
    pub value: String,
    pub kind: String,
    pub indexed: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
    pub functions: Option<Vec<MatchParamsMap>>,
    pub events: Option<Vec<MatchParamsMap>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedOperationResult {
    pub contract_address: String,
    pub function_name: String,
    pub function_signature: String,
    pub arguments: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParamEntry {
    pub value: String,
    pub kind: String,
    pub indexed: bool,
}
