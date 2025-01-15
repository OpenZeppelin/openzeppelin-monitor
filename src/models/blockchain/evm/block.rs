//! EVM block data structures.

use serde::{Deserialize, Serialize};
use std::ops::Deref;
use web3::types::{Block as Web3Block, Transaction as Web3Transaction};

/// Wrapper around Web3 Block that implements additional functionality
///
/// This type provides a convenient interface for working with EVM blocks
/// while maintaining compatibility with the web3 types.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block(pub Web3Block<Web3Transaction>);

impl Block {
	/// Get the block number
	///
	/// Unwraps the optional block number and converts it to u64.
	pub fn number(&self) -> u64 {
		self.0.number.unwrap().as_u64()
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
