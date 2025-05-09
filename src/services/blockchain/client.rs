//! Core blockchain client interface and traits.
//!
//! This module defines the common interface that all blockchain implementations
//! must follow, ensuring consistent behavior across different blockchain types.

use async_trait::async_trait;

use crate::{
	models::{BlockType, ContractSpec},
	services::filter::BlockFilter,
};

/// Defines the core interface for blockchain clients
///
/// This trait must be implemented by all blockchain-specific clients to provide
/// standardized access to blockchain data and operations.
#[async_trait]
pub trait BlockChainClient: Send + Sync + Clone {
	/// Retrieves the latest block number from the blockchain
	///
	/// # Returns
	/// * `Result<u64, anyhow::Error>` - The latest block number or an error
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error>;

	/// Retrieves a range of blocks from the blockchain
	///
	/// # Arguments
	/// * `start_block` - The starting block number
	/// * `end_block` - Optional ending block number. If None, only fetches start_block
	///
	/// # Returns
	/// * `Result<Vec<BlockType>, anyhow::Error>` - Vector of blocks or an error
	///
	/// # Note
	/// The implementation should handle cases where end_block is None by returning
	/// only the start_block data.
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error>;

	/// Retrieves the contract spec for a given contract ID
	///
	/// # Arguments
	/// * `contract_id` - The ID of the contract to retrieve the spec for
	///
	/// # Returns
	/// * `Result<ContractSpec, anyhow::Error>` - The contract spec or an error
	async fn get_contract_spec(&self, _contract_id: &str) -> Result<ContractSpec, anyhow::Error> {
		Err(anyhow::anyhow!("get_contract_spec not implemented"))
	}
}

/// Defines the factory interface for creating block filters
///
/// This trait must be implemented by all blockchain-specific clients to provide
/// a way to create block filters.
pub trait BlockFilterFactory<T> {
	type Filter: BlockFilter<Client = T> + Send;
	fn filter() -> Self::Filter;
}
