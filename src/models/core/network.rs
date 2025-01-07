use serde::{Deserialize, Serialize};

use crate::models::BlockChainType;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcUrl {
    pub type_: String,
    pub url: String,
    pub weight: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Network {
    pub network_type: BlockChainType,
    pub slug: String,
    pub name: String,
    pub rpc_urls: Vec<RpcUrl>,
    pub chain_id: Option<u64>,
    pub network_passphrase: Option<String>,
    pub block_time_ms: u64,
    pub confirmation_blocks: u64,
    pub cron_schedule: String,
    pub max_past_blocks: Option<u64>,
    pub store_blocks: Option<bool>,
}
