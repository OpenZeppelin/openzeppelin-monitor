//! Midnight transaction data structures.
//!
//! Note: These structures are based on the Midnight RPC implementation:
//! <https://github.com/midnightntwrk/midnight-node/blob/39dbdf54afc5f0be7e7913b387637ac52d0c50f2/pallets/midnight/rpc/src/lib.rs>

use alloy::hex::ToHexExt;
use midnight_ledger::structure::{
	ContractAction, Proof, Proofish, Transaction as MidnightNodeTransaction, TransactionIdentifier,
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

	/// Get the status of the transaction
	pub fn status(&self) -> bool {
		self.status
	}

	/// Get the contract addresses of the transaction
	pub fn contract_addresses(&self) -> Vec<String> {
		self.inner
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

	/// Get the contract entry points of the transaction
	pub fn entry_points(&self) -> Vec<String> {
		self.inner
			.operations
			.iter()
			.filter_map(|op| match op {
				Operation::Call { entry_point, .. } => Some(entry_point.clone()),
				_ => None,
			})
			.collect()
	}

	/// Get the contract addresses and entry points of the transaction
	pub fn contract_addresses_and_entry_points(&self) -> Vec<(String, String)> {
		self.inner
			.operations
			.iter()
			.map(|op| match op {
				Operation::Call {
					address,
					entry_point,
					..
				} => (address.clone(), entry_point.clone()),
				Operation::Deploy { address, .. } => (address.clone(), "".to_string()),
				Operation::Maintain { address, .. } => (address.clone(), "".to_string()),
				_ => ("".to_string(), "".to_string()),
			})
			.filter(|(addr, entry)| !addr.is_empty() && !entry.is_empty())
			.collect()
	}
}

impl From<MidnightRpcTransaction> for Transaction {
	fn from(tx: MidnightRpcTransaction) -> Self {
		Self {
			inner: tx,
			status: true, // TODO: add status
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

impl<D: DB> TryFrom<(MidnightNodeTransaction<Proof, D>, &Vec<ChainConfiguration>)> for Transaction {
	type Error = anyhow::Error;

	fn try_from(
		(tx, chain_configurations): (MidnightNodeTransaction<Proof, D>, &Vec<ChainConfiguration>),
	) -> Result<Self, Self::Error> {
		let tx_hash = tx.transaction_hash().0 .0.encode_hex();

		let identifiers = tx
			.identifiers()
			.map(|id| match id {
				TransactionIdentifier::Merged(pedersen) => pedersen.0.to_string(),
				TransactionIdentifier::Unique(hash) => hash.0.encode_hex(),
			})
			.collect();

		// Check if chain_configuration has viewing keys and decrypt the transaction's coins
		for chain_configuration in chain_configurations {
			if let Some(midnight) = &chain_configuration.midnight {
				for viewing_key in &midnight.viewing_keys {
					if let SecretValue::Plain(secret) = viewing_key {
						let viewing_key_str = secret.as_str();
						// TODO: Do something with the coins...
						let _ = process_transaction_for_coins::<D>(viewing_key_str, &tx);
					}
				}
			}
		}

		let operations = match tx {
			MidnightNodeTransaction::Standard(stx) => {
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
			MidnightNodeTransaction::ClaimMint(mtx) => {
				vec![Operation::ClaimMint {
					value: mtx.mint.coin.value,
					coin_type: mtx.mint.coin.type_.0 .0.encode_hex(),
				}]
			}
		};

		Ok(Self {
			inner: MidnightRpcTransaction {
				tx_hash,
				operations,
				identifiers,
			},
			status: true, // TODO: add status by looking at extrinsic events
		})
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
