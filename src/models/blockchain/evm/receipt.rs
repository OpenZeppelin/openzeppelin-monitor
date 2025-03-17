//! EVM receipt data structures.

use std::ops::Deref;

use serde::{Deserialize, Serialize};

use alloy::{
	consensus::{Eip658Value, ReceiptEnvelope},
	primitives::{aliases::B2048, Address, Bytes, Log as AlloyLog, B256, U256, U64},
	rpc::types::{Index, TransactionReceipt as AlloyTransactionReceipt},
};

/// Base Receipt struct
/// Copied from web3 crate (now deprecated) and slightly modified for alloy compatibility
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseReceipt {
	/// Transaction hash.
	#[serde(rename = "transactionHash")]
	pub transaction_hash: B256,
	/// Index within the block.
	#[serde(rename = "transactionIndex")]
	pub transaction_index: Index,
	/// Hash of the block this transaction was included within.
	#[serde(rename = "blockHash")]
	pub block_hash: Option<B256>,
	/// Number of the block this transaction was included within.
	#[serde(rename = "blockNumber")]
	pub block_number: Option<U64>,
	/// Sender
	/// Note: default address if the client did not return this value
	/// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
	#[serde(default)]
	pub from: Address,
	/// Recipient (None when contract creation)
	/// Note: Also `None` if the client did not return this value
	/// (maintains backwards compatibility for <= 0.7.0 when this field was missing)
	#[serde(default)]
	pub to: Option<Address>,
	/// Cumulative gas used within the block after this was executed.
	#[serde(rename = "cumulativeGasUsed")]
	pub cumulative_gas_used: U256,
	/// Gas used by this transaction alone.
	///
	/// Gas used is `None` if the the client is running in light client mode.
	#[serde(rename = "gasUsed")]
	pub gas_used: Option<U256>,
	/// Contract address created, or `None` if not a deployment.
	#[serde(rename = "contractAddress")]
	pub contract_address: Option<Address>,
	/// Logs generated within this transaction.
	pub logs: Vec<BaseLog>,
	/// Status: either 1 (success) or 0 (failure).
	pub status: Option<U64>,
	/// State root.
	pub root: Option<B256>,
	/// Logs bloom
	#[serde(rename = "logsBloom")]
	pub logs_bloom: B2048,
	/// Transaction type, Some(1) for AccessList transaction, None for Legacy
	#[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
	pub transaction_type: Option<U64>,
	/// Effective gas price
	#[serde(rename = "effectiveGasPrice")]
	pub effective_gas_price: Option<U256>,
}

/// Base Log struct
/// Copied from web3 crate (now deprecated) and slightly modified for alloy compatibility
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseLog {
	/// H160
	pub address: Address,
	/// Topics
	pub topics: Vec<B256>,
	/// Data
	pub data: Bytes,
	/// Block Hash
	#[serde(rename = "blockHash")]
	pub block_hash: Option<B256>,
	/// Block Number
	#[serde(rename = "blockNumber")]
	pub block_number: Option<U64>,
	/// Transaction Hash
	#[serde(rename = "transactionHash")]
	pub transaction_hash: Option<B256>,
	/// Transaction Index
	#[serde(rename = "transactionIndex")]
	pub transaction_index: Option<Index>,
	/// Log Index in Block
	#[serde(rename = "logIndex")]
	pub log_index: Option<U256>,
	/// Log Index in Transaction
	#[serde(rename = "transactionLogIndex")]
	pub transaction_log_index: Option<U256>,
	/// Log Type
	#[serde(rename = "logType")]
	pub log_type: Option<String>,
	/// Removed
	pub removed: Option<bool>,
}

impl From<AlloyLog> for BaseLog {
	fn from(log: AlloyLog) -> Self {
		Self {
			address: log.address,
			topics: log.topics().to_vec(),
			data: log.data.data,
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: None,
			transaction_log_index: None,
			log_type: None,
			removed: None,
		}
	}
}

/// Wrapper around Base Receipt that implements additional functionality
///
/// This type provides a convenient interface for working with EVM receipts
/// while maintaining compatibility with the alloy types.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TransactionReceipt(pub BaseReceipt);

impl From<BaseReceipt> for TransactionReceipt {
	fn from(tx: BaseReceipt) -> Self {
		Self(tx)
	}
}

impl From<AlloyTransactionReceipt> for TransactionReceipt {
	fn from(receipt: AlloyTransactionReceipt) -> Self {
		let inner_receipt = match &receipt.inner {
			ReceiptEnvelope::Legacy(r) => &r.receipt,
			ReceiptEnvelope::Eip2930(r) => &r.receipt,
			ReceiptEnvelope::Eip1559(r) => &r.receipt,
			ReceiptEnvelope::Eip4844(r) => &r.receipt,
			ReceiptEnvelope::Eip7702(r) => &r.receipt,
		};

		let tx = BaseReceipt {
			transaction_hash: receipt.transaction_hash,
			transaction_index: Index::from(receipt.transaction_index.unwrap_or(0) as usize),
			block_hash: receipt.block_hash,
			block_number: receipt.block_number.map(U64::from),
			from: receipt.from,
			to: receipt.to,
			cumulative_gas_used: U256::from(inner_receipt.cumulative_gas_used),
			gas_used: Some(U256::from(receipt.gas_used)),
			contract_address: receipt.contract_address,
			logs: inner_receipt
				.logs
				.iter()
				.cloned()
				.map(|l| BaseLog::from(alloy::primitives::Log::from(l)))
				.collect(),
			status: match inner_receipt.status {
				Eip658Value::Eip658(status) => Some(U64::from(if status { 1u64 } else { 0u64 })),
				Eip658Value::PostState(_) => Some(U64::from(1u64)),
			},
			root: None,
			logs_bloom: B2048::from_slice(match &receipt.inner {
				ReceiptEnvelope::Legacy(r) => r.logs_bloom.as_slice(),
				ReceiptEnvelope::Eip2930(r) => r.logs_bloom.as_slice(),
				ReceiptEnvelope::Eip1559(r) => r.logs_bloom.as_slice(),
				ReceiptEnvelope::Eip4844(r) => r.logs_bloom.as_slice(),
				ReceiptEnvelope::Eip7702(r) => r.logs_bloom.as_slice(),
			}),
			transaction_type: Some(U64::from(match receipt.inner {
				ReceiptEnvelope::Legacy(_) => 0,
				ReceiptEnvelope::Eip2930(_) => 1,
				ReceiptEnvelope::Eip1559(_) => 2,
				ReceiptEnvelope::Eip4844(_) => 3,
				ReceiptEnvelope::Eip7702(_) => 4,
			})),
			effective_gas_price: Some(U256::from(receipt.effective_gas_price)),
		};
		Self(tx)
	}
}

impl Deref for TransactionReceipt {
	type Target = BaseReceipt;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
