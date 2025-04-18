//! Midnight transaction data structures.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use std::ops::Deref;

use serde::{Deserialize, Serialize};

/// Represents a Midnight RPC transaction Enum
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L200-L211>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RpcTransaction {
	MidnightTransaction {
		#[serde(skip)]
		tx_raw: String,
		tx: MidnightRpcTransaction,
	},
	MalformedMidnightTransaction,
	Timestamp(u64),
	RuntimeUpgrade,
	UnknownTransaction,
}

/// Represents a Midnight transaction operations
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L185-L192>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Operation {
	Call {
		address: String,
		entry_point: String,
	},
	Deploy {
		address: String,
	},
	FallibleCoins,
	GuaranteedCoins,
	Maintain {
		address: String,
	},
	ClaimMint {
		value: u128,
		coin_type: String,
	},
}

/// Represents a Midnight transaction
///
/// <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs#L194-L198>
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MidnightRpcTransaction {
	pub tx_hash: String,
	pub operations: Vec<Operation>,
	pub identifiers: Vec<String>,
}

/// Wrapper around MidnightRpcTransaction that provides additional functionality
///
/// This type implements convenience methods for working with Midnight transactions
/// while maintaining compatibility with the RPC response format.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Transaction(pub MidnightRpcTransaction);

impl Transaction {
	/// Get the transaction hash
	pub fn hash(&self) -> &String {
		&self.0.tx_hash
	}
}

impl From<MidnightRpcTransaction> for Transaction {
	fn from(tx: MidnightRpcTransaction) -> Self {
		Self(tx)
	}
}

impl Deref for Transaction {
	type Target = MidnightRpcTransaction;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
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

		let transaction = Transaction(tx_info);

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
}
