//! Midnight transaction data structures.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use alloy::hex::ToHexExt;
use midnight_ledger::{
	storage::db::DB,
	structure::{ContractAction, Proofish, Transaction as MidnightTx, TransactionIdentifier},
};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

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
pub struct Transaction {
	#[serde(flatten)]
	pub inner: MidnightRpcTransaction,
	// Status of the transaction (checks for existence of fallible_transcript, guaranteed_transcript, etc)
	pub status: bool,
}

impl Transaction {
	/// Get the transaction hash
	pub fn hash(&self) -> &String {
		&self.inner.tx_hash
	}

	pub fn status(&self) -> bool {
		self.status
	}
}

impl From<MidnightRpcTransaction> for Transaction {
	fn from(tx: MidnightRpcTransaction) -> Self {
		Self {
			inner: tx,
			status: true,
		}
	}
}

impl<P: Proofish<D>, D: DB> From<ContractAction<P, D>> for Operation {
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

impl<P: Proofish<D>, D: DB> From<MidnightTx<P, D>> for Transaction {
	fn from(tx: MidnightTx<P, D>) -> Self {
		// Get hash and identifiers before moving tx
		// TODO: Implement this correctly
		let tx_hash = "0x0".to_string(); // &tx.transaction_hash().0 .0.encode_hex();

		let identifiers = tx
			.identifiers()
			.map(|id| match id {
				TransactionIdentifier::Merged(pedersen) => pedersen.0.to_string(),
				TransactionIdentifier::Unique(hash) => hash.0.encode_hex(),
			})
			.collect();

		let operations = match tx {
			MidnightTx::Standard(stx) => {
				let mut ops = Vec::new();
				// Add guaranteed coins operation
				ops.push(Operation::GuaranteedCoins);

				// Add fallible coins operation if present
				if stx.fallible_coins.is_some() {
					ops.push(Operation::FallibleCoins);
				}

				// Add contract calls if present
				if let Some(calls) = &stx.contract_calls {
					ops.extend(calls.calls.iter().map(|call| Operation::from(call.clone())));
				}
				ops
			}
			MidnightTx::ClaimMint(mtx) => {
				vec![Operation::ClaimMint {
					value: mtx.mint.coin.value,
					coin_type: mtx.mint.coin.type_.0 .0.encode_hex(),
				}]
			}
		};

		Self {
			inner: MidnightRpcTransaction {
				tx_hash,
				operations,
				identifiers,
			},
			status: true,
		}
	}
}

impl Deref for Transaction {
	type Target = MidnightRpcTransaction;

	fn deref(&self) -> &Self::Target {
		&self.inner
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
}
