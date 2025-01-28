use openzeppelin_monitor::models::{
	BlockChainType, BlockType, EVMBlock, Network, RpcUrl, StellarBlock, StellarLedgerInfo,
};

pub fn create_test_network(name: &str, slug: &str, network_type: BlockChainType) -> Network {
	Network {
		name: name.to_string(),
		slug: slug.to_string(),
		network_type,
		rpc_urls: vec![RpcUrl {
			url: "http://localhost:8545".to_string(),
			type_: "rpc".to_string(),
			weight: 100,
		}],
		cron_schedule: "*/5 * * * * *".to_string(),
		confirmation_blocks: 1,
		store_blocks: Some(false),
		chain_id: Some(1),
		network_passphrase: None,
		block_time_ms: 1000,
		max_past_blocks: None,
	}
}

pub fn create_test_block(chain: BlockChainType, block_number: u64) -> BlockType {
	match chain {
		BlockChainType::EVM => BlockType::EVM(Box::new(EVMBlock::from(web3::types::Block {
			number: Some(block_number.into()),
			..Default::default()
		}))),
		BlockChainType::Stellar => {
			BlockType::Stellar(Box::new(StellarBlock::from(StellarLedgerInfo {
				sequence: block_number as u32,
				..Default::default()
			})))
		}
		_ => panic!("Unsupported chain"),
	}
}
