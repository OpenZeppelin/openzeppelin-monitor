//! Test helper utilities for Solana Block
//!
//! - `BlockBuilder`: Builder for creating test SolanaBlock instances

use crate::models::{SolanaBlock, SolanaConfirmedBlock, SolanaTransaction};

/// A builder for creating test Solana blocks with default values.
#[derive(Debug, Default)]
pub struct BlockBuilder {
	slot: Option<u64>,
	blockhash: Option<String>,
	previous_blockhash: Option<String>,
	parent_slot: Option<u64>,
	block_time: Option<i64>,
	block_height: Option<u64>,
	transactions: Option<Vec<SolanaTransaction>>,
}

impl BlockBuilder {
	/// Creates a new BlockBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the slot number.
	pub fn slot(mut self, slot: u64) -> Self {
		self.slot = Some(slot);
		self
	}

	/// Sets the blockhash.
	pub fn blockhash(mut self, blockhash: &str) -> Self {
		self.blockhash = Some(blockhash.to_string());
		self
	}

	/// Sets the previous blockhash.
	pub fn previous_blockhash(mut self, previous_blockhash: &str) -> Self {
		self.previous_blockhash = Some(previous_blockhash.to_string());
		self
	}

	/// Sets the parent slot.
	pub fn parent_slot(mut self, parent_slot: u64) -> Self {
		self.parent_slot = Some(parent_slot);
		self
	}

	/// Sets the block time.
	pub fn block_time(mut self, block_time: i64) -> Self {
		self.block_time = Some(block_time);
		self
	}

	/// Sets the block height.
	pub fn block_height(mut self, block_height: u64) -> Self {
		self.block_height = Some(block_height);
		self
	}

	/// Sets the transactions.
	pub fn transactions(mut self, transactions: Vec<SolanaTransaction>) -> Self {
		self.transactions = Some(transactions);
		self
	}

	/// Adds a single transaction.
	pub fn add_transaction(mut self, transaction: SolanaTransaction) -> Self {
		let mut transactions = self.transactions.unwrap_or_default();
		transactions.push(transaction);
		self.transactions = Some(transactions);
		self
	}

	/// Builds the SolanaBlock instance.
	pub fn build(self) -> SolanaBlock {
		let slot = self.slot.unwrap_or(123456789);
		let confirmed_block = SolanaConfirmedBlock {
			slot,
			blockhash: self
				.blockhash
				.unwrap_or_else(|| "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string()),
			previous_blockhash: self
				.previous_blockhash
				.unwrap_or_else(|| "3sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string()),
			parent_slot: self.parent_slot.unwrap_or(slot.saturating_sub(1)),
			block_time: self.block_time,
			block_height: self.block_height,
			transactions: self.transactions.unwrap_or_default(),
		};

		SolanaBlock::from(confirmed_block)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::tests::builders::solana::transaction::TransactionBuilder;

	#[test]
	fn test_default_block() {
		let block = BlockBuilder::new().build();

		assert_eq!(block.number(), Some(123456789));
		assert_eq!(
			block.blockhash(),
			"4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn"
		);
	}

	#[test]
	fn test_custom_block() {
		let block = BlockBuilder::new()
			.slot(999)
			.blockhash("custom_blockhash")
			.block_time(1234567890)
			.block_height(100)
			.build();

		assert_eq!(block.number(), Some(999));
		assert_eq!(block.blockhash(), "custom_blockhash");
		assert_eq!(block.block_time(), Some(1234567890));
	}

	#[test]
	fn test_block_with_transactions() {
		let tx = TransactionBuilder::new().signature("test_sig").build();

		let block = BlockBuilder::new().add_transaction(tx).build();

		assert_eq!(block.transactions.len(), 1);
	}

	#[test]
	fn test_block_with_multiple_transactions() {
		let tx1 = TransactionBuilder::new().signature("sig1").build();
		let tx2 = TransactionBuilder::new().signature("sig2").build();

		let block = BlockBuilder::new().transactions(vec![tx1, tx2]).build();

		assert_eq!(block.transactions.len(), 2);
	}
}
