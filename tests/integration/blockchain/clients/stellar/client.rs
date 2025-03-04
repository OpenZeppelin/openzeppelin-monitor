use crate::integration::mocks::{
	create_stellar_test_network_with_urls, create_stellar_valid_server_mock_network_response,
	MockStellarClientTrait, MockStellarTransportClient,
};
use mockall::predicate;
use mockito::Server;
use openzeppelin_monitor::{
	models::{
		BlockType, StellarBlock, StellarEvent, StellarLedgerInfo, StellarTransaction,
		StellarTransactionInfo,
	},
	services::blockchain::{BlockChainClient, StellarClient, StellarClientTrait},
};

#[tokio::test]
async fn test_get_transactions() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let expected_transaction = StellarTransaction::from(StellarTransactionInfo {
		status: "SUCCESS".to_string(),
		transaction_hash: "test_hash".to_string(),
		..Default::default()
	});

	mock.expect_get_transactions()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![expected_transaction.clone()]));

	let result = mock.get_transactions(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_events() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let expected_event = StellarEvent {
		..Default::default()
	};

	mock.expect_get_events()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![expected_event.clone()]));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 100u64);
}

#[tokio::test]
async fn test_get_blocks() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();

	let block = BlockType::Stellar(Box::new(StellarBlock::from(StellarLedgerInfo {
		sequence: 1,
		..Default::default()
	})));

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
		BlockType::Stellar(block) => assert_eq!(block.sequence, 1),
		_ => panic!("Expected Stellar block"),
	}
}

#[tokio::test]
async fn test_new_client() {
	let mut server = Server::new_async().await;

	let mock = create_stellar_valid_server_mock_network_response(&mut server, "soroban");
	// Create a test network
	let network = create_stellar_test_network_with_urls(vec![&server.url()], "rpc");

	// Test successful client creation
	let result = StellarClient::new(&network).await;
	assert!(result.is_ok(), "Client creation should succeed");
	mock.assert();
}
