//! EVM block data structures.

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use web3::types::{Block as Web3Block, Transaction as Web3Transaction};

/// Wrapper around Web3 Block that implements additional functionality
///
/// This type provides a convenient interface for working with EVM blocks
/// while maintaining compatibility with the web3 types.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Block(pub Web3Block<Web3Transaction>);

impl Block {
	/// Get the block number
	///
	/// Returns the block number as an `Option<u64>`.
	pub fn number(&self) -> Option<u64> {
		self.0.number.map(|n| n.as_u64())
	}
}

impl From<Web3Block<Web3Transaction>> for Block {
	fn from(block: Web3Block<Web3Transaction>) -> Self {
		Self(block)
	}
}

impl Deref for Block {
	type Target = Web3Block<Web3Transaction>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use web3::types::{H160, H256, U256, U64};

	#[test]
	fn test_block_number() {
		// Create a test block with number
		let web3_block = Web3Block {
			number: Some(U64::from(12345)),
			hash: Some(H256::zero()),
			parent_hash: H256::zero(),
			uncles_hash: H256::zero(),
			author: H160::zero(),
			state_root: H256::zero(),
			transactions_root: H256::zero(),
			receipts_root: H256::zero(),
			gas_used: U256::zero(),
			gas_limit: U256::zero(),
			extra_data: vec![].into(),
			logs_bloom: None,
			timestamp: U256::zero(),
			difficulty: U256::zero(),
			total_difficulty: None,
			seal_fields: vec![],
			uncles: vec![],
			transactions: vec![],
			size: None,
			mix_hash: None,
			nonce: None,
			base_fee_per_gas: None,
		};

		let block = Block(web3_block.clone());
		assert_eq!(block.number(), Some(12345));

		// Test with None value
		let web3_block_no_number = Web3Block {
			number: None,
			..web3_block
		};
		let block_no_number = Block(web3_block_no_number);
		assert_eq!(block_no_number.number(), None);
	}

	#[test]
	fn test_from_web3_block() {
		let web3_block = Web3Block {
			number: Some(U64::from(12345)),
			hash: Some(H256::zero()),
			parent_hash: H256::zero(),
			uncles_hash: H256::zero(),
			author: H160::zero(),
			state_root: H256::zero(),
			transactions_root: H256::zero(),
			receipts_root: H256::zero(),
			gas_used: U256::zero(),
			gas_limit: U256::zero(),
			extra_data: vec![].into(),
			logs_bloom: None,
			timestamp: U256::zero(),
			difficulty: U256::zero(),
			total_difficulty: None,
			seal_fields: vec![],
			uncles: vec![],
			transactions: vec![],
			size: None,
			mix_hash: None,
			nonce: None,
			base_fee_per_gas: None,
		};

		let block: Block = web3_block.clone().into();
		assert_eq!(block.0.number, web3_block.number);
	}

	#[test]
	fn test_deref() {
		let web3_block = Web3Block {
			number: Some(U64::from(12345)),
			hash: Some(H256::zero()),
			parent_hash: H256::zero(),
			uncles_hash: H256::zero(),
			author: H160::zero(),
			state_root: H256::zero(),
			transactions_root: H256::zero(),
			receipts_root: H256::zero(),
			gas_used: U256::zero(),
			gas_limit: U256::zero(),
			extra_data: vec![].into(),
			logs_bloom: None,
			timestamp: U256::zero(),
			difficulty: U256::zero(),
			total_difficulty: None,
			seal_fields: vec![],
			uncles: vec![],
			transactions: vec![],
			size: None,
			mix_hash: None,
			nonce: None,
			base_fee_per_gas: None,
		};

		let block = Block(web3_block.clone());
		// Test that we can access Web3Block fields through deref
		assert_eq!(block.number, web3_block.number);
		assert_eq!(block.hash, web3_block.hash);
	}
}
