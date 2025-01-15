//! Core blockchain client interface and traits.
//!
//! This module defines the common interface that all blockchain implementations
//! must follow, ensuring consistent behavior across different blockchain types.

use async_trait::async_trait;

use crate::{models::BlockType, services::blockchain::error::BlockChainError};

/// Defines the core interface for blockchain clients
///
/// This trait must be implemented by all blockchain-specific clients to provide
/// standardized access to blockchain data and operations.
#[async_trait]
pub trait BlockChainClient: Send + Sync {
	/// Retrieves the latest block number from the blockchain
	///
	/// # Returns
	/// * `Result<u64, BlockChainError>` - The latest block number or an error
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;

	/// Retrieves a range of blocks from the blockchain
	///
	/// # Arguments
	/// * `start_block` - The starting block number
	/// * `end_block` - Optional ending block number. If None, only fetches start_block
	///
	/// # Returns
	/// * `Result<Vec<BlockType>, BlockChainError>` - Vector of blocks or an error
	///
	/// # Note
	/// The implementation should handle cases where end_block is None by returning
	/// only the start_block data.
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, BlockChainError>;
}
