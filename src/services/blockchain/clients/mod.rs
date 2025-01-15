//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network
//!
//! Provides a unified enum type for handling different client types.

mod evm;
mod stellar;

use async_trait::async_trait;
pub use evm::{EvmClient, EvmClientTrait};
pub use stellar::{StellarClient, StellarClientTrait};

use crate::{
	models::BlockType,
	services::blockchain::{BlockChainClient, BlockChainError},
};

/// Enum wrapper for different blockchain client implementations
///
/// This enum allows for unified handling of different blockchain clients
/// while maintaining type safety and specific functionality for each chain.
pub enum BlockChainClientEnum {
	/// EVM-compatible blockchain client implementation
	EVM(Box<dyn EvmClientTrait>),
	/// Stellar blockchain client implementation
	Stellar(Box<dyn StellarClientTrait>),
}

#[async_trait]
impl BlockChainClient for BlockChainClientEnum {
	/// Delegates the latest block number request to the specific client implementation
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
		match self {
			BlockChainClientEnum::EVM(client) => client.get_latest_block_number().await,
			BlockChainClientEnum::Stellar(client) => client.get_latest_block_number().await,
		}
	}

	/// Delegates the block retrieval request to the specific client implementation
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, BlockChainError> {
		match self {
			BlockChainClientEnum::EVM(client) => client.get_blocks(start_block, end_block).await,
			BlockChainClientEnum::Stellar(client) => {
				client.get_blocks(start_block, end_block).await
			}
		}
	}
}
