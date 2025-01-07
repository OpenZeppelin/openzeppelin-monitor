use serde::{Deserialize, Serialize};

pub mod evm;
pub mod stellar;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockChainType {
    EVM,
    Stellar,
    Midnight,
    Solana,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockType {
    EVM(evm::EVMBlock),
    Stellar(stellar::StellarBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorMatch {
    EVM(evm::EVMMonitorMatch),
    Stellar(stellar::StellarMonitorMatch),
}
