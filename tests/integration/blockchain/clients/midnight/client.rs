use crate::integration::mocks::{
	create_midnight_test_network_with_urls, create_midnight_valid_server_mock_network_response,
	MockMidnightClientTrait, MockMidnightTransportClient,
};
use mockall::predicate;
use mockito::Server;
use openzeppelin_monitor::{
	models::BlockType,
	services::blockchain::{BlockChainClient, MidnightClient, MidnightClientTrait},
	utils::tests::midnight::block::BlockBuilder,
};

#[tokio::test]
async fn test_get_events() {
	let mut mock = MockMidnightClientTrait::<MockMidnightTransportClient>::new();

	mock.expect_get_events()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(vec![]));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 0);
}

#[tokio::test]
async fn test_get_chain_type() {
	let mut server = Server::new_async().await;

	// Mock system_chain when initializing client
	let mock_init = create_midnight_valid_server_mock_network_response(&mut server);

	// Test testnet chain type
	let mock_dev = server
		.mock("POST", "/")
		.with_body(r#"{"jsonrpc":"2.0","result":"testnet-02-1","id":1}"#)
		.expect(1)
		.create_async()
		.await;

	let network = create_midnight_test_network_with_urls(vec![&server.url()]);

	let client = MidnightClient::new(&network).await.unwrap();
	mock_init.assert_async().await;

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "testnet-02-1");
	mock_dev.assert_async().await;

	// Test mainnet chain type
	let mock_prod = server
		.mock("POST", "/")
		.with_body(r#"{"jsonrpc":"2.0","result":"mainnet-01-1","id":1}"#)
		.create_async()
		.await;

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "mainnet-01-1");
	mock_prod.assert_async().await;
}

#[tokio::test]
async fn test_get_chain_type_error_cases() {
	let mut server = Server::new_async().await;

	// Mock system_chain when initializing client
	let mock_init = create_midnight_valid_server_mock_network_response(&mut server);

	// Test missing result field
	let mock_missing_result = server
		.mock("POST", "/")
		.with_body(r#"{"jsonrpc":"2.0","id":1}"#)
		.create_async()
		.await;

	let network = create_midnight_test_network_with_urls(vec![&server.url()]);
	let client = MidnightClient::new(&network).await.unwrap();
	mock_init.assert_async().await;
	let result = client.get_chain_type().await;

	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");
	mock_missing_result.assert_async().await;

	// Test null result field
	let mock_null_result = server
		.mock("POST", "/")
		.with_body(r#"{"jsonrpc":"2.0","result":null,"id":1}"#)
		.create_async()
		.await;

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");
	mock_null_result.assert_async().await;

	// Test invalid JSON response
	let mock_invalid_json = server
		.mock("POST", "/")
		.with_body(r#"{"jsonrpc":"2.0","result":123,"id":1}"#)
		.create_async()
		.await;

	let result = client.get_chain_type().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), "");
	mock_invalid_json.assert_async().await;
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockMidnightClientTrait::<MockMidnightTransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 100u64);
}

#[tokio::test]
async fn test_get_blocks() {
	let mut mock = MockMidnightClientTrait::<MockMidnightTransportClient>::new();

	let block = BlockBuilder::new()
		.parent_hash("0xabc123".to_string())
		.number(74565)
		.build();

	let blocks = vec![BlockType::Midnight(Box::new(block))];

	mock.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(blocks.clone()));

	let result = mock.get_blocks(1, Some(2)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
	match &blocks[0] {
		BlockType::Midnight(block) => assert_eq!(block.number(), Some(74565)),
		_ => panic!("Expected Midnight block"),
	}
}

#[tokio::test]
async fn test_new_client() {
	let mut server = Server::new_async().await;

	let mock = create_midnight_valid_server_mock_network_response(&mut server);
	// Create a test network
	let network = create_midnight_test_network_with_urls(vec![&server.url()]);

	// Test successful client creation
	let result = MidnightClient::new(&network).await;
	assert!(result.is_ok(), "Client creation should succeed");
	mock.assert();
}
