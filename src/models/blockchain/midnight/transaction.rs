//! Midnight transaction data structures.
//!
//! This module provides data structures and implementations for working with Midnight blockchain transactions.
//! It includes types for representing RPC transactions, operations, and transaction processing.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use alloy::hex::ToHexExt;
use midnight_ledger::structure::{
	ContractAction, Proof, Proofish, Transaction as MidnightNodeTransaction,
};

use midnight_node_ledger_helpers::DB;

use serde::{Deserialize, Serialize};
use std::ops::Deref;

use crate::{
	models::{ChainConfiguration, SecretValue},
	services::filter::midnight_helpers::process_transaction_for_coins,
};

/// Represents a Midnight RPC transaction Enum
///
/// This enum represents different types of transactions that can be received from the Midnight RPC.
/// It includes standard Midnight transactions, malformed transactions, timestamps, and runtime upgrades.
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L200-L211>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RpcTransaction {
	/// A standard Midnight transaction with raw data and parsed transaction information
	MidnightTransaction {
		/// Raw transaction data (serialized as string)
		#[serde(skip)]
		tx_raw: String,
		/// Parsed transaction information
		tx: MidnightRpcTransaction,
	},
	/// A transaction that could not be properly parsed
	MalformedMidnightTransaction,
	/// A timestamp transaction
	Timestamp(u64),
	/// A runtime upgrade transaction
	RuntimeUpgrade,
	/// An unknown transaction type
	UnknownTransaction,
}

/// Represents a Midnight transaction operations
///
/// This enum defines the various operations that can be performed in a Midnight transaction,
/// including contract calls, deployments, coin operations, and maintenance actions.
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L185-L192>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Operation {
	/// A contract call operation with target address and entry point
	Call {
		/// The contract address to call
		address: String,
		/// The entry point to call (hex-encoded)
		entry_point: String,
	},
	/// A contract deployment operation
	Deploy {
		/// The address where the contract is deployed
		address: String,
	},
	/// A fallible coin operation
	FallibleCoins,
	/// A guaranteed coin operation
	GuaranteedCoins,
	/// A contract maintenance operation
	Maintain {
		/// The contract address to maintain
		address: String,
	},
	/// A claim mint operation
	ClaimMint {
		/// The value to mint
		value: u128,
		/// The type of coin to mint
		coin_type: String,
	},
}

/// Represents a Midnight transaction
///
/// This struct contains the core information about a Midnight transaction,
/// including its hash, operations, and identifiers.
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L194-L198>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MidnightRpcTransaction {
	/// The transaction hash
	pub tx_hash: String,
	/// The list of operations in the transaction
	pub operations: Vec<Operation>,
	/// The list of identifiers associated with the transaction
	pub identifiers: Vec<String>,
}

/// Wrapper around MidnightRpcTransaction that provides additional functionality
///
/// This type implements convenience methods for working with Midnight transactions
/// while maintaining compatibility with the RPC response format. It provides methods
/// for accessing transaction details and processing transaction data.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction(pub MidnightRpcTransaction);

impl Transaction {
	/// Get the transaction hash
	///
	/// # Returns
	/// A reference to the transaction hash string
	pub fn hash(&self) -> &String {
		&self.0.tx_hash
	}

	/// Get the contract addresses involved in the transaction
	///
	/// This method extracts all contract addresses from Call, Deploy, and Maintain operations.
	///
	/// # Returns
	/// A vector of contract addresses as strings
	pub fn contract_addresses(&self) -> Vec<String> {
		self.0
			.operations
			.iter()
			.filter_map(|op| match op {
				Operation::Call { address, .. } => Some(address.clone()),
				Operation::Deploy { address, .. } => Some(address.clone()),
				Operation::Maintain { address, .. } => Some(address.clone()),
				_ => None,
			})
			.collect()
	}

	/// Get the contract entry points called in the transaction
	///
	/// This method extracts and decodes entry points from Call operations.
	/// Entry points are decoded from hex to UTF-8 strings.
	///
	/// # Returns
	/// A vector of decoded entry point strings
	pub fn entry_points(&self) -> Vec<String> {
		self.0
			.operations
			.iter()
			.filter_map(|op| match op {
				Operation::Call { entry_point, .. } => Some(
					// Decode the entry point from hex to utf8
					String::from_utf8(hex::decode(entry_point.clone()).unwrap_or_default())
						.unwrap_or_default(),
				),
				_ => None,
			})
			.collect()
	}

	/// Get the contract addresses and their corresponding entry points
	///
	/// This method pairs contract addresses with their entry points for Call operations.
	/// Entry points are decoded from hex to UTF-8 strings.
	///
	/// # Returns
	/// A vector of (address, entry_point) pairs
	pub fn contract_addresses_and_entry_points(&self) -> Vec<(String, String)> {
		self.0
			.operations
			.iter()
			.map(|op| match op {
				Operation::Call {
					address,
					entry_point,
					..
				} => (
					address.clone(),
					// Decode the entry point from hex to utf8
					String::from_utf8(hex::decode(entry_point.clone()).unwrap_or_default())
						.unwrap_or_default(),
				),
				Operation::Deploy { address, .. } => (address.clone(), "".to_string()),
				Operation::Maintain { address, .. } => (address.clone(), "".to_string()),
				_ => ("".to_string(), "".to_string()),
			})
			.filter(|(addr, entry)| !addr.is_empty() && !entry.is_empty())
			.collect()
	}
}

impl From<MidnightRpcTransaction> for Transaction {
	/// Creates a new Transaction from a MidnightRpcTransaction
	///
	/// # Arguments
	/// * `tx` - The MidnightRpcTransaction to convert
	///
	/// # Returns
	/// A new Transaction instance
	fn from(tx: MidnightRpcTransaction) -> Self {
		Self(tx)
	}
}

impl From<Transaction> for MidnightRpcTransaction {
	/// Converts a Transaction back into a MidnightRpcTransaction
	///
	/// # Arguments
	/// * `tx` - The Transaction to convert
	///
	/// # Returns
	/// The underlying MidnightRpcTransaction
	fn from(tx: Transaction) -> Self {
		tx.0
	}
}

impl<P: Proofish<D>, D: DB> From<ContractAction<P, D>> for Operation {
	/// Converts a ContractAction into an Operation
	///
	/// This implementation handles the conversion of different types of contract actions
	/// into their corresponding Operation variants, including proper encoding of addresses
	/// and entry points.
	///
	/// # Arguments
	/// * `action` - The ContractAction to convert
	///
	/// # Returns
	/// The corresponding Operation variant
	fn from(action: ContractAction<P, D>) -> Self {
		match action {
			ContractAction::Call(call) => Operation::Call {
				address: call.address.0 .0.encode_hex(),
				entry_point: String::from_utf8_lossy(&call.entry_point.0).to_string(),
			},
			ContractAction::Deploy(deploy) => Operation::Deploy {
				address: deploy.address().0 .0.encode_hex(),
			},
			ContractAction::Maintain(update) => Operation::Maintain {
				address: update.address.0 .0.encode_hex(),
			},
		}
	}
}

impl<D: DB>
	TryFrom<(
		Transaction,
		Option<MidnightNodeTransaction<Proof, D>>,
		&Vec<ChainConfiguration>,
	)> for Transaction
{
	type Error = anyhow::Error;

	/// Attempts to create a Transaction from a tuple of transaction data and chain configuration
	///
	/// This implementation processes the transaction data and attempts to decrypt any coins
	/// using the provided chain configuration's viewing keys.
	///
	/// # Arguments
	/// * `(block_tx, ledger_tx, chain_configurations)` - A tuple containing:
	///   - The block transaction
	///   - An optional ledger transaction
	///   - A reference to chain configurations
	///
	/// # Returns
	/// * `Result<Self, Self::Error>` - The processed transaction or an error
	fn try_from(
		(block_tx, ledger_tx, chain_configurations): (
			Transaction,
			Option<MidnightNodeTransaction<Proof, D>>,
			&Vec<ChainConfiguration>,
		),
	) -> Result<Self, Self::Error> {
		// Check if chain_configuration has viewing keys and decrypt the transaction's coins
		for chain_configuration in chain_configurations {
			if let Some(midnight) = &chain_configuration.midnight {
				for viewing_key in &midnight.viewing_keys {
					if let SecretValue::Plain(secret) = viewing_key {
						let viewing_key_str = secret.as_str();
						if let Some(ref ledger_tx) = ledger_tx {
							// TODO: Do something with the coins...
							let _ = process_transaction_for_coins::<D>(viewing_key_str, ledger_tx);
						}
					}
				}
			}
		}

		Ok(block_tx)
	}
}

impl Deref for Transaction {
	type Target = MidnightRpcTransaction;

	/// Dereferences the Transaction to access the underlying MidnightRpcTransaction
	///
	/// # Returns
	/// A reference to the underlying MidnightRpcTransaction
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests the conversion from MidnightRpcTransaction to Transaction
	#[test]
	fn test_transaction_from_rpc_transaction() {
		let tx_info = MidnightRpcTransaction {
			tx_hash: "test_hash".to_string(),
			operations: vec![Operation::Call {
				address: "0x1234567890abcdef".to_string(),
				entry_point: "0x1234567890abcdef".to_string(),
			}],
			identifiers: vec!["0x1234567890abcdef".to_string()],
		};

		let transaction = Transaction::from(tx_info);

		// Verify the transaction was created
		assert_eq!(transaction.hash(), "test_hash");
		assert_eq!(
			transaction.operations,
			vec![Operation::Call {
				address: "0x1234567890abcdef".to_string(),
				entry_point: "0x1234567890abcdef".to_string(),
			}]
		);
		assert_eq!(
			transaction.identifiers,
			vec!["0x1234567890abcdef".to_string()]
		);
	}

	/// Tests the Deref implementation for Transaction
	#[test]
	fn test_transaction_deref() {
		let tx_info = MidnightRpcTransaction {
			tx_hash: "test_hash".to_string(),
			operations: vec![Operation::Call {
				address: "0x1234567890abcdef".to_string(),
				entry_point: "0x1234567890abcdef".to_string(),
			}],
			identifiers: vec!["0x1234567890abcdef".to_string()],
		};

		let transaction = Transaction::from(tx_info);

		// Test that we can access MidnightRpcTransaction fields through deref
		assert_eq!(transaction.tx_hash, "test_hash");
		assert_eq!(
			transaction.operations,
			vec![Operation::Call {
				address: "0x1234567890abcdef".to_string(),
				entry_point: "0x1234567890abcdef".to_string(),
			}]
		);
		assert_eq!(
			transaction.identifiers,
			vec!["0x1234567890abcdef".to_string()]
		);
	}

	/// Tests the contract_addresses method
	#[test]
	fn test_contract_addresses() {
		let tx_info = MidnightRpcTransaction {
			tx_hash: "test_hash".to_string(),
			operations: vec![
				Operation::Call {
					address: "0x123".to_string(),
					entry_point: "656E74727931".to_string(),
				},
				Operation::Deploy {
					address: "0x456".to_string(),
				},
				Operation::Maintain {
					address: "0x789".to_string(),
				},
				Operation::GuaranteedCoins,
			],
			identifiers: vec![],
		};

		let transaction = Transaction::from(tx_info);
		let addresses = transaction.contract_addresses();

		assert_eq!(addresses.len(), 3);
		assert!(addresses.contains(&"0x123".to_string()));
		assert!(addresses.contains(&"0x456".to_string()));
		assert!(addresses.contains(&"0x789".to_string()));
	}

	/// Tests the entry_points method
	#[test]
	fn test_entry_points() {
		let tx_info = MidnightRpcTransaction {
			tx_hash: "test_hash".to_string(),
			operations: vec![
				Operation::Call {
					address: "0x123".to_string(),
					entry_point: "656E74727931".to_string(),
				},
				Operation::Call {
					address: "0x456".to_string(),
					entry_point: "656E74727932".to_string(),
				},
				Operation::Deploy {
					address: "0x789".to_string(),
				},
			],
			identifiers: vec![],
		};

		let transaction = Transaction::from(tx_info);
		let entry_points = transaction.entry_points();

		assert_eq!(entry_points.len(), 2);
		assert!(entry_points.contains(&"entry1".to_string()));
		assert!(entry_points.contains(&"entry2".to_string()));
	}

	/// Tests the contract_addresses_and_entry_points method
	#[test]
	fn test_contract_addresses_and_entry_points() {
		let tx_info = MidnightRpcTransaction {
			tx_hash: "test_hash".to_string(),
			operations: vec![
				Operation::Call {
					address: "0x123".to_string(),
					entry_point: "656E74727931".to_string(),
				},
				Operation::Call {
					address: "0x456".to_string(),
					entry_point: "656E74727932".to_string(),
				},
				Operation::Deploy {
					address: "0x789".to_string(),
				},
				Operation::GuaranteedCoins,
			],
			identifiers: vec![],
		};

		let transaction = Transaction::from(tx_info);

		let pairs = transaction.contract_addresses_and_entry_points();

		assert_eq!(pairs.len(), 2);
		assert!(pairs.contains(&("0x123".to_string(), "entry1".to_string())));
		assert!(pairs.contains(&("0x456".to_string(), "entry2".to_string())));
	}
}
