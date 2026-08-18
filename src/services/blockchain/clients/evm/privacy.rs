//! Besu/Tessera private transaction response models and normalization helpers.

use alloy::{
	primitives::{aliases::B2048, Address, Bytes, B256, U256, U64},
	rpc::types::Index,
};
use serde::Deserialize;

use crate::models::{
	EVMBaseReceipt, EVMBaseTransaction, EVMReceiptLog, EVMTransaction, EVMTransactionReceipt,
};

/// Decrypted transaction payload returned by `priv_getPrivateTransaction`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BesuPrivateTransaction {
	#[serde(default)]
	pub from: Option<Address>,
	#[serde(default)]
	pub to: Option<Address>,
	#[serde(default)]
	pub input: Bytes,
	#[serde(default)]
	pub nonce: U256,
	#[serde(default)]
	pub value: U256,
	#[serde(default)]
	pub gas: U256,
	#[serde(default)]
	pub gas_price: Option<U256>,
	#[serde(default)]
	pub private_from: Option<String>,
	#[serde(default)]
	pub private_for: Vec<String>,
	#[serde(default)]
	pub privacy_group_id: Option<String>,
	#[serde(default)]
	pub restriction: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BesuPrivateReceipt {
	#[serde(default)]
	pub from: Option<Address>,
	#[serde(default)]
	pub to: Option<Address>,
	#[serde(default)]
	pub contract_address: Option<Address>,
	#[serde(default)]
	pub cumulative_gas_used: U256,
	#[serde(default)]
	pub gas_used: Option<U256>,
	#[serde(default)]
	pub status: Option<U64>,
	#[serde(default)]
	pub logs: Vec<BesuPrivateLog>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BesuPrivateLog {
	pub address: Address,
	#[serde(default)]
	pub topics: Vec<B256>,
	#[serde(default)]
	pub data: Bytes,
}

pub(super) fn normalize_private_transaction(
	public_pmt: &EVMTransaction,
	private: BesuPrivateTransaction,
) -> EVMTransaction {
	let mut extra = public_pmt.extra.clone();
	if let Some(private_from) = private.private_from {
		extra.insert("privateFrom".to_string(), private_from.into());
	}
	if !private.private_for.is_empty() {
		extra.insert("privateFor".to_string(), private.private_for.into());
	}
	if let Some(restriction) = private.restriction {
		extra.insert("restriction".to_string(), restriction.into());
	}

	EVMTransaction(EVMBaseTransaction {
		hash: public_pmt.hash,
		nonce: private.nonce,
		block_hash: public_pmt.block_hash,
		block_number: public_pmt.block_number,
		transaction_index: public_pmt.transaction_index,
		is_private: true,
		privacy_group_id: private.privacy_group_id,
		from: private.from.or(public_pmt.from),
		to: private.to,
		value: private.value,
		gas_price: private.gas_price.or(public_pmt.gas_price),
		gas: private.gas,
		input: private.input,
		v: public_pmt.v,
		r: public_pmt.r,
		s: public_pmt.s,
		raw: public_pmt.raw.clone(),
		transaction_type: public_pmt.transaction_type,
		access_list: public_pmt.access_list.clone(),
		max_fee_per_gas: public_pmt.max_fee_per_gas,
		max_priority_fee_per_gas: public_pmt.max_priority_fee_per_gas,
		l2: public_pmt.l2.clone(),
		extra,
	})
}

pub(super) fn normalize_private_receipt(
	transaction: &EVMTransaction,
	private: BesuPrivateReceipt,
) -> EVMTransactionReceipt {
	let transaction_index = transaction.transaction_index.unwrap_or(Index::from(0));
	let logs = private
		.logs
		.into_iter()
		.enumerate()
		.map(|(log_index, log)| EVMReceiptLog {
			address: log.address,
			topics: log.topics,
			data: log.data,
			block_hash: transaction.block_hash,
			block_number: transaction.block_number,
			transaction_hash: Some(transaction.hash),
			transaction_index: Some(transaction_index),
			log_index: Some(U256::from(log_index)),
			transaction_log_index: Some(U256::from(log_index)),
			log_type: None,
			removed: Some(false),
		})
		.collect();

	EVMTransactionReceipt(EVMBaseReceipt {
		transaction_hash: transaction.hash,
		transaction_index,
		block_hash: transaction.block_hash,
		block_number: transaction.block_number,
		from: private.from.or(transaction.from).unwrap_or_default(),
		to: private.to.or(transaction.to),
		cumulative_gas_used: private.cumulative_gas_used,
		gas_used: private.gas_used,
		contract_address: private.contract_address,
		logs,
		status: private.status,
		root: None,
		logs_bloom: B2048::default(),
		transaction_type: transaction.transaction_type,
		effective_gas_price: transaction.gas_price,
	})
}
