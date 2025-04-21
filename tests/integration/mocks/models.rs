use mockito::{Mock, Server};
use openzeppelin_monitor::models::{
	BlockChainType, BlockType, EVMBlock, EVMTransaction, EVMTransactionReceipt, Network, RpcUrl,
	StellarBlock, StellarLedgerInfo, StellarTransaction, StellarTransactionInfo, TransactionType,
};
use serde_json::json;

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

pub fn create_stellar_test_network_with_urls(urls: Vec<&str>) -> Network {
	Network {
		name: "test".to_string(),
		slug: "test".to_string(),
		network_type: BlockChainType::Stellar,
		rpc_urls: urls
			.iter()
			.map(|url| RpcUrl {
				url: url.to_string(),
				type_: "rpc".to_string(),
				weight: 100,
			})
			.collect(),
		cron_schedule: "*/5 * * * * *".to_string(),
		confirmation_blocks: 1,
		store_blocks: Some(false),
		chain_id: None,
		network_passphrase: Some("Test SDF Network ; September 2015".to_string()),
		block_time_ms: 5000,
		max_past_blocks: None,
	}
}

pub fn create_stellar_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"getNetwork","params":[]}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(
			json!({
				"jsonrpc": "2.0",
				"result": {
					"friendbotUrl": "https://friendbot.stellar.org/",
					"passphrase": "Test SDF Network ; September 2015",
					"protocolVersion": 22
				},
				"id": 0
			})
			.to_string(),
		)
		.create()
}

pub fn create_evm_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"net_version","params":[]}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","id":1,"result":"1"}"#)
		.create()
}

pub fn create_evm_test_network_with_urls(urls: Vec<&str>) -> Network {
	Network {
		name: "test".to_string(),
		slug: "test".to_string(),
		network_type: BlockChainType::EVM,
		rpc_urls: urls
			.iter()
			.map(|url| RpcUrl {
				url: url.to_string(),
				type_: "rpc".to_string(),
				weight: 100,
			})
			.collect(),
		cron_schedule: "*/5 * * * * *".to_string(),
		confirmation_blocks: 1,
		store_blocks: Some(false),
		chain_id: None,
		network_passphrase: None,
		block_time_ms: 5000,
		max_past_blocks: None,
	}
}

pub fn create_http_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"net_version","params":[]}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","id":1,"result":"1"}"#)
		.create()
}

pub fn create_test_block(chain: BlockChainType, block_number: u64) -> BlockType {
	match chain {
		BlockChainType::EVM => BlockType::EVM(Box::new(EVMBlock::from(alloy::rpc::types::Block {
			header: alloy::rpc::types::Header {
				hash: alloy::primitives::B256::ZERO,
				inner: alloy::consensus::Header {
					number: block_number,
					..Default::default()
				},
				..Default::default()
			},
			transactions: alloy::rpc::types::BlockTransactions::Full(vec![]),
			uncles: vec![],
			withdrawals: None,
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

pub fn create_test_transaction(chain: BlockChainType) -> TransactionType {
	match chain {
		BlockChainType::EVM => {
			let tx = alloy::consensus::TxLegacy {
				chain_id: None,
				nonce: 0,
				gas_price: 0,
				gas_limit: 0,
				to: alloy::primitives::TxKind::Call(alloy::primitives::Address::ZERO),
				value: alloy::primitives::U256::ZERO,
				input: alloy::primitives::Bytes::default(),
			};

			let signature = alloy::signers::Signature::from_scalars_and_parity(
				alloy::primitives::B256::ZERO,
				alloy::primitives::B256::ZERO,
				false,
			);

			let hash = alloy::primitives::B256::ZERO;

			TransactionType::EVM(EVMTransaction::from(alloy::rpc::types::Transaction {
				inner: alloy::consensus::transaction::Recovered::new_unchecked(
					alloy::consensus::transaction::TxEnvelope::Legacy(
						alloy::consensus::Signed::new_unchecked(tx, signature, hash),
					),
					alloy::primitives::Address::ZERO,
				),
				block_hash: None,
				block_number: None,
				transaction_index: None,
				effective_gas_price: None,
			}))
		}
		BlockChainType::Stellar => {
			TransactionType::Stellar(StellarTransaction::from(StellarTransactionInfo {
				..Default::default()
			}))
		}
		_ => panic!("Unsupported chain"),
	}
}

pub fn create_test_evm_transaction_receipt() -> EVMTransactionReceipt {
	EVMTransactionReceipt::from(alloy::rpc::types::TransactionReceipt {
		inner: alloy::consensus::ReceiptEnvelope::Legacy(alloy::consensus::ReceiptWithBloom {
			receipt: alloy::consensus::Receipt::default(),
			logs_bloom: alloy::primitives::Bloom::default(),
		}),
		transaction_hash: alloy::primitives::B256::ZERO,
		transaction_index: Some(0),
		block_hash: Some(alloy::primitives::B256::ZERO),
		block_number: Some(0),
		gas_used: 0,
		effective_gas_price: 0,
		blob_gas_used: None,
		blob_gas_price: None,
		from: alloy::primitives::Address::ZERO,
		to: Some(alloy::primitives::Address::ZERO),
		contract_address: None,
	})
}
