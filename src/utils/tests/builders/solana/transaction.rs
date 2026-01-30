//! Test helper utilities for Solana Transaction
//!
//! - `TransactionBuilder`: Builder for creating test SolanaTransaction instances

use crate::models::{
	SolanaInstruction, SolanaParsedInstruction, SolanaTransaction, SolanaTransactionInfo,
	SolanaTransactionMessage, SolanaTransactionMeta,
};

/// A builder for creating test Solana transactions with default values.
#[derive(Debug, Default)]
pub struct TransactionBuilder {
	signature: Option<String>,
	slot: Option<u64>,
	block_time: Option<i64>,
	account_keys: Option<Vec<String>>,
	recent_blockhash: Option<String>,
	instructions: Option<Vec<SolanaInstruction>>,
	fee: Option<u64>,
	err: Option<serde_json::Value>,
	pre_balances: Option<Vec<u64>>,
	post_balances: Option<Vec<u64>>,
	log_messages: Option<Vec<String>>,
	compute_units_consumed: Option<u64>,
}

impl TransactionBuilder {
	/// Creates a new TransactionBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the transaction signature.
	pub fn signature(mut self, signature: &str) -> Self {
		self.signature = Some(signature.to_string());
		self
	}

	/// Sets the slot number.
	pub fn slot(mut self, slot: u64) -> Self {
		self.slot = Some(slot);
		self
	}

	/// Sets the block time.
	pub fn block_time(mut self, block_time: i64) -> Self {
		self.block_time = Some(block_time);
		self
	}

	/// Sets the account keys.
	pub fn account_keys(mut self, account_keys: Vec<String>) -> Self {
		self.account_keys = Some(account_keys);
		self
	}

	/// Sets the recent blockhash.
	pub fn recent_blockhash(mut self, recent_blockhash: &str) -> Self {
		self.recent_blockhash = Some(recent_blockhash.to_string());
		self
	}

	/// Sets the instructions.
	pub fn instructions(mut self, instructions: Vec<SolanaInstruction>) -> Self {
		self.instructions = Some(instructions);
		self
	}

	/// Adds a single instruction.
	pub fn add_instruction(mut self, instruction: SolanaInstruction) -> Self {
		let mut instructions = self.instructions.unwrap_or_default();
		instructions.push(instruction);
		self.instructions = Some(instructions);
		self
	}

	/// Adds a parsed instruction with program ID and instruction type.
	pub fn add_parsed_instruction(
		mut self,
		program_id: &str,
		instruction_type: &str,
		info: serde_json::Value,
	) -> Self {
		let instruction = SolanaInstruction {
			program_id_index: 0,
			accounts: vec![],
			data: String::new(),
			parsed: Some(SolanaParsedInstruction {
				instruction_type: instruction_type.to_string(),
				info,
			}),
			program: Some(program_id.to_string()),
			program_id: Some(program_id.to_string()),
		};
		let mut instructions = self.instructions.unwrap_or_default();
		instructions.push(instruction);
		self.instructions = Some(instructions);
		self
	}

	/// Sets the transaction fee.
	pub fn fee(mut self, fee: u64) -> Self {
		self.fee = Some(fee);
		self
	}

	/// Sets the transaction error.
	pub fn err(mut self, err: serde_json::Value) -> Self {
		self.err = Some(err);
		self
	}

	/// Sets the pre-transaction balances.
	pub fn pre_balances(mut self, pre_balances: Vec<u64>) -> Self {
		self.pre_balances = Some(pre_balances);
		self
	}

	/// Sets the post-transaction balances.
	pub fn post_balances(mut self, post_balances: Vec<u64>) -> Self {
		self.post_balances = Some(post_balances);
		self
	}

	/// Sets the log messages.
	pub fn log_messages(mut self, log_messages: Vec<String>) -> Self {
		self.log_messages = Some(log_messages);
		self
	}

	/// Adds a log message.
	pub fn add_log_message(mut self, log_message: &str) -> Self {
		let mut log_messages = self.log_messages.unwrap_or_default();
		log_messages.push(log_message.to_string());
		self.log_messages = Some(log_messages);
		self
	}

	/// Sets the compute units consumed.
	pub fn compute_units_consumed(mut self, compute_units: u64) -> Self {
		self.compute_units_consumed = Some(compute_units);
		self
	}

	/// Builds the SolanaTransaction instance.
	pub fn build(self) -> SolanaTransaction {
		let transaction_info = SolanaTransactionInfo {
			signature: self.signature.unwrap_or_else(|| {
				"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
					.to_string()
			}),
			slot: self.slot.unwrap_or(123456789),
			block_time: self.block_time,
			transaction: SolanaTransactionMessage {
				account_keys: self.account_keys.unwrap_or_else(|| {
					vec![
						"11111111111111111111111111111111".to_string(),
						"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
					]
				}),
				recent_blockhash: self
					.recent_blockhash
					.unwrap_or_else(|| "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string()),
				instructions: self.instructions.unwrap_or_default(),
				address_table_lookups: vec![],
			},
			meta: Some(SolanaTransactionMeta {
				err: self.err,
				fee: self.fee.unwrap_or(5000),
				pre_balances: self.pre_balances.unwrap_or_else(|| vec![1000000, 500000]),
				post_balances: self.post_balances.unwrap_or_else(|| vec![995000, 500000]),
				pre_token_balances: vec![],
				post_token_balances: vec![],
				inner_instructions: vec![],
				log_messages: self.log_messages.unwrap_or_default(),
				compute_units_consumed: self.compute_units_consumed,
				loaded_addresses: None,
			}),
		};

		SolanaTransaction::from(transaction_info)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_transaction() {
		let tx = TransactionBuilder::new().build();

		assert_eq!(
			tx.signature(),
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
		);
		assert_eq!(tx.slot(), 123456789);
	}

	#[test]
	fn test_custom_transaction() {
		let tx = TransactionBuilder::new()
			.signature("custom_signature")
			.slot(999)
			.block_time(1234567890)
			.fee(10000)
			.build();

		assert_eq!(tx.signature(), "custom_signature");
		assert_eq!(tx.slot(), 999);
	}

	#[test]
	fn test_transaction_with_instructions() {
		let tx = TransactionBuilder::new()
			.add_parsed_instruction(
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
				"transfer",
				serde_json::json!({
					"source": "source_account",
					"destination": "dest_account",
					"amount": "1000000"
				}),
			)
			.build();

		assert!(!tx.transaction.instructions.is_empty());
	}

	#[test]
	fn test_transaction_with_logs() {
		let tx = TransactionBuilder::new()
			.log_messages(vec![
				"Program log: Instruction: Transfer".to_string(),
				"Program log: Success".to_string(),
			])
			.build();

		let meta = tx.meta.as_ref().unwrap();
		assert_eq!(meta.log_messages.len(), 2);
	}
}
