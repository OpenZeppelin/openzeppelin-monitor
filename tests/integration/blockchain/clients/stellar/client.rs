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
use serde_json::json;

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

	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	// Create a test network
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Test successful client creation
	let result = StellarClient::new(&network).await;
	assert!(result.is_ok(), "Client creation should succeed");
	mock.assert();
}

#[tokio::test]
async fn test_get_transactions_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"transactions": [
				{
				"status": "SUCCESS",
					"txHash": "1723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d",
					"applicationOrder": 1,
					"feeBump": false,
					"envelopeXdr": "CCC",
					"resultXdr": "BBB",
					"resultMetaXdr": "AAA",
					"ledger": 1,
					"createdAt": 1735440610
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"transactions": [
				{
					"status": "SUCCESS",
					"txHash": "2723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d",
					"applicationOrder": 1,
					"feeBump": false,
					"envelopeXdr": "CCC",
					"resultXdr": "BBB=",
					"resultMetaXdr": "AAA",
					"ledger": 2,
					"createdAt": 1735440610
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_transactions(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	assert_eq!(
		result[0].transaction_hash,
		"1723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d"
	);
	assert_eq!(
		result[1].transaction_hash,
		"2723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d"
	);

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_events_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"events": [
				{
					"type": "contract",
					"ledger": 1,
					"ledgerClosedAt": "2024-12-29T02:50:10Z",
					"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
					"id": "0001364073023291392-0000000001",
					"pagingToken": "0001364073023291392-0000000001",
					"inSuccessfulContractCall": true,
					"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
					"topic": [
					  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA",
					  "AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
					  "AAAADgAAAAlnYW5hY2hlLTAAAAA=",
					  "AAAADgAAACoweDY4QjkzMDQ1ZmU3RDg3OTRhN2NBRjMyN2U3Zjg1NUNENkNkMDNCQjgAAA==",
					  "AAAADQAAACAaemkIzyqB6sH3VVev7iSjYHderf04InYUVZQLYhCsdg=="
					],
					"value": "AAA"
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"events": [
				{
					"type": "contract",
					"ledger": 2,
					"ledgerClosedAt": "2024-12-29T02:50:10Z",
					"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
					"id": "0001364073023291392-0000000001",
					"pagingToken": "0001364073023291392-0000000001",
					"inSuccessfulContractCall": true,
					"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
					"topic": [
					  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA",
					  "AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
					  "AAAADgAAAAlnYW5hY2hlLTAAAAA=",
					  "AAAADgAAACoweDY4QjkzMDQ1ZmU3RDg3OTRhN2NBRjMyN2U3Zjg1NUNENkNkMDNCQjgAAA==",
					  "AAAADQAAACAaemkIzyqB6sH3VVev7iSjYHderf04InYUVZQLYhCsdg=="
					],
					"value": "AAA"
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_events(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	assert_eq!(result[0].ledger, 1);
	assert_eq!(result[1].ledger, 2);

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_blocks_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"ledgers": [
				{
					"hash": "eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f",
					"sequence": 1,
					"ledgerCloseTime": "1734715051",
					"headerXdr": "AAA",
					"metadataXdr": "BBB"
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"ledgers": [
				{
					"hash": "eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f",
					"sequence": 2,
					"ledgerCloseTime": "1734715051",
					"headerXdr": "AAA",
					"metadataXdr": "BBB"
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_blocks(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	match &result[0] {
		BlockType::Stellar(block) => assert_eq!(block.sequence, 1),
		_ => panic!("Expected Stellar block"),
	}
	match &result[1] {
		BlockType::Stellar(block) => assert_eq!(block.sequence, 2),
		_ => panic!("Expected Stellar block"),
	}

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}
