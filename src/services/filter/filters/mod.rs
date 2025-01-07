mod evm;
mod stellar;

use async_trait::async_trait;
pub use evm::EVMBlockFilter;
pub use stellar::StellarBlockFilter;

use crate::{
    models::{BlockType, Monitor, MonitorMatch, Network},
    services::blockchain::BlockChainClientEnum,
};

use super::FilterError;

#[async_trait]
pub trait BlockFilter {
    async fn filter_block(
        &self,
        client: &BlockChainClientEnum,
        network: &Network,
        block: &BlockType,
        monitors: &[Monitor],
    ) -> Result<Vec<MonitorMatch>, FilterError>;
}

pub struct FilterService {}

impl FilterService {
    pub fn new() -> Self {
        FilterService {}
    }
}

impl FilterService {
    pub async fn filter_block(
        &self,
        client: &BlockChainClientEnum,
        network: &Network,
        block: &BlockType,
        monitors: &[Monitor],
    ) -> Result<Vec<MonitorMatch>, FilterError> {
        match block {
            BlockType::EVM(_) => {
                let filter = EVMBlockFilter {};
                filter.filter_block(client, network, block, monitors).await
            }
            BlockType::Stellar(_) => {
                let filter = StellarBlockFilter {};
                filter.filter_block(client, network, block, monitors).await
            }
        }
    }
}
