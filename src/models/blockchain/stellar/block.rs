use std::ops::Deref;

use serde::{Deserialize, Serialize};
use serde_json::Value;
/*
 * https://github.com/stellar/stellar-rpc/pull/303
 * This struct is returned from the Stellar RPC but currently not available in the soroban SDK.
 * So we manually define it here until the SDK is updated.
 */
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LedgerInfo {
    #[serde(rename = "hash")]
    pub hash: String,

    #[serde(rename = "sequence")]
    pub sequence: u32,

    #[serde(rename = "ledgerCloseTime")]
    pub ledger_close_time: String,

    #[serde(rename = "headerXdr")]
    pub ledger_header: String,

    #[serde(rename = "headerJson")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_header_json: Option<Value>,

    #[serde(rename = "metadataXdr")]
    pub ledger_metadata: String,

    #[serde(rename = "metadataJSON")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ledger_metadata_json: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block(LedgerInfo);

impl Block {
    pub fn number(&self) -> u64 {
        self.0.sequence as u64
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
