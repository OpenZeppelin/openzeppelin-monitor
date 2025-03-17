//! EVM block data structures.

use alloy::{
	primitives::{aliases::B2048, Address, Bytes, B256, B64, U256, U64},
	rpc::types::{Block as AlloyBlock, BlockTransactions, Transaction as AlloyTransaction},
};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use super::EVMTransaction;

/// Base Block struct
/// Copied from web3 crate (now deprecated) and slightly modified for alloy compatibility
#[derive(Debug, Default, Clone, PartialEq, Deserialize, Serialize)]
pub struct BaseBlock<TX> {
	/// Hash of the block
	pub hash: Option<B256>,
	/// Hash of the parent
	#[serde(rename = "parentHash")]
	pub parent_hash: B256,
	/// Hash of the uncles
	#[serde(rename = "sha3Uncles")]
	#[serde(default)]
	pub uncles_hash: B256,
	/// Miner/author's address.
	#[serde(rename = "miner", default)]
	pub author: Address,
	/// State root hash
	#[serde(rename = "stateRoot")]
	pub state_root: B256,
	/// Transactions root hash
	#[serde(rename = "transactionsRoot")]
	pub transactions_root: B256,
	/// Transactions receipts root hash
	#[serde(rename = "receiptsRoot")]
	pub receipts_root: B256,
	/// Block number. None if pending.
	pub number: Option<U64>,
	/// Gas Used
	#[serde(rename = "gasUsed")]
	pub gas_used: U256,
	/// Gas Limit
	#[serde(rename = "gasLimit")]
	#[serde(default)]
	pub gas_limit: U256,
	/// Base fee per unit of gas (if past London)
	#[serde(rename = "baseFeePerGas", skip_serializing_if = "Option::is_none")]
	pub base_fee_per_gas: Option<U256>,
	/// Extra data
	#[serde(rename = "extraData")]
	pub extra_data: Bytes,
	/// Logs bloom
	#[serde(rename = "logsBloom")]
	pub logs_bloom: Option<B2048>,
	/// Timestamp
	pub timestamp: U256,
	/// Difficulty
	#[serde(default)]
	pub difficulty: U256,
	/// Total difficulty
	#[serde(rename = "totalDifficulty")]
	pub total_difficulty: Option<U256>,
	/// Seal fields
	#[serde(default, rename = "sealFields")]
	pub seal_fields: Vec<Bytes>,
	/// Uncles' hashes
	#[serde(default)]
	pub uncles: Vec<B256>,
	/// Transactions
	pub transactions: Vec<TX>,
	/// Size in bytes
	pub size: Option<U256>,
	/// Mix Hash
	#[serde(rename = "mixHash")]
	pub mix_hash: Option<B256>,
	/// Nonce
	pub nonce: Option<B64>,
}

/// Wrapper around Base Block that implements additional functionality
///
/// This type provides a convenient interface for working with EVM blocks
/// while maintaining compatibility with the alloy types.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block(pub BaseBlock<EVMTransaction>);

impl Block {
	/// Get the block number
	///
	/// Returns the block number as an `Option<u64>`.
	pub fn number(&self) -> Option<u64> {
		self.0.number.map(|n| n.to())
	}
}

impl From<BaseBlock<EVMTransaction>> for Block {
	fn from(block: BaseBlock<EVMTransaction>) -> Self {
		Self(block)
	}
}

impl From<AlloyBlock<AlloyTransaction>> for Block {
	fn from(block: AlloyBlock<AlloyTransaction>) -> Self {
		let block = BaseBlock {
			hash: Some(block.header.hash),
			parent_hash: block.header.inner.parent_hash,
			uncles_hash: block.header.inner.ommers_hash,
			author: block.header.inner.beneficiary,
			state_root: block.header.inner.state_root,
			transactions_root: block.header.inner.transactions_root,
			receipts_root: block.header.inner.receipts_root,
			number: Some(U64::from(block.header.inner.number)),
			gas_used: U256::from(block.header.inner.gas_used),
			gas_limit: U256::from(block.header.inner.gas_limit),
			base_fee_per_gas: block
				.header
				.inner
				.base_fee_per_gas
				.map(|fee| U256::from(fee)),
			extra_data: block.header.inner.extra_data,
			logs_bloom: Some(block.header.inner.logs_bloom.into()),
			timestamp: U256::from(block.header.inner.timestamp),
			difficulty: block.header.inner.difficulty,
			total_difficulty: block.header.total_difficulty,
			seal_fields: vec![], // Alloy doesn't have seal fields
			uncles: block.uncles,
			transactions: match block.transactions {
				BlockTransactions::Full(txs) => txs.into_iter().map(EVMTransaction::from).collect(),
				_ => vec![],
			},
			size: block.header.size.map(|s| U256::from(s)),
			mix_hash: Some(block.header.inner.mix_hash),
			nonce: Some(block.header.inner.nonce),
		};

		Self(block)
	}
}

impl Deref for Block {
	type Target = BaseBlock<EVMTransaction>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloy::primitives::{Address, B256, U256, U64};

	fn create_test_block(block_number: u64) -> BaseBlock<EVMTransaction> {
		BaseBlock {
			number: Some(U64::from(block_number)),
			hash: Some(B256::ZERO),
			parent_hash: B256::ZERO,
			uncles_hash: B256::ZERO,
			author: Address::ZERO,
			state_root: B256::ZERO,
			transactions_root: B256::ZERO,
			receipts_root: B256::ZERO,
			gas_used: U256::ZERO,
			gas_limit: U256::ZERO,
			extra_data: vec![].into(),
			logs_bloom: None,
			timestamp: U256::ZERO,
			difficulty: U256::ZERO,
			total_difficulty: None,
			seal_fields: vec![],
			uncles: vec![],
			transactions: vec![],
			size: None,
			mix_hash: None,
			nonce: None,
			base_fee_per_gas: None,
		}
	}

	#[test]
	fn test_block_number() {
		// Create a test block with number
		let base_block = create_test_block(12345);
		let block = Block(base_block.clone());
		assert_eq!(block.number(), Some(12345));

		// Test with None value
		let base_block_no_number = BaseBlock {
			number: None,
			..base_block
		};
		let block_no_number = Block(base_block_no_number);
		assert_eq!(block_no_number.number(), None);
	}

	#[test]
	fn test_from_base_block() {
		let base_block = create_test_block(12345);
		let block: Block = base_block.clone().into();
		assert_eq!(block.0.number, base_block.number);
	}

	#[test]
	fn test_deref() {
		let base_block = create_test_block(12345);

		let block = Block(base_block.clone());
		// Test that we can access BaseBlock fields through deref
		assert_eq!(block.number, base_block.number);
		assert_eq!(block.hash, base_block.hash);
	}
}
