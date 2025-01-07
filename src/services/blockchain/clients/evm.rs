use async_trait::async_trait;
use web3::types::{BlockId, BlockNumber};

use crate::models::{BlockType, EVMBlock, Network};
use crate::services::blockchain::transports::Web3TransportClient;
use crate::services::blockchain::{client::BlockChainClient, BlockChainError};
use crate::services::filter::helpers::evm::string_to_h256;
use crate::utils::WithRetry;

pub struct EvmClient {
    web3_client: Web3TransportClient,
    _network: Network,
}

impl EvmClient {
    pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
        let web3_client = Web3TransportClient::new(network).await?;
        Ok(Self {
            web3_client,
            _network: network.clone(),
        })
    }
}

#[async_trait]
pub trait EvmClientTrait: BlockChainClient {
    async fn get_transaction_receipt(
        &self,
        transaction_hash: String,
    ) -> Result<web3::types::TransactionReceipt, BlockChainError>;

    async fn get_logs_for_blocks(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<web3::types::Log>, BlockChainError>;
}

#[async_trait]
impl EvmClientTrait for EvmClient {
    async fn get_transaction_receipt(
        &self,
        transaction_hash: String,
    ) -> Result<web3::types::TransactionReceipt, BlockChainError> {
        let hash = string_to_h256(&transaction_hash).map_err(|e| {
            BlockChainError::internal_error(format!(
                "Invalid transaction hash ({}): {}",
                transaction_hash, e
            ))
        })?;

        let receipt = self
            .web3_client
            .client
            .eth()
            .transaction_receipt(hash)
            .await?;

        Ok(receipt.ok_or_else(|| {
            BlockChainError::request_error("Transaction receipt not found".to_string())
        })?)
    }

    async fn get_logs_for_blocks(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<web3::types::Log>, BlockChainError> {
        self.web3_client
            .client
            .eth()
            .logs(
                web3::types::FilterBuilder::default()
                    .from_block(BlockNumber::Number(from_block.into()))
                    .to_block(BlockNumber::Number(to_block.into()))
                    .build(),
            )
            .await
            .map_err(|e| BlockChainError::request_error(e.to_string()))
    }
}

#[async_trait]
impl BlockChainClient for EvmClient {
    async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
        let with_retry = WithRetry::with_default_config();
        with_retry
            .attempt(|| async {
                self.web3_client
                    .client
                    .eth()
                    .block_number()
                    .await
                    .map(|n| n.as_u64())
                    .map_err(|e| BlockChainError::request_error(e.to_string()))
            })
            .await
    }

    async fn get_blocks(
        &self,
        start_block: u64,
        end_block: Option<u64>,
    ) -> Result<Vec<BlockType>, BlockChainError> {
        let with_retry = WithRetry::with_default_config();
        with_retry
            .attempt(|| async {
                let mut blocks = Vec::new();
                for block_number in start_block..=end_block.unwrap_or(start_block) {
                    let block = self
                        .web3_client
                        .client
                        .eth()
                        .block_with_txs(BlockId::Number(BlockNumber::Number(block_number.into())))
                        .await?
                        .ok_or_else(|| BlockChainError::block_not_found(block_number))?;

                    blocks.push(BlockType::EVM(EVMBlock::from(block)));
                }
                Ok(blocks)
            })
            .await
    }
}
