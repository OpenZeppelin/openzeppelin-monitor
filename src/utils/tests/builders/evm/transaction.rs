use crate::models::{EVMBaseTransaction, EVMTransaction};
use alloy::{
	primitives::{Address, Bytes, B256, U256},
	rpc::types::Index,
};

/// A builder for creating test EVM transactions with default values.
#[derive(Debug, Default)]
pub struct TransactionBuilder {
	hash: Option<B256>,
	from: Option<Address>,
	to: Option<Address>,
	value: Option<U256>,
	input: Option<Bytes>,
	gas_price: Option<U256>,
	max_fee_per_gas: Option<U256>,
	max_priority_fee_per_gas: Option<U256>,
	gas_limit: Option<U256>,
	nonce: Option<U256>,
	transaction_index: Option<Index>,
}

impl TransactionBuilder {
	/// Creates a new TransactionBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the hash of the transaction.
	pub fn hash(mut self, hash: B256) -> Self {
		self.hash = Some(hash);
		self
	}

	/// Sets the sender address of the transaction.
	pub fn from(mut self, from: Address) -> Self {
		self.from = Some(from);
		self
	}

	/// Sets the recipient address of the transaction.
	pub fn to(mut self, to: Address) -> Self {
		self.to = Some(to);
		self
	}

	/// Sets the transaction value (amount sent).
	pub fn value(mut self, value: U256) -> Self {
		self.value = Some(value);
		self
	}

	/// Sets the transaction input data.
	pub fn input(mut self, input: Bytes) -> Self {
		self.input = Some(input);
		self
	}

	/// Sets the gas price for legacy transactions.
	pub fn gas_price(mut self, gas_price: U256) -> Self {
		self.gas_price = Some(gas_price);
		self
	}

	/// Sets the max fee per gas for EIP-1559 transactions.
	pub fn max_fee_per_gas(mut self, max_fee_per_gas: U256) -> Self {
		self.max_fee_per_gas = Some(max_fee_per_gas);
		self
	}

	/// Sets the max priority fee per gas for EIP-1559 transactions.
	pub fn max_priority_fee_per_gas(mut self, max_priority_fee_per_gas: U256) -> Self {
		self.max_priority_fee_per_gas = Some(max_priority_fee_per_gas);
		self
	}

	/// Sets the gas limit for the transaction.
	pub fn gas_limit(mut self, gas_limit: U256) -> Self {
		self.gas_limit = Some(gas_limit);
		self
	}

	/// Sets the nonce for the transaction.
	pub fn nonce(mut self, nonce: U256) -> Self {
		self.nonce = Some(nonce);
		self
	}

	/// Sets the transaction index for the transaction.
	pub fn transaction_index(mut self, transaction_index: usize) -> Self {
		self.transaction_index = Some(Index(transaction_index));
		self
	}

	/// Builds the Transaction instance.
	pub fn build(self) -> EVMTransaction {
		let default_gas_limit = U256::from(21000);

		let base_tx = EVMBaseTransaction {
			hash: self.hash.unwrap_or_default(),
			from: self.from,
			to: self.to,
			gas_price: self.gas_price,
			max_fee_per_gas: self.max_fee_per_gas,
			max_priority_fee_per_gas: self.max_priority_fee_per_gas,
			gas: self.gas_limit.unwrap_or(default_gas_limit),
			nonce: self.nonce.unwrap_or_default(),
			value: self.value.unwrap_or_default(),
			input: self.input.unwrap_or_default(),
			transaction_index: self.transaction_index,
			..Default::default()
		};

		EVMTransaction(base_tx)
	}
}
