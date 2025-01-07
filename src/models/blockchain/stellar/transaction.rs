use std::ops::Deref;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json;
use stellar_xdr::curr::{Limits, ReadXdr, TransactionEnvelope, TransactionMeta, TransactionResult};

/**
 * Copied from https://github.com/stellar/stellar-rpc/blob/main/cmd/stellar-rpc/internal/methods/get_transactions.go#L58-L93
 * This struct is returned from the Stellar RPC but currently not available in the soroban SDK.
 * So we manually define it here until the SDK is updated.
 */
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionInfo {
    // Status fields
    pub status: String,
    #[serde(rename = "txHash")]
    pub transaction_hash: String,
    #[serde(rename = "applicationOrder")]
    pub application_order: i32,
    #[serde(rename = "feeBump")]
    pub fee_bump: bool,

    // XDR and JSON fields
    #[serde(rename = "envelopeXdr", skip_serializing_if = "Option::is_none")]
    pub envelope_xdr: Option<String>,
    #[serde(rename = "envelopeJson", skip_serializing_if = "Option::is_none")]
    pub envelope_json: Option<serde_json::Value>,

    #[serde(rename = "resultXdr", skip_serializing_if = "Option::is_none")]
    pub result_xdr: Option<String>,
    #[serde(rename = "resultJson", skip_serializing_if = "Option::is_none")]
    pub result_json: Option<serde_json::Value>,

    #[serde(rename = "resultMetaXdr", skip_serializing_if = "Option::is_none")]
    pub result_meta_xdr: Option<String>,
    #[serde(rename = "resultMetaJson", skip_serializing_if = "Option::is_none")]
    pub result_meta_json: Option<serde_json::Value>,

    // Diagnostic events
    #[serde(
        rename = "diagnosticEventsXdr",
        skip_serializing_if = "Option::is_none"
    )]
    pub diagnostic_events_xdr: Option<Vec<String>>,
    #[serde(
        rename = "diagnosticEventsJson",
        skip_serializing_if = "Option::is_none"
    )]
    pub diagnostic_events_json: Option<Vec<serde_json::Value>>,

    // Ledger information
    pub ledger: u32,
    #[serde(rename = "createdAt")]
    pub ledger_close_time: i64,

    // Custom fields not part of the RPC response
    pub decoded: Option<DecodedTransaction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DecodedTransaction {
    pub envelope: Option<TransactionEnvelope>,
    pub result: Option<TransactionResult>,
    pub meta: Option<TransactionMeta>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction(TransactionInfo);

impl Transaction {
    pub fn hash(&self) -> &String {
        &self.0.transaction_hash
    }

    pub fn decoded(&self) -> Option<&DecodedTransaction> {
        self.0.decoded.as_ref()
    }

    fn decode_xdr(xdr: &str) -> Option<Vec<u8>> {
        base64::engine::general_purpose::STANDARD.decode(xdr).ok()
    }
}

impl From<TransactionInfo> for Transaction {
    fn from(tx: TransactionInfo) -> Self {
        let decoded = DecodedTransaction {
            envelope: tx
                .envelope_xdr
                .as_ref()
                .and_then(|xdr| Self::decode_xdr(xdr))
                .and_then(|bytes| TransactionEnvelope::from_xdr(bytes, Limits::none()).ok()),

            result: tx
                .result_xdr
                .as_ref()
                .and_then(|xdr| Self::decode_xdr(xdr))
                .and_then(|bytes| TransactionResult::from_xdr(bytes, Limits::none()).ok()),

            meta: tx
                .result_meta_xdr
                .as_ref()
                .and_then(|xdr| Self::decode_xdr(xdr))
                .and_then(|bytes| TransactionMeta::from_xdr(bytes, Limits::none()).ok()),
        };

        Self(TransactionInfo {
            decoded: Some(decoded),
            ..tx
        })
    }
}

impl Deref for Transaction {
    type Target = TransactionInfo;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
