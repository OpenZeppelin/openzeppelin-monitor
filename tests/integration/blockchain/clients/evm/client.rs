use crate::integration::mocks::{
	create_evm_test_network_with_urls, create_evm_valid_server_mock_network_response,
	MockEvmClientTrait, MockWeb3TransportClient,
};
use mockall::predicate;
use mockito::Server;
use openzeppelin_monitor::{
	models::{BlockType, EVMBlock},
	services::blockchain::{BlockChainClient, EvmClient, EvmClientTrait},
};
use web3::types::{Log, TransactionReceipt, H256};

#[tokio::test]
async fn test_get_transaction_receipt() {
	let mut mock = MockEvmClientTrait::<MockWeb3TransportClient>::new();
	let expected_receipt = TransactionReceipt {
		transaction_hash: H256::zero(),
		..Default::default()
	};

	mock.expect_get_transaction_receipt()
		.with(predicate::eq("0x123".to_string()))
		.times(1)
		.returning(move |_| Ok(expected_receipt.clone()));

	let result = mock.get_transaction_receipt("0x123".to_string()).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().transaction_hash, H256::zero());
}

#[tokio::test]
async fn test_get_logs_for_blocks() {
	let mut mock = MockEvmClientTrait::<MockWeb3TransportClient>::new();
	let expected_logs = vec![Log {
		address: Default::default(),
		topics: vec![],
		data: vec![].into(),
		block_number: Some(1u64.into()),
		block_hash: None,
		transaction_hash: None,
		transaction_index: None,
		log_index: None,
		transaction_log_index: None,
		log_type: None,
		removed: None,
	}];

	mock.expect_get_logs_for_blocks()
		.with(predicate::eq(1u64), predicate::eq(2u64))
		.times(1)
		.returning(move |_, _| Ok(expected_logs.clone()));

	let result = mock.get_logs_for_blocks(1, 2).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockEvmClientTrait::<MockWeb3TransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 100u64);
}

#[tokio::test]
async fn test_get_blocks() {
	let mut mock = MockEvmClientTrait::<MockWeb3TransportClient>::new();

	let block = BlockType::EVM(Box::new(EVMBlock::from(web3::types::Block {
		number: Some(1u64.into()),
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
		BlockType::EVM(block) => assert_eq!(block.number, Some(1u64.into())),
		_ => panic!("Expected EVM block"),
	}
}

#[tokio::test]
async fn test_new_client() {
	let mut server = Server::new_async().await;

	let mock = create_evm_valid_server_mock_network_response(&mut server);
	// Create a test network
	let network = create_evm_test_network_with_urls(vec![&server.url()]);

	// Test successful client creation
	let result = EvmClient::new(&network).await;
	assert!(result.is_ok(), "Client creation should succeed");
	mock.assert();
}
