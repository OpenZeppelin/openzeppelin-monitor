use openzeppelin_monitor::models::{BlockType, StellarEvent, StellarTransaction};
use openzeppelin_monitor::services::blockchain::{
    BlockChainClient, BlockChainError, EvmClientTrait, StellarClientTrait,
};

use async_trait::async_trait;
use mockall::{mock, predicate::*};

mock! {
    pub EvmClientTrait {}

    #[async_trait]
    impl BlockChainClient for EvmClientTrait {
        async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;
        async fn get_blocks(
            &self,
            start_block: u64,
            end_block: Option<u64>,
        ) -> Result<Vec<BlockType>, BlockChainError>;
    }

    #[async_trait]
    impl EvmClientTrait for EvmClientTrait {
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
}

mock! {
    pub StellarClientTrait {}

    #[async_trait]
    impl BlockChainClient for StellarClientTrait {
        async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;
        async fn get_blocks(
            &self,
            start_block: u64,
            end_block: Option<u64>,
        ) -> Result<Vec<BlockType>, BlockChainError>;
    }

    #[async_trait]
    impl StellarClientTrait for StellarClientTrait {
        async fn get_transactions(
            &self,
            start_sequence: u32,
            end_sequence: Option<u32>,
        ) -> Result<Vec<StellarTransaction>, BlockChainError>;

        async fn get_events(
            &self,
            start_sequence: u32,
            end_sequence: Option<u32>,
        ) -> Result<Vec<StellarEvent>, BlockChainError>;
    }
}
