use log::debug;
use mockito::{Mock, Server};
use openzeppelin_monitor::{
	models::{BlockChainType, Network, RpcUrl},
	services::blockchain::{
		BlockChainError, BlockchainTransport, HorizonTransportClient, RotatingTransport,
	},
};
use serde_json::json;

fn create_test_network(urls: Vec<&str>) -> Network {
	Network {
		name: "test".to_string(),
		slug: "test".to_string(),
		network_type: BlockChainType::Stellar,
		rpc_urls: urls
			.iter()
			.map(|url| RpcUrl {
				url: url.to_string(),
				type_: "horizon".to_string(),
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

fn create_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
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
		.create()
}

#[tokio::test]
async fn test_client_creation() {
	let mut server = Server::new_async().await;
	let mock = create_valid_server_mock_network_response(&mut server);
	let network = create_test_network(vec![&server.url()]);

	match HorizonTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server.url());
		}
		Err(e) => {
			debug!("Transport creation failed with error: {:?}", e);
			panic!("Transport creation failed: {:?}", e);
		}
	}
	mock.assert();
}

#[tokio::test]
async fn test_client_creation_with_fallback() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	let mock = server
		.mock("GET", "/")
		.with_header("content-type", "application/json")
		.with_status(500) // Simulate a failed request
		.create();

	let mock2 = create_valid_server_mock_network_response(&mut server2);

	let network = create_test_network(vec![&server.url(), &server2.url()]);

	match HorizonTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server2.url());
			mock.assert();
			mock2.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}
}

#[tokio::test]
async fn test_client_update_client() {
	let mut server = Server::new_async().await;
	let server2 = Server::new_async().await;

	let mock1 = create_valid_server_mock_network_response(&mut server);

	let network = create_test_network(vec![&server.url()]);

	let client = HorizonTransportClient::new(&network).await.unwrap();

	// Test successful update
	let result = client.update_client(&server2.url()).await;
	assert!(result.is_ok(), "Update to valid URL should succeed");

	assert_eq!(client.get_current_url().await, server2.url());

	// Test invalid URL update
	let result = client.update_client("invalid-url").await;
	assert!(result.is_err(), "Update with invalid URL should fail");
	match result {
		Err(BlockChainError::ConnectionError(msg)) => {
			assert_eq!(msg, "Failed to create client");
		}
		_ => panic!("Expected ConnectionError"),
	}

	// Verify both mock was called the expected number of times
	mock1.assert();
}

#[tokio::test]
async fn test_client_try_connect() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	let mock = create_valid_server_mock_network_response(&mut server);
	let mock2 = create_valid_server_mock_network_response(&mut server2);

	let network = create_test_network(vec![&server.url()]);
	let client = HorizonTransportClient::new(&network).await.unwrap();

	let result = client.try_connect(&server2.url()).await;
	assert!(result.is_ok(), "Try connect should succeed");

	let result = client.try_connect("invalid-url").await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");

	mock.assert();
	mock2.assert();
}
