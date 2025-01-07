mod evm;
mod stellar;

use async_trait::async_trait;
pub use evm::{EvmClient, EvmClientTrait};
pub use stellar::{StellarClient, StellarClientTrait};

use crate::models::BlockType;

use super::{BlockChainClient, BlockChainError};

pub enum BlockChainClientEnum {
    EVM(Box<dyn EvmClientTrait>),
    Stellar(Box<dyn StellarClientTrait>),
}

#[async_trait]
impl BlockChainClient for BlockChainClientEnum {
    async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
        match self {
            BlockChainClientEnum::EVM(client) => client.get_latest_block_number().await,
            BlockChainClientEnum::Stellar(client) => client.get_latest_block_number().await,
        }
    }

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
