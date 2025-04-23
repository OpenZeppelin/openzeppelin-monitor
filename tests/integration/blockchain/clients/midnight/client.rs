use crate::integration::mocks::{
	create_midnight_test_network_with_urls, create_midnight_valid_server_mock_network_response,
	MockMidnightClientTrait, MockMidnightTransportClient,
};
use mockall::predicate;
use mockito::Server;
use openzeppelin_monitor::{
	models::{
		BlockType, MidnightBlock, MidnightBlockDigest, MidnightBlockHeader, MidnightRpcBlock,
	},
	services::blockchain::{BlockChainClient, MidnightClient, MidnightClientTrait},
};

#[tokio::test]
async fn test_get_transactions() {
	let mut mock = MockMidnightClientTrait::<MockMidnightTransportClient>::new();

	mock.expect_get_transactions()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![]));

	let result = mock.get_transactions(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 0);
}

#[tokio::test]
async fn test_get_events() {
	let mut mock = MockMidnightClientTrait::<MockMidnightTransportClient>::new();

	mock.expect_get_events()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![]));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 0);
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

	let rpc_block = MidnightRpcBlock::<MidnightBlockHeader> {
		header: MidnightBlockHeader {
			parent_hash: "0xabc123".to_string(),
			number: "0x12345".to_string(),
			state_root: "0x1234567890abcdef".to_string(),
			extrinsics_root: "0xabcdef1234567890".to_string(),
			digest: MidnightBlockDigest { logs: vec![] },
		},
		body: vec![],
		transactions_index: vec![],
	};

	let block = BlockType::Midnight(Box::new(MidnightBlock::from(rpc_block)));

	let blocks = vec![block];

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
