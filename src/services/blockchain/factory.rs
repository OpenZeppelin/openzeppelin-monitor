use crate::models::{BlockChainType, Network};

use super::{
    clients::{EvmClient, StellarClient},
    BlockChainClientEnum, BlockChainError,
};

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
        BlockChainType::Midnight => {
            unimplemented!("Midnight client not yet implemented")
        }
        BlockChainType::Solana => {
            unimplemented!("Solana client not yet implemented")
        }
    }
}
