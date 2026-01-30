//! Solana transaction data structures.
//!
//! Note: These structures are based on the Solana RPC implementation:
//! <https://solana.com/docs/rpc/http/gettransaction>

use serde::{Deserialize, Serialize};
use std::ops::Deref;

/// Information about a Solana transaction
///
/// This structure represents the response from the Solana RPC endpoint
/// and matches the format defined in the Solana JSON-RPC specification.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
	/// The transaction signature (base58 encoded)
	pub signature: String,

	/// The slot this transaction was processed in
	pub slot: u64,

	/// Timestamp when the block containing this transaction was produced
	pub block_time: Option<i64>,

	/// The transaction message
	pub transaction: TransactionMessage,

	/// Transaction metadata
	pub meta: Option<TransactionMeta>,
}

/// The transaction message containing accounts and instructions
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransactionMessage {
	/// List of account keys used in the transaction
	pub account_keys: Vec<String>,

	/// Recent blockhash used for the transaction
	pub recent_blockhash: String,

	/// Instructions in the transaction
	pub instructions: Vec<Instruction>,

	/// Address table lookups (for versioned transactions)
	#[serde(default)]
	pub address_table_lookups: Vec<AddressTableLookup>,
}

/// A single instruction in a Solana transaction
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
	/// Index of the program ID in the account keys array
	pub program_id_index: u8,

	/// Indexes of the accounts used by this instruction
	pub accounts: Vec<u8>,

	/// Instruction data (base58 or base64 encoded depending on encoding)
	pub data: String,

	/// Parsed instruction data (if available, only for known programs)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub parsed: Option<ParsedInstruction>,

	/// Program name (if parsed)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub program: Option<String>,

	/// Program ID (if parsed format)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub program_id: Option<String>,
}

/// Parsed instruction data for known programs
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParsedInstruction {
	/// The type of instruction
	#[serde(rename = "type")]
	pub instruction_type: String,

	/// Instruction-specific data
	pub info: serde_json::Value,
}

/// Address table lookup for versioned transactions
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddressTableLookup {
	/// The account key of the address lookup table
	pub account_key: String,

	/// Indexes of writable accounts
	pub writable_indexes: Vec<u8>,

	/// Indexes of readonly accounts
	pub readonly_indexes: Vec<u8>,
}

/// Transaction metadata including status and logs
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TransactionMeta {
	/// Error if transaction failed
	pub err: Option<serde_json::Value>,

	/// Fee paid for the transaction (in lamports)
	pub fee: u64,

	/// Account balances before the transaction
	pub pre_balances: Vec<u64>,

	/// Account balances after the transaction
	pub post_balances: Vec<u64>,

	/// Token balances before the transaction
	#[serde(default)]
	pub pre_token_balances: Vec<TokenBalance>,

	/// Token balances after the transaction
	#[serde(default)]
	pub post_token_balances: Vec<TokenBalance>,

	/// Inner instructions (cross-program invocations)
	#[serde(default)]
	pub inner_instructions: Vec<InnerInstruction>,

	/// Log messages from the transaction
	#[serde(default)]
	pub log_messages: Vec<String>,

	/// Compute units consumed
	#[serde(skip_serializing_if = "Option::is_none")]
	pub compute_units_consumed: Option<u64>,

	/// Addresses loaded from address lookup tables (for v0 transactions)
	#[serde(default)]
	pub loaded_addresses: Option<LoadedAddresses>,
}

/// Addresses loaded from address lookup tables in versioned transactions
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoadedAddresses {
	/// Writable addresses loaded from lookup tables
	#[serde(default)]
	pub writable: Vec<String>,

	/// Readonly addresses loaded from lookup tables
	#[serde(default)]
	pub readonly: Vec<String>,
}

/// Token balance information
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenBalance {
	/// Account index in the transaction
	pub account_index: u8,

	/// Token mint address
	pub mint: String,

	/// Token account owner
	#[serde(skip_serializing_if = "Option::is_none")]
	pub owner: Option<String>,

	/// Token program ID
	#[serde(skip_serializing_if = "Option::is_none")]
	pub program_id: Option<String>,

	/// UI token amount
	pub ui_token_amount: UiTokenAmount,
}

/// UI-friendly token amount
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UiTokenAmount {
	/// Token amount as a string
	pub amount: String,

	/// Number of decimals
	pub decimals: u8,

	/// UI amount as a float (may be None for very large amounts)
	pub ui_amount: Option<f64>,

	/// UI amount as a string
	pub ui_amount_string: String,
}

/// Inner instruction (cross-program invocation)
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct InnerInstruction {
	/// Index of the instruction that generated these inner instructions
	pub index: u8,

	/// The inner instructions
	pub instructions: Vec<Instruction>,
}

/// Wrapper around TransactionInfo that provides additional functionality
///
/// This type implements convenience methods for working with Solana transactions
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Transaction(pub TransactionInfo);

impl Transaction {
	/// Get the transaction signature
	pub fn signature(&self) -> &str {
		&self.0.signature
	}

	/// Get the slot number
	pub fn slot(&self) -> u64 {
		self.0.slot
	}

	/// Check if the transaction was successful
	pub fn is_success(&self) -> bool {
		self.0
			.meta
			.as_ref()
			.map(|m| m.err.is_none())
			.unwrap_or(false)
	}

	/// Get the log messages from the transaction
	pub fn logs(&self) -> &[String] {
		self.0
			.meta
			.as_ref()
			.map(|m| m.log_messages.as_slice())
			.unwrap_or(&[])
	}

	/// Get the fee paid for the transaction
	pub fn fee(&self) -> u64 {
		self.0.meta.as_ref().map(|m| m.fee).unwrap_or(0)
	}

	/// Get all program IDs invoked in this transaction
	pub fn program_ids(&self) -> Vec<String> {
		let account_keys = &self.0.transaction.account_keys;
		self.0
			.transaction
			.instructions
			.iter()
			.filter_map(|ix| {
				// First check if there's a parsed program_id
				if let Some(program_id) = &ix.program_id {
					return Some(program_id.clone());
				}
				// Otherwise, look up by index
				let idx = ix.program_id_index as usize;
				account_keys.get(idx).cloned()
			})
			.collect()
	}

	/// Get the fee payer address (first account in account_keys by Solana convention)
	/// Returns None if account_keys is empty
	pub fn fee_payer(&self) -> Option<&str> {
		self.0
			.transaction
			.account_keys
			.first()
			.filter(|s| !s.is_empty())
			.map(|s| s.as_str())
	}

	/// Get all account addresses involved in the transaction
	/// Includes both static account_keys and addresses loaded from lookup tables (ALTs)
	pub fn accounts(&self) -> Vec<String> {
		let mut accounts = self.0.transaction.account_keys.clone();

		// Include addresses loaded from address lookup tables (v0 transactions)
		if let Some(meta) = &self.0.meta {
			if let Some(loaded) = &meta.loaded_addresses {
				accounts.extend(loaded.writable.iter().cloned());
				accounts.extend(loaded.readonly.iter().cloned());
			}
		}

		accounts
	}
}

impl From<TransactionInfo> for Transaction {
	fn from(tx: TransactionInfo) -> Self {
		Self(tx)
	}
}

impl Deref for Transaction {
	type Target = TransactionInfo;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn create_test_transaction(success: bool) -> TransactionInfo {
		TransactionInfo {
			signature: "5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
				.to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: TransactionMessage {
				account_keys: vec![
					"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
					"11111111111111111111111111111111".to_string(),
				],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![Instruction {
					program_id_index: 0,
					accounts: vec![1],
					data: "3Bxs4h24hBtQy9rw".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(TransactionMeta {
				err: if success {
					None
				} else {
					Some(serde_json::json!({"InstructionError": [0, "Custom"]}))
				},
				fee: 5000,
				pre_balances: vec![1000000000, 0],
				post_balances: vec![999995000, 0],
				pre_token_balances: vec![],
				post_token_balances: vec![],
				inner_instructions: vec![],
				log_messages: vec![
					"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]".to_string(),
					"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success".to_string(),
				],
				compute_units_consumed: Some(2000),
				loaded_addresses: None,
			}),
		}
	}

	#[test]
	fn test_transaction_wrapper_methods() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		assert_eq!(
			transaction.signature(),
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
		);
		assert_eq!(transaction.slot(), 123456789);
		assert!(transaction.is_success());
		assert_eq!(transaction.fee(), 5000);
		assert_eq!(transaction.logs().len(), 2);
	}

	#[test]
	fn test_failed_transaction() {
		let tx_info = create_test_transaction(false);
		let transaction = Transaction(tx_info);

		assert!(!transaction.is_success());
	}

	#[test]
	fn test_program_ids() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		let program_ids = transaction.program_ids();
		assert_eq!(program_ids.len(), 1);
		assert_eq!(
			program_ids[0],
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		);
	}

	#[test]
	fn test_transaction_from_info() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction::from(tx_info);

		assert_eq!(
			transaction.signature(),
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
		);
	}

	#[test]
	fn test_transaction_deref() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		// Test that we can access TransactionInfo fields through deref
		assert_eq!(
			transaction.signature,
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
		);
		assert_eq!(transaction.slot, 123456789);
	}

	#[test]
	fn test_default_implementation() {
		let transaction = Transaction::default();

		assert_eq!(transaction.signature(), "");
		assert_eq!(transaction.slot(), 0);
		assert!(!transaction.is_success());
		assert_eq!(transaction.fee(), 0);
		assert!(transaction.logs().is_empty());
		assert!(transaction.fee_payer().is_none());
		assert!(transaction.accounts().is_empty());
	}

	#[test]
	fn test_fee_payer() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		// First account key is the fee payer by Solana convention
		let fee_payer = transaction.fee_payer();
		assert!(fee_payer.is_some());
		assert_eq!(
			fee_payer.unwrap(),
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		);
	}

	#[test]
	fn test_accounts() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		let accounts = transaction.accounts();
		assert_eq!(accounts.len(), 2);
		assert_eq!(accounts[0], "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
		assert_eq!(accounts[1], "11111111111111111111111111111111");
	}

	#[test]
	fn test_accounts_with_loaded_addresses() {
		let mut tx_info = create_test_transaction(true);

		// Add loaded addresses from address lookup tables (ALTs)
		if let Some(ref mut meta) = tx_info.meta {
			meta.loaded_addresses = Some(LoadedAddresses {
				writable: vec!["WritableALTAddress111111111111111111".to_string()],
				readonly: vec!["ReadonlyALTAddress111111111111111111".to_string()],
			});
		}

		let transaction = Transaction(tx_info);
		let accounts = transaction.accounts();

		// Should include both static account_keys and loaded addresses
		assert_eq!(accounts.len(), 4);
		assert_eq!(accounts[0], "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
		assert_eq!(accounts[1], "11111111111111111111111111111111");
		assert_eq!(accounts[2], "WritableALTAddress111111111111111111");
		assert_eq!(accounts[3], "ReadonlyALTAddress111111111111111111");
	}

	#[test]
	fn test_serde_serialization() {
		let tx_info = create_test_transaction(true);
		let transaction = Transaction(tx_info);

		// Test serialization
		let serialized = serde_json::to_string(&transaction).unwrap();

		// Test deserialization
		let deserialized: Transaction = serde_json::from_str(&serialized).unwrap();

		assert_eq!(
			deserialized.signature(),
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
		);
		assert_eq!(deserialized.slot(), 123456789);
		assert!(deserialized.is_success());
	}

	#[test]
	fn test_parsed_instruction() {
		let parsed = ParsedInstruction {
			instruction_type: "transfer".to_string(),
			info: serde_json::json!({
				"source": "ABC123",
				"destination": "DEF456",
				"amount": "1000000"
			}),
		};

		assert_eq!(parsed.instruction_type, "transfer");
		assert_eq!(parsed.info["amount"], "1000000");
	}
}
