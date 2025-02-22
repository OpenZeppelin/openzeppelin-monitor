use mockito::{Mock, Server};
use openzeppelin_monitor::{
	models::{BlockChainType, Network, RpcUrl},
	services::blockchain::{
		BlockChainError, BlockchainTransport, RotatingTransport, Web3TransportClient,
	},
};

fn create_test_network(urls: Vec<&str>) -> Network {
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

fn create_valid_server_mock_network_response(server: &mut Server) -> Mock {
	server
		.mock("POST", "/")
		.match_body(r#"{"jsonrpc":"2.0","method":"net_version","params":[],"id":0}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":"1","id":0}"#)
		.create()
}

#[tokio::test]
async fn test_client_creation() {
	let mut server = Server::new_async().await;
	let mock = create_valid_server_mock_network_response(&mut server);
	let network = create_test_network(vec![&server.url()]);

	match Web3TransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server.url());
			mock.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}
}

#[tokio::test]
async fn test_client_creation_with_fallback() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.match_body(r#"{"jsonrpc":"2.0","method":"net_version","params":[],"id":0}"#)
		.with_header("content-type", "application/json")
		.with_status(500) // Simulate a failed request
		.create();

	let mock2 = create_valid_server_mock_network_response(&mut server2);

	let network = create_test_network(vec![&server.url(), &server2.url()]);

	match Web3TransportClient::new(&network).await {
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
	let client = Web3TransportClient::new(&network).await.unwrap();

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

	mock1.assert();
}

#[tokio::test]
async fn test_client_try_connect() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	let mock = create_valid_server_mock_network_response(&mut server);
	let mock2 = create_valid_server_mock_network_response(&mut server2);

	let network = create_test_network(vec![&server.url()]);
	let client = Web3TransportClient::new(&network).await.unwrap();

	let result = client.try_connect(&server2.url()).await;
	assert!(result.is_ok(), "Try connect should succeed");

	let result = client.try_connect("invalid://url").await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");

	mock.assert();
	mock2.assert();
}
