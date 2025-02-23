use mockito::{Mock, Server};
use openzeppelin_monitor::models::{
	BlockChainType, BlockType, EVMBlock, Network, RpcUrl, StellarBlock, StellarLedgerInfo,
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

pub fn create_stellar_test_network_with_urls(urls: Vec<&str>, type_: &str) -> Network {
	Network {
		name: "test".to_string(),
		slug: "test".to_string(),
		network_type: BlockChainType::Stellar,
		rpc_urls: urls
			.iter()
			.map(|url| RpcUrl {
				url: url.to_string(),
				type_: type_.to_string(),
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

pub fn create_stellar_valid_server_mock_network_response(server: &mut Server, type_: &str) -> Mock {
	match type_ {
		"horizon" => server
			.mock("GET", "/")
			.match_header("x-client-name", "aurora-rs/stellar-horizon-rs")
			.match_header("x-client-version", "0.7.1")
			.with_status(200)
			.with_header("content-type", "application/json")
		.with_body(
			json!({
				"_links": {
					"account": {
						"href": server.url() + "/accounts/{account_id}",
						"templated": true
					},
					"accounts": {
						"href": server.url() + "/accounts{?signer,sponsor,asset,liquidity_pool,cursor,limit,order}",
						"templated": true
					},
					"account_transactions": {
						"href": server.url() + "/accounts/{account_id}/transactions{?cursor,limit,order}",
						"templated": true
					},
					"claimable_balances": {
						"href": server.url() + "/claimable_balances{?asset,sponsor,claimant,cursor,limit,order}",
						"templated": true
					},
					"assets": {
						"href": server.url() + "/assets{?asset_code,asset_issuer,cursor,limit,order}",
						"templated": true
					},
					"effects": {
						"href": server.url() + "/effects{?cursor,limit,order}",
						"templated": true
					},
					"fee_stats": {
						"href": server.url() + "/fee_stats"
					},
					"ledger": {
						"href": server.url() + "/ledgers/{sequence}",
						"templated": true
					},
					"ledgers": {
						"href": server.url() + "/ledgers{?cursor,limit,order}",
						"templated": true
					},
					"liquidity_pools": {
						"href": server.url() + "/liquidity_pools{?reserves,account,cursor,limit,order}",
						"templated": true
					},
					"offer": {
						"href": server.url() + "/offers/{offer_id}",
						"templated": true
					},
					"offers": {
						"href": server.url() + "/offers{?selling,buying,seller,sponsor,cursor,limit,order}",
						"templated": true
					},
					"operation": {
						"href": server.url() + "/operations/{id}",
						"templated": true
					},
					"operations": {
						"href": server.url() + "/operations{?cursor,limit,order,include_failed}",
						"templated": true
					},
					"order_book": {
						"href": server.url() + "/order_book{?selling_asset_type,selling_asset_code,selling_asset_issuer,buying_asset_type,buying_asset_code,buying_asset_issuer,limit}",
						"templated": true
					},
					"payments": {
						"href": server.url() + "/payments{?cursor,limit,order,include_failed}",
						"templated": true
					},
					"self": {
						"href": server.url()
					},
					"strict_receive_paths": {
						"href": server.url() + "/paths/strict-receive{?source_assets,source_account,destination_account,destination_asset_type,destination_asset_issuer,destination_asset_code,destination_amount}",
						"templated": true
					},
					"strict_send_paths": {
						"href": server.url() + "/paths/strict-send{?destination_account,destination_assets,source_asset_type,source_asset_issuer,source_asset_code,source_amount}",
						"templated": true
					},
					"trade_aggregations": {
						"href": server.url() + "/trade_aggregations",
						"templated": true
					},
					"trades": {
						"href": server.url() + "/trades",
						"templated": true
					},
					"transaction": {
						"href": server.url() + "/transactions/{hash}",
						"templated": true
					},
					"transactions": {
						"href": server.url() + "/transactions{?cursor,limit,order}",
						"templated": true
					}
				},
				"horizon_version": "22.0.2-5df7099e675469f409ad1b8ef5bb2a8a19db7f00",
				"core_version": "stellar-core 22.1.0 (0241e79f74dc017f20e190abd3825873222c5ca5)",
				"ingest_latest_ledger": 1276131,
				"history_latest_ledger": 1276131,
				"history_latest_ledger_closed_at": "2025-02-22T15:34:42Z",
				"history_elder_ledger": 2,
				"core_latest_ledger": 1276131,
				"network_passphrase": "Test SDF Network ; September 2015",
				"current_protocol_version": 22,
				"supported_protocol_version": 22,
				"core_supported_protocol_version": 22
				})
				.to_string(),
			)
			.create(),
		"soroban" =>
			server
				.mock("POST", "/")
				.match_body(r#"{"jsonrpc":"2.0","id":0,"method":"getNetwork"}"#)
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
				.create(),
		_ => panic!("Unsupported type"),
	}
}

pub fn create_evm_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
		.mock("POST", "/")
		.match_body(r#"{"jsonrpc":"2.0","method":"net_version","params":[],"id":0}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":"1","id":0}"#)
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
