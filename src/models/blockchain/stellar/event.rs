/**
 * Copied from https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/methods/get_events.go#L88-L107
 * This struct is returned from the Stellar RPC
 */
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Event {
    #[serde(rename = "type")]
    pub event_type: String,

    pub ledger: u32,
    #[serde(rename = "ledgerClosedAt")]
    pub ledger_closed_at: String,

    #[serde(rename = "contractId")]
    pub contract_id: String,

    pub id: String,

    // Deprecated: Use cursor at top level for pagination
    #[serde(rename = "pagingToken")]
    pub paging_token: String,

    #[serde(rename = "inSuccessfulContractCall")]
    pub in_successful_contract_call: bool,

    #[serde(rename = "txHash")]
    pub transaction_hash: String,

    // Base64-encoded list of ScVals
    #[serde(rename = "topic", skip_serializing_if = "Option::is_none")]
    pub topic_xdr: Option<Vec<String>>,
    #[serde(rename = "topicJson", skip_serializing_if = "Option::is_none")]
    pub topic_json: Option<Vec<serde_json::Value>>,

    // Base64-encoded ScVal
    #[serde(rename = "value", skip_serializing_if = "Option::is_none")]
    pub value_xdr: Option<String>,
    #[serde(rename = "valueJson", skip_serializing_if = "Option::is_none")]
    pub value_json: Option<serde_json::Value>,
}
