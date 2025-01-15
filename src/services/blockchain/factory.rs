//! Blockchain client factory implementation.
//!
//! This module provides factory functionality to create appropriate blockchain clients
//! based on the network type, abstracting away the specifics of client initialization.

use crate::models::{BlockChainType, Network};

use crate::services::blockchain::{
	clients::{EvmClient, StellarClient},
	BlockChainClientEnum, BlockChainError,
};

/// Creates appropriate blockchain clients based on network type
///
/// # Arguments
/// * `network` - Network configuration containing the blockchain type and connection details
///
/// # Returns
/// * `Result<BlockChainClientEnum, BlockChainError>` - Initialized blockchain client or error
pub async fn create_blockchain_client(
	network: &Network,
) -> Result<BlockChainClientEnum, BlockChainError> {
	match network.network_type {
		BlockChainType::EVM => {
			let client = EvmClient::new(network).await?;
			Ok(BlockChainClientEnum::EVM(Box::new(client)))
		}
		BlockChainType::Stellar => {
			let client = StellarClient::new(network).await?;
			Ok(BlockChainClientEnum::Stellar(Box::new(client)))
		}
		// Future blockchain implementations
		BlockChainType::Midnight => unimplemented!("Midnight client not yet implemented"),
		BlockChainType::Solana => unimplemented!("Solana client not yet implemented"),
	}
}
