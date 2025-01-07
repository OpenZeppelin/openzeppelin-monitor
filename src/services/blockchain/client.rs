use async_trait::async_trait;

use crate::models::BlockType;

use super::BlockChainError;

#[async_trait]
pub trait BlockChainClient: Send + Sync {
    async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;
    async fn get_blocks(
        &self,
        start_block: u64,
        end_block: Option<u64>,
    ) -> Result<Vec<BlockType>, BlockChainError>;
}
