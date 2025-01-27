//! EVM transaction data structures.

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use web3::types::{Transaction as Web3Transaction, H160, H256, U256};

/// Wrapper around Web3 Transaction that implements additional functionality
///
/// This type provides a convenient interface for working with EVM transactions
/// while maintaining compatibility with the web3 types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction(pub Web3Transaction);

impl Transaction {
	/// Get the transaction value (amount of ETH transferred)
	pub fn value(&self) -> &U256 {
		&self.0.value
	}

	/// Get the transaction sender address
	pub fn sender(&self) -> Option<&H160> {
		self.0.from.as_ref()
	}

	/// Get the transaction recipient address (None for contract creation)
	pub fn to(&self) -> Option<&H160> {
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
	pub fn hash(&self) -> &H256 {
		&self.0.hash
	}
}

impl From<Web3Transaction> for Transaction {
	fn from(tx: Web3Transaction) -> Self {
		Self(tx)
	}
}

impl Deref for Transaction {
	type Target = Web3Transaction;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use web3::types::{Bytes, U64};

	fn create_test_transaction() -> Web3Transaction {
		Web3Transaction {
			hash: H256::from_low_u64_be(1),
			nonce: U256::from(2),
			block_hash: Some(H256::from_low_u64_be(3)),
			block_number: Some(U64::from(4)),
			transaction_index: Some(U64::from(0)),
			from: Some(H160::from_low_u64_be(5)),
			to: Some(H160::from_low_u64_be(6)),
			value: U256::from(100),
			gas_price: Some(U256::from(20)),
			gas: U256::from(21000),
			input: Bytes::default(),
			v: None,
			r: None,
			s: None,
			raw: None,
			transaction_type: None,
			access_list: None,
			max_priority_fee_per_gas: None,
			max_fee_per_gas: None,
		}
	}

	#[test]
	fn test_value() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(*tx.value(), U256::from(100));
	}

	#[test]
	fn test_sender() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(tx.sender(), Some(&H160::from_low_u64_be(5)));
	}

	#[test]
	fn test_recipient() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(tx.to(), Some(&H160::from_low_u64_be(6)));
	}

	#[test]
	fn test_gas() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(*tx.gas(), U256::from(21000));
	}

	#[test]
	fn test_gas_price() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(tx.gas_price(), Some(&U256::from(20)));
	}

	#[test]
	fn test_nonce() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(*tx.nonce(), U256::from(2));
	}

	#[test]
	fn test_hash() {
		let tx = Transaction(create_test_transaction());
		assert_eq!(*tx.hash(), H256::from_low_u64_be(1));
	}

	#[test]
	fn test_from_web3_transaction() {
		let web3_tx = create_test_transaction();
		let tx: Transaction = web3_tx.clone().into();
		assert_eq!(tx.0, web3_tx);
	}

	#[test]
	fn test_deref() {
		let web3_tx = create_test_transaction();
		let tx = Transaction(web3_tx.clone());
		assert_eq!(*tx, web3_tx);
	}
}
