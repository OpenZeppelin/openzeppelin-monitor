use mockito::{Mock, Server};
use openzeppelin_monitor::{
	models::{
		BlockChainType, BlockType, EVMBlock, EVMReceiptLog, EVMTransactionReceipt, Network,
		StellarBlock, StellarLedgerInfo, StellarTransaction, StellarTransactionInfo,
		TransactionType,
	},
	utils::tests::{
		builders::network::NetworkBuilder,
		evm::{receipt::ReceiptBuilder, transaction::TransactionBuilder},
	},
};
use serde_json::json;

pub fn create_test_network(name: &str, slug: &str, network_type: BlockChainType) -> Network {
	NetworkBuilder::new()
		.name(name)
		.slug(slug)
		.network_type(network_type)
		.rpc_url("http://localhost:8545")
		.cron_schedule("*/5 * * * * *")
		.confirmation_blocks(1)
		.store_blocks(false)
		.chain_id(1)
		.block_time_ms(1000)
		.build()
}

pub fn create_stellar_test_network_with_urls(urls: Vec<&str>) -> Network {
	NetworkBuilder::new()
		.name("test")
		.slug("test")
		.network_type(BlockChainType::Stellar)
		.cron_schedule("*/5 * * * * *")
		.confirmation_blocks(1)
		.store_blocks(false)
		.block_time_ms(5000)
		.network_passphrase("Test SDF Network ; September 2015")
		.rpc_urls(urls)
		.build()
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
	NetworkBuilder::new()
		.name("test")
		.slug("test")
		.network_type(BlockChainType::EVM)
		.cron_schedule("*/5 * * * * *")
		.confirmation_blocks(1)
		.store_blocks(false)
		.block_time_ms(5000)
		.rpc_urls(urls)
		.build()
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
		BlockChainType::EVM => TransactionType::EVM(TransactionBuilder::new().build()),
		BlockChainType::Stellar => {
			TransactionType::Stellar(Box::new(StellarTransaction::from(StellarTransactionInfo {
				..Default::default()
			})))
		}
		_ => panic!("Unsupported chain"),
	}
}

pub fn create_test_evm_transaction_receipt() -> EVMTransactionReceipt {
	ReceiptBuilder::new().build()
}

pub fn create_test_evm_logs() -> Vec<EVMReceiptLog> {
	ReceiptBuilder::new().build().logs.clone()
}
