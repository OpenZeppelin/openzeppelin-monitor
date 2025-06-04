//! EVM transaction data structures.

use std::{collections::HashMap, ops::Deref};

use serde::{Deserialize, Serialize};

use alloy::{
	consensus::Transaction as AlloyConsensusTransaction,
	primitives::{Address, Bytes, B256, U256, U64},
	rpc::types::{AccessList, Index, Transaction as AlloyTransaction},
};

/// L2-specific transaction fields
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct BaseL2Transaction {
	/// Deposit receipt version (for L2 transactions)
	#[serde(
		rename = "depositReceiptVersion",
		default,
		skip_serializing_if = "Option::is_none"
	)]
	pub deposit_receipt_version: Option<U64>,

	/// Source hash (for L2 transactions)
	#[serde(
		rename = "sourceHash",
		default,
		skip_serializing_if = "Option::is_none"
	)]
	pub source_hash: Option<B256>,

	/// Mint amount (for L2 transactions)
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mint: Option<U256>,

	/// Y parity (alternative to v in some implementations)
	#[serde(rename = "yParity", default, skip_serializing_if = "Option::is_none")]
	pub y_parity: Option<U64>,
}

/// Base Transaction struct
/// Copied from web3 crate (now deprecated) and slightly modified for alloy compatibility
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct BaseTransaction {
	/// Hash
	pub hash: B256,
	/// Nonce
	pub nonce: U256,
	/// Block hash. None when pending.
	#[serde(rename = "blockHash")]
	pub block_hash: Option<B256>,
	/// Block number. None when pending.
	#[serde(rename = "blockNumber")]
	pub block_number: Option<U64>,
	/// Transaction Index. None when pending.
	#[serde(rename = "transactionIndex")]
	pub transaction_index: Option<Index>,
	/// Sender
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub from: Option<Address>,
	/// Recipient (None when contract creation)
	pub to: Option<Address>,
	/// Transferred value
	pub value: U256,
	/// Gas Price
	#[serde(rename = "gasPrice")]
	pub gas_price: Option<U256>,
	/// Gas amount
	pub gas: U256,
	/// Input data
	pub input: Bytes,
	/// ECDSA recovery id
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub v: Option<U64>,
	/// ECDSA signature r, 32 bytes
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub r: Option<U256>,
	/// ECDSA signature s, 32 bytes
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub s: Option<U256>,
	/// Raw transaction data
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub raw: Option<Bytes>,
	/// Transaction type, Some(1) for AccessList transaction, None for Legacy
	#[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
	pub transaction_type: Option<U64>,
	/// Access list
	#[serde(
		rename = "accessList",
		default,
		skip_serializing_if = "Option::is_none"
	)]
	pub access_list: Option<AccessList>,
	/// Max fee per gas
	#[serde(rename = "maxFeePerGas", skip_serializing_if = "Option::is_none")]
	pub max_fee_per_gas: Option<U256>,
	/// miner bribe
	#[serde(
		rename = "maxPriorityFeePerGas",
		skip_serializing_if = "Option::is_none"
	)]
	pub max_priority_fee_per_gas: Option<U256>,

	/// L2-specific transaction fields
	#[serde(flatten)]
	pub l2: BaseL2Transaction,

	/// Catch-all for non-standard fields
	#[serde(flatten)]
	pub extra: HashMap<String, serde_json::Value>,
}

/// Wrapper around Base Transaction that implements additional functionality
///
/// This type provides a convenient interface for working with EVM transactions
/// while maintaining compatibility with the base types.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Transaction(pub BaseTransaction);

impl Transaction {
	/// Get the transaction value (amount of ETH transferred)
	pub fn value(&self) -> &U256 {
		&self.0.value
	}

	/// Get the transaction sender address
	pub fn sender(&self) -> Option<&Address> {
		self.0.from.as_ref()
	}

	/// Get the transaction recipient address (None for contract creation)
	pub fn to(&self) -> Option<&Address> {
		self.0.to.as_ref()
	}

	/// Get the gas limit for the transaction
	pub fn gas(&self) -> &U256 {
		&self.0.gas
	}

	/// Get the gas price (None for EIP-1559 transactions)
	pub fn gas_price(&self) -> Option<&U256> {
		self.0.gas_price.as_ref()
	}

	/// Get the transaction nonce
	pub fn nonce(&self) -> &U256 {
		&self.0.nonce
	}

	/// Get the transaction hash
	pub fn hash(&self) -> &B256 {
		&self.0.hash
	}
}

impl From<BaseTransaction> for Transaction {
	fn from(tx: BaseTransaction) -> Self {
		Self(tx)
	}
}

impl From<AlloyTransaction> for Transaction {
	fn from(tx: AlloyTransaction) -> Self {
		let tx = BaseTransaction {
			hash: *tx.inner.tx_hash(),
			nonce: U256::from(tx.inner.nonce()),
			block_hash: tx.block_hash,
			block_number: tx.block_number.map(U64::from),
			transaction_index: tx.transaction_index.map(|i| Index::from(i as usize)),
			from: Some(tx.inner.signer()),
			to: tx.inner.to(),
			value: tx.inner.value(),
			gas_price: tx.inner.gas_price().map(U256::from),
			gas: U256::from(tx.inner.gas_limit()),
			input: tx.inner.input().clone(),
			v: Some(U64::from(u64::from(tx.inner.signature().v()))),
			r: Some(U256::from(tx.inner.signature().r())),
			s: Some(U256::from(tx.inner.signature().s())),
			raw: None,
			transaction_type: Some(U64::from(tx.inner.tx_type() as u64)),
			access_list: tx.inner.access_list().cloned(),
			max_fee_per_gas: Some(U256::from(tx.inner.max_fee_per_gas())),
			max_priority_fee_per_gas: Some(U256::from(
				tx.inner.max_priority_fee_per_gas().unwrap_or(0),
			)),
			l2: BaseL2Transaction {
				deposit_receipt_version: None,
				source_hash: None,
				mint: None,
				y_parity: None,
			},
			extra: HashMap::new(),
		};
		Self(tx)
	}
}

impl Deref for Transaction {
	type Target = BaseTransaction;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::tests::builders::evm::transaction::TransactionBuilder;
	use alloy::primitives::{Address, B256, U256};

	#[test]
	fn test_value() {
		let value = U256::from(100);
		let tx = TransactionBuilder::new().value(value).build();
		assert_eq!(*tx.value(), value);
	}

	#[test]
	fn test_sender() {
		let address = Address::with_last_byte(5);
		let tx = TransactionBuilder::new().from(address).build();
		assert_eq!(tx.sender(), Some(&address));
	}

	#[test]
	fn test_recipient() {
		let address = Address::with_last_byte(6);
		let tx = TransactionBuilder::new().to(address).build();
		assert_eq!(tx.to(), Some(&address));
	}

	#[test]
	fn test_gas() {
		let default_tx = TransactionBuilder::new().build(); // Default gas is 21000
		assert_eq!(*default_tx.gas(), U256::from(21000));

		// Set custom gas limit
		let gas = U256::from(45000);
		let tx = TransactionBuilder::new().gas_limit(gas).build();
		assert_eq!(*tx.gas(), gas);
	}

	#[test]
	fn test_gas_price() {
		let gas_price = U256::from(20);
		let tx = TransactionBuilder::new().gas_price(gas_price).build();
		assert_eq!(tx.gas_price(), Some(&gas_price));
	}

	#[test]
	fn test_nonce() {
		let nonce = U256::from(2);
		let tx = TransactionBuilder::new().nonce(nonce).build();
		assert_eq!(*tx.nonce(), nonce);
	}

	#[test]
	fn test_hash() {
		let hash = B256::with_last_byte(1);
		let tx = TransactionBuilder::new().hash(hash).build();
		assert_eq!(*tx.hash(), hash);
	}

	#[test]
	fn test_from_base_transaction() {
		let base_tx = TransactionBuilder::new().build().0;
		let tx: Transaction = base_tx.clone().into();
		assert_eq!(tx.0, base_tx);
	}

	#[test]
	fn test_deref() {
		let base_tx = TransactionBuilder::new().build().0;
		let tx = Transaction(base_tx.clone());
		assert_eq!(*tx, base_tx);
	}
}
